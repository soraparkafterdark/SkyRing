//! The first magic slice's spell matrix, driven end to end through the public
//! API only: every spell is cast through `Caster::begin` and `Caster::tick`
//! one 60 Hz tick at a time, registered in an `ImpactLedger`, resolved as a
//! native impact would be, and run through `ConsequenceState::resolve` with a
//! host-shaped `TargetRules`.
//!
//! The assertions are per channel: damage, status, and modifiers are
//! independent, and nothing exceeds the spell damage cap. All numbers are
//! fractions and ratios; Skyrim owns every absolute magnitude.

use er_combat_core::cast::{CastEvent, CastInput, CastRefusal, CastRequest, Caster, ImpactError, ImpactLedger};
use er_combat_core::consequences::{
    ConsequenceState, DamageElement, HitEvent, TargetRules,
};
use er_combat_core::effects::{ModifiedStat, StatusKind};
use er_combat_core::magic::{
    passive_bonus, spell_damage_multiplier, Catalyst, CatalystKind, Hand, MAX_SPELL_DAMAGE_MULTIPLIER,
    School, SpellDefinition, SpellDelivery, SpellSnapshot, MAX_UPGRADE_LEVEL, UPGRADE_STEP,
};
use er_combat_core::posture::{PoiseDefinition, StanceDefinition};
use er_combat_core::spells::{
    all_catalysts, all_spells, apprentice_staff, cinder_lance, ember_bolt, feeble_hex, haste,
    pyre_pillar, rime_sigil, sages_staff, solar_lance, sun_ring, HIGH_STAFF_QUALITY,
};

const DT: f32 = 1.0 / 60.0;
const MAGICKA: f32 = 100.0;

/// A target shaped like the host's: plain thresholds, neutral resistances.
fn target_rules() -> TargetRules {
    let status = er_combat_core::effects::StatusDefinition {
        threshold: 100.0,
        decay_delay_seconds: 2.0,
        decay_per_second: 10.0,
        proc_lockout_seconds: 1.0,
    };
    TargetRules {
        poise: PoiseDefinition {
            threshold: 50.0,
            recovery_delay_seconds: 1.0,
            recovery_per_second: 10.0,
        },
        stance: StanceDefinition {
            threshold: 100.0,
            decay_delay_seconds: 2.0,
            decay_per_second: 10.0,
            break_lockout_seconds: 1.0,
        },
        statuses: [status; er_combat_core::effects::STATUS_COUNT],
        status_received: [1.0; er_combat_core::effects::STATUS_COUNT],
        stance_received: 1.0,
    }
}

/// Casts `spell` from `catalyst` in `hand`, ticking one 60 Hz step at a time
/// until the cast finishes, and returns every event.
///
/// `held` is whether the cast button is down each tick: a charged spell
/// charges to full and auto-releases, anything else releases at its frame.
fn drive(spell: &SpellDefinition, catalyst: &Catalyst, hand: Hand, held: bool) -> Vec<CastEvent> {
    let mut caster = Caster::new(1);
    let started = caster
        .begin(CastRequest {
            hand,
            casting: catalyst,
            offhand: None,
            spell,
            skill: 0.0,
            magicka_current: MAGICKA,
            magicka_max: MAGICKA,
            cost_scale: 1.0,
        })
        .expect("a level-one caster with a full pool casts from a valid catalyst");
    let mut events = vec![started];
    for _ in 0..600 {
        if caster.is_idle() {
            break;
        }
        events.extend(caster.tick(
            DT,
            CastInput { held, dodge: false, poise_broken: false, magicka_current: MAGICKA },
        ));
    }
    assert!(
        matches!(events.last(), Some(CastEvent::Finished)),
        "{:?} must commit, release and recover: {events:?}",
        spell.id
    );
    events
}

/// The `Released` snapshot of a driven cast, asserting there is exactly one.
fn released(spell: &SpellDefinition, catalyst: &Catalyst, hand: Hand, held: bool) -> SpellSnapshot {
    let snapshots: Vec<SpellSnapshot> = drive(spell, catalyst, hand, held)
        .into_iter()
        .filter_map(|event| match event {
            CastEvent::Released(snapshot) => Some(snapshot),
            _ => None,
        })
        .collect();
    assert_eq!(snapshots.len(), 1, "{:?} must release exactly once", spell.id);
    snapshots.into_iter().next().unwrap()
}

/// A spell's multiplier for a caster with no off hand at skill `skill`.
fn multiplier_for(spell: &SpellDefinition, catalyst: &Catalyst, skill: f32) -> f32 {
    spell_damage_multiplier(spell, catalyst, None, skill, 1.0)
}

// ---------------------------------------------------------------------------
// Definition validity
// ---------------------------------------------------------------------------

#[test]
fn every_catalyst_and_spell_of_the_slice_is_valid() {
    for catalyst in all_catalysts() {
        assert!(catalyst.valid(), "{catalyst:?} must satisfy Catalyst::valid");
    }
    for spell in all_spells() {
        spell.validate().expect("every slice spell must validate");
        assert!(
            (0.02..=0.2).contains(&spell.cost_fraction),
            "{:?} cost_fraction {} must sit in 0.02..=0.2",
            spell.id,
            spell.cost_fraction
        );
        assert!(
            spell.power_ratio <= 1.0,
            "{:?} power_ratio {} is a fraction of scaling, never above 1.0",
            spell.id,
            spell.power_ratio
        );
        assert!(
            spell.timing.cancel_until <= spell.timing.release_at,
            "the cancel window must close at or before the release frame"
        );
        assert!(
            spell.timing.hyperarmour_from <= spell.timing.release_at
                && spell.timing.release_at <= spell.timing.hyperarmour_to,
            "hyperarmour must cover the release frame"
        );
    }
}

#[test]
fn the_seven_spells_cover_the_seven_deliveries_with_distinct_families() {
    let spells = all_spells();
    assert_eq!(spells.len(), 7);
    let deliveries: Vec<SpellDelivery> = spells.iter().map(|s| s.delivery).collect();
    assert!(deliveries.contains(&SpellDelivery::Instant));
    assert!(deliveries.contains(&SpellDelivery::Projectile));
    assert!(deliveries.contains(&SpellDelivery::Area));
    assert!(deliveries.contains(&SpellDelivery::SelfCast));
    assert!(spells.iter().any(|s| s.charge.is_some()), "one spell is a charged cast");
    assert!(spells.iter().any(|s| s.area.is_some()), "one spell is a persistent area");
    let mut families: Vec<_> = spells.iter().map(|s| s.family).collect();
    families.dedup();
    assert_eq!(families.len(), 7, "every spell owns a distinct family");
}

#[test]
fn destruction_straddles_both_catalyst_families() {
    // The Charter: staves and rings both carry Destruction, with different
    // spells on each side.
    assert!(ember_bolt().castable_with(&apprentice_staff()));
    assert!(!ember_bolt().castable_with(&sun_ring()));
    assert!(pyre_pillar().castable_with(&sun_ring()));
    assert!(pyre_pillar().castable_with(&apprentice_staff()));
    assert!(solar_lance().castable_with(&sun_ring()));
    assert!(!solar_lance().castable_with(&apprentice_staff()));
}

// ---------------------------------------------------------------------------
// The whole matrix through the real cast path
// ---------------------------------------------------------------------------

#[test]
fn every_spell_releases_and_resolves_within_the_damage_cap() {
    for spell in all_spells() {
        let catalyst = if spell.castable_with(&apprentice_staff()) { apprentice_staff() } else { sun_ring() };
        let hand = if catalyst.kind == CatalystKind::Ring { Hand::Left } else { Hand::Right };
        let snapshot = released(&spell, &catalyst, hand, false);
        assert!(
            snapshot.damage_multiplier <= MAX_SPELL_DAMAGE_MULTIPLIER,
            "{:?} multiplier {} exceeds the cap",
            snapshot.spell.id,
            snapshot.damage_multiplier
        );
        let mut ledger = ImpactLedger::default();
        let registered = ledger.register(snapshot.clone());
        assert_eq!(
            registered,
            snapshot.spell.delivery != SpellDelivery::SelfCast,
            "a self-cast has no ledger entry; everything else registers"
        );
        if registered {
            let event = ledger
                .resolve(snapshot.release_id, 77, 0)
                .expect("the first native impact resolves");
            let mut target = ConsequenceState::default();
            let outcome = target.resolve(&event, target_rules());
            assert!(
                outcome.skyrim_damage_multiplier <= MAX_SPELL_DAMAGE_MULTIPLIER,
                "{:?} resolved multiplier {} exceeds the cap",
                event.action_id,
                outcome.skyrim_damage_multiplier
            );
        }
    }
}

#[test]
fn the_damage_spells_land_nonzero_bounded_damage() {
    let damage_spells = [ember_bolt(), cinder_lance(), pyre_pillar(), solar_lance()];
    for spell in damage_spells {
        let catalyst = if spell.castable_with(&apprentice_staff()) { apprentice_staff() } else { sun_ring() };
        let hand = if catalyst.kind == CatalystKind::Ring { Hand::Left } else { Hand::Right };
        let snapshot = released(&spell, &catalyst, hand, false);
        assert!(snapshot.spell.payload.damages(), "{:?} carries a damage share", spell.id);

        let mut ledger = ImpactLedger::default();
        assert!(ledger.register(snapshot.clone()));
        let event: HitEvent = ledger.resolve(snapshot.release_id, 9, 0).unwrap();
        let mut target = ConsequenceState::default();
        let outcome = target.resolve(&event, target_rules());
        assert!(
            outcome.skyrim_damage_multiplier > 0.0,
            "{:?} must ask Skyrim for damage, got {}",
            spell.id,
            outcome.skyrim_damage_multiplier
        );
        assert!(outcome.skyrim_damage_multiplier <= MAX_SPELL_DAMAGE_MULTIPLIER);
    }
}

#[test]
fn the_debuff_deals_no_damage_and_applies_exactly_one_modifier() {
    let spell = feeble_hex();
    let snapshot = released(&spell, &apprentice_staff(), Hand::Right, false);
    let mut ledger = ImpactLedger::default();
    assert!(ledger.register(snapshot.clone()));
    let event = ledger.resolve(snapshot.release_id, 9, 0).unwrap();

    let mut target = ConsequenceState::default();
    let outcome = target.resolve(&event, target_rules());
    assert_eq!(
        outcome.skyrim_damage_multiplier, 0.0,
        "a pure hex must leave the damage channel at zero"
    );
    assert_eq!(outcome.modifiers_applied, 1, "the hex applies its one modifier");
    assert_eq!(target.modifiers.len(), 1);
    assert_eq!(
        target.modifiers.evaluate(ModifiedStat::DamageTaken, 1.0),
        1.25,
        "the hex raises damage taken by its stated factor"
    );

    // Reapplying the same hex follows its StackRule::Strongest: one instance,
    // refreshed duration, never a second stack.
    let again = ledger.resolve(snapshot.release_id, 9, 0);
    assert_eq!(again, Err(ImpactError::Duplicate));
}

#[test]
fn the_status_spell_builds_frostbite_without_touching_damage() {
    let spell = rime_sigil();
    let snapshot = released(&spell, &apprentice_staff(), Hand::Right, false);
    let mut ledger = ImpactLedger::default();
    assert!(ledger.register(snapshot.clone()));
    let event = ledger.resolve(snapshot.release_id, 9, 0).unwrap();

    let mut target = ConsequenceState::default();
    let outcome = target.resolve(&event, target_rules());
    assert_eq!(
        outcome.skyrim_damage_multiplier, 0.0,
        "a pure status spell must leave the damage channel at zero"
    );
    assert!(outcome.status_procs.iter().all(Option::is_none), "45 of 100 buildup does not proc");
    let frostbite = target.statuses.meter(StatusKind::Frostbite);
    assert!(
        frostbite.buildup() > 0.0 && frostbite.buildup() < 100.0,
        "the frostbite meter carries the spell's buildup: {}",
        frostbite.buildup()
    );
    for kind in StatusKind::ALL {
        if kind != StatusKind::Frostbite {
            assert_eq!(target.statuses.meter(kind).buildup(), 0.0, "{kind:?} stays untouched");
        }
    }
}

#[test]
fn the_buff_lands_on_the_caster_through_the_snapshot_not_the_ledger() {
    let spell = haste();
    let snapshot = released(&spell, &apprentice_staff(), Hand::Right, false);
    let mut ledger = ImpactLedger::default();
    assert!(
        !ledger.register(snapshot.clone()),
        "a self-cast registers nothing: there is no impact to correlate"
    );
    assert!(ledger.resolve(snapshot.release_id, 9, 0).is_err(), "a self-cast has no impact");

    let mut caster_state = ConsequenceState::default();
    let modifiers = snapshot.caster_modifiers();
    assert_eq!(modifiers.len(), 1, "the buff is exactly one caster modifier");
    for modifier in &modifiers {
        assert!(caster_state.modifiers.apply(*modifier));
    }
    assert_eq!(
        caster_state.modifiers.evaluate(ModifiedStat::DamageDealt, 1.0),
        1.25,
        "the buff multiplies damage dealt by its stated factor"
    );
    assert_eq!(
        caster_state.modifiers.evaluate(ModifiedStat::DamageTaken, 1.0),
        1.0,
        "a buff on the caster does not change what the caster takes"
    );
}

#[test]
fn the_charged_cast_reaches_full_charge_and_scales_by_the_charge_power() {
    let spell = solar_lance();
    let held = released(&spell, &sun_ring(), Hand::Left, true);
    assert!(
        (held.charge_fraction - 1.0).abs() < 1e-6,
        "holding to full charge releases at fraction 1.0, got {}",
        held.charge_fraction
    );
    let expected = spell.power_ratio * sun_ring().scaling(0.0) * 1.25 * 1.5;
    assert!(
        (held.damage_multiplier - expected).abs() < 1e-5,
        "full charge multiplies by power_at_full: {} vs {expected}",
        held.damage_multiplier
    );

    let unheld = released(&spell, &sun_ring(), Hand::Left, false);
    assert!(unheld.charge_fraction.abs() < 1e-6, "an unheld charged cast releases immediately");
    let expected_uncharged = spell.power_ratio * sun_ring().scaling(0.0) * 1.25;
    assert!((unheld.damage_multiplier - expected_uncharged).abs() < 1e-5);
    assert!(
        held.damage_multiplier > unheld.damage_multiplier,
        "charging is strictly stronger than tapping"
    );
    assert!(held.damage_multiplier <= MAX_SPELL_DAMAGE_MULTIPLIER);
}

// ---------------------------------------------------------------------------
// The gear axis
// ---------------------------------------------------------------------------

#[test]
fn a_level_one_character_casts_every_spell_at_quality_times_upgrade_scaling() {
    for spell in all_spells() {
        for catalyst in all_catalysts() {
            if !spell.castable_with(&catalyst) {
                continue;
            }
            // Skill 0 is a level-one character: eligibility is unconditional,
            // and scaling is exactly quality times the upgrade factor.
            let hand = if catalyst.kind == CatalystKind::Ring { Hand::Left } else { Hand::Right };
            let snapshot = released(&spell, &catalyst, hand, false);
            let upgrade = 1.0 + UPGRADE_STEP * f32::from(catalyst.upgrade_level.min(MAX_UPGRADE_LEVEL));
            let expected_scaling = catalyst.quality * upgrade;
            assert!(
                (catalyst.scaling(0.0) - expected_scaling).abs() < 1e-6,
                "skill 0 scaling must be quality × upgrade: {} vs {expected_scaling}",
                catalyst.scaling(0.0)
            );
            let passive = 1.0 + passive_bonus(spell.family, &catalyst, None);
            let expected = spell.power_ratio * expected_scaling * passive;
            assert!(
                (snapshot.damage_multiplier - expected).abs() < 1e-5,
                "{:?} from {:?}: multiplier {} vs {expected}",
                spell.id,
                catalyst.id,
                snapshot.damage_multiplier
            );
        }
    }
}

#[test]
fn the_high_scaling_staff_beats_the_baseline_by_exactly_the_quality_ratio() {
    for skill in [0.0_f32, 50.0, 100.0] {
        for spell in [ember_bolt(), cinder_lance()] {
            let baseline = multiplier_for(&spell, &apprentice_staff(), skill);
            let high = multiplier_for(&spell, &sages_staff(), skill);
            assert!(
                high < MAX_SPELL_DAMAGE_MULTIPLIER,
                "the comparison must sit below the cap for skill {skill}"
            );
            let ratio = high / baseline;
            assert!(
                (ratio - HIGH_STAFF_QUALITY).abs() < 1e-4,
                "skill {skill}: ratio {ratio} must equal the quality ratio {}",
                HIGH_STAFF_QUALITY
            );
        }
    }
}

#[test]
fn the_ring_passive_boosts_only_its_own_family() {
    let bare = Catalyst {
        id: 30,
        kind: CatalystKind::Ring,
        school: School::Restoration,
        quality: 1.0,
        upgrade_level: 0,
        passive: None,
        cost_multiplier: 1.0,
    };
    let ring = sun_ring();

    // Its own family: exactly the stated bonus.
    let boosted = multiplier_for(&solar_lance(), &ring, 0.0);
    let plain = multiplier_for(&solar_lance(), &bare, 0.0);
    assert!((boosted / plain - 1.25).abs() < 1e-5, "sunlight gains exactly +25%: {boosted} vs {plain}");

    // Any other family: identical, so the passive is not a general power bump.
    for spell in [ember_bolt(), cinder_lance(), pyre_pillar()] {
        if !spell.castable_with(&bare) {
            continue;
        }
        let with_passive = multiplier_for(&spell, &ring, 0.0);
        let without = multiplier_for(&spell, &bare, 0.0);
        assert!(
            (with_passive - without).abs() < 1e-6,
            "{:?} is outside the passive's family and must not gain",
            spell.id
        );
    }
}

// ---------------------------------------------------------------------------
// Ledger discipline
// ---------------------------------------------------------------------------

#[test]
fn a_second_resolve_of_the_same_release_and_target_is_a_duplicate() {
    for spell in [ember_bolt(), cinder_lance()] {
        let snapshot = released(&spell, &apprentice_staff(), Hand::Right, false);
        let mut ledger = ImpactLedger::default();
        assert!(ledger.register(snapshot.clone()));
        let first = ledger.resolve(snapshot.release_id, 9, 0);
        assert!(first.is_ok());
        let second = ledger.resolve(snapshot.release_id, 9, 0);
        assert_eq!(second, Err(ImpactError::Duplicate), "{:?} resolves once per target", spell.id);
    }
}

#[test]
fn the_area_resolves_once_per_tick_and_refuses_ticks_past_its_life() {
    let spell = pyre_pillar();
    let snapshot = released(&spell, &apprentice_staff(), Hand::Right, false);
    let mut ledger = ImpactLedger::default();
    assert!(ledger.register(snapshot.clone()));
    let area = spell.area.expect("the area spell carries an Area");
    let ticks = area.ticks();
    assert!(ticks >= 2, "the slice's area ticks more than once: {ticks}");

    for tick in 0..ticks {
        let event = ledger
            .resolve(snapshot.release_id, 9, tick)
            .expect("each in-life tick resolves once");
        assert_eq!(event.hit_id, tick + 1, "an area tick names its own resolution");
        assert_eq!(
            ledger.resolve(snapshot.release_id, 9, tick),
            Err(ImpactError::Duplicate),
            "tick {tick} already resolved on this target"
        );
    }
    assert_eq!(
        ledger.resolve(snapshot.release_id, 9, ticks),
        Err(ImpactError::TickOutOfRange),
        "a tick past the area's life is not an impact"
    );
}

#[test]
fn equipment_changes_after_release_do_not_alter_the_snapshot() {
    let spell = cinder_lance();
    let snapshot = released(&spell, &apprentice_staff(), Hand::Right, false);
    let before = snapshot.clone();

    // The caster swaps to a fully upgraded high-scaling staff mid-flight.
    let upgraded = Catalyst {
        id: 40,
        kind: CatalystKind::Staff,
        school: School::Destruction,
        quality: HIGH_STAFF_QUALITY,
        upgrade_level: MAX_UPGRADE_LEVEL,
        passive: None,
        cost_multiplier: 1.5,
    };
    let reforged = spell_damage_multiplier(&spell, &upgraded, None, 100.0, 1.0);
    assert!(
        reforged > snapshot.damage_multiplier,
        "the new gear would cast harder: {reforged} > {}",
        snapshot.damage_multiplier
    );
    assert_eq!(snapshot, before, "the frozen snapshot is unchanged by the swap");
    assert!(
        (snapshot.damage_multiplier - multiplier_for(&spell, &apprentice_staff(), 0.0)).abs() < 1e-6,
        "the snapshot still carries the multiplier of the catalyst that cast it"
    );

    // And the impact it already produced resolves on the frozen terms.
    let mut ledger = ImpactLedger::default();
    assert!(ledger.register(snapshot.clone()));
    let event = ledger.resolve(snapshot.release_id, 9, 0).unwrap();
    assert!((event.damage_multiplier - before.damage_multiplier).abs() < 1e-6);
}

// ---------------------------------------------------------------------------
// Commitment: the ring dedicates a hand exactly as a staff does
// ---------------------------------------------------------------------------

#[test]
fn a_ring_cast_dedicates_the_casting_hand_until_finished() {
    let spell = solar_lance();
    let ring = sun_ring();
    let mut caster = Caster::new(5);
    caster
        .begin(CastRequest {
            hand: Hand::Left,
            casting: &ring,
            offhand: None,
            spell: &spell,
            skill: 0.0,
            magicka_current: MAGICKA,
            magicka_max: MAGICKA,
            cost_scale: 1.0,
        })
        .expect("the ring casts");
    assert_eq!(caster.dedicated_hand(), Some(Hand::Left), "startup dedicates the hand");

    let mut released = false;
    for _ in 0..600 {
        if caster.is_idle() {
            break;
        }
        let events = caster.tick(
            DT,
            CastInput { held: true, dodge: false, poise_broken: false, magicka_current: MAGICKA },
        );
        if events.iter().any(|event| matches!(event, CastEvent::Released(_))) {
            released = true;
            // Through charge and recovery the hand stays dedicated: it is not
            // blocking, attacking or two-handing.
            assert_eq!(
                caster.dedicated_hand(),
                Some(Hand::Left),
                "release does not free the hand while the cast is live"
            );
        }
    }
    assert!(released, "the held charged cast must release");
    assert!(caster.is_idle(), "the cast must finish");
    assert_eq!(caster.dedicated_hand(), None, "only Finished frees the hand");

    // And the freed caster can begin again: one cast at a time, never a lock.
    caster
        .begin(CastRequest {
            hand: Hand::Left,
            casting: &ring,
            offhand: None,
            spell: &spell,
            skill: 0.0,
            magicka_current: MAGICKA,
            magicka_max: MAGICKA,
            cost_scale: 1.0,
        })
        .expect("a finished cast frees the caster for the next one");
}

#[test]
fn a_second_begin_during_a_live_cast_is_busy_not_queued() {
    let spell = ember_bolt();
    let staff = apprentice_staff();
    let mut caster = Caster::new(6);
    caster
        .begin(CastRequest {
            hand: Hand::Right,
            casting: &staff,
            offhand: None,
            spell: &spell,
            skill: 0.0,
            magicka_current: MAGICKA,
            magicka_max: MAGICKA,
            cost_scale: 1.0,
        })
        .unwrap();
    let second = caster.begin(CastRequest {
        hand: Hand::Right,
        casting: &staff,
        offhand: None,
        spell: &spell,
        skill: 0.0,
        magicka_current: MAGICKA,
        magicka_max: MAGICKA,
        cost_scale: 1.0,
    });
    assert_eq!(second, Err(CastRefusal::Busy));
}

#[test]
fn the_wrong_catalyst_family_is_the_only_refusal_that_is_not_about_resources() {
    // A level-one character (skill 0, empty perks) is refused only by family
    // fit or pool: never by skill, level, perk or attribute.
    let solar = solar_lance();
    let refusal = Caster::new(7).begin(CastRequest {
        hand: Hand::Right,
        casting: &apprentice_staff(),
        offhand: None,
        spell: &solar,
        skill: 0.0,
        magicka_current: MAGICKA,
        magicka_max: MAGICKA,
        cost_scale: 1.0,
    });
    assert_eq!(refusal, Err(CastRefusal::WrongCatalystFamily));

    let broke = Caster::new(7).begin(CastRequest {
        hand: Hand::Left,
        casting: &sun_ring(),
        offhand: None,
        spell: &solar,
        skill: 0.0,
        magicka_current: 0.0,
        magicka_max: MAGICKA,
        cost_scale: 1.0,
    });
    assert!(matches!(broke, Err(CastRefusal::NotEnoughMagicka { .. })));
}

#[test]
fn the_element_shares_reach_skyrim_through_the_shared_contract() {
    // Fire spells ask for fire; the sunlight lance asks for holy, which stays
    // its own channel in the shared contract.
    let fire = released(&ember_bolt(), &apprentice_staff(), Hand::Right, false);
    assert!((fire.spell.payload.elements.share(DamageElement::Fire) - 1.0).abs() < 1e-6);
    let holy = released(&solar_lance(), &sun_ring(), Hand::Left, false);
    assert!((holy.spell.payload.elements.share(DamageElement::Holy) - 1.0).abs() < 1e-6);
    assert!(holy.spell.payload.elements.share(DamageElement::Fire) == 0.0);
}

#[test]
fn magicka_costs_are_fractions_of_the_pool_scaled_by_the_catalyst_trade() {
    // The high-scaling staff's 1.5× cost trade is visible in host units.
    let spell = cinder_lance();
    let baseline = spell.magicka_cost(&apprentice_staff(), 1.0, MAGICKA);
    let traded = spell.magicka_cost(&sages_staff(), 1.0, MAGICKA);
    assert!((baseline - 0.12 * MAGICKA).abs() < 1e-5);
    assert!((traded - 1.5 * baseline).abs() < 1e-5);
    // The reported spend matches: the core never owns the pool itself.
    let snapshot_cast = drive(&spell, &sages_staff(), Hand::Right, false);
    assert!(snapshot_cast.iter().any(|event| matches!(event, CastEvent::Released(_))));
    let mut caster = Caster::new(11);
    caster
        .begin(CastRequest {
            hand: Hand::Right,
            casting: &sages_staff(),
            offhand: None,
            spell: &spell,
            skill: 0.0,
            magicka_current: MAGICKA,
            magicka_max: MAGICKA,
            cost_scale: 1.0,
        })
        .unwrap();
    for _ in 0..600 {
        if caster.is_idle() {
            break;
        }
        caster.tick(
            DT,
            CastInput { held: false, dodge: false, poise_broken: false, magicka_current: MAGICKA },
        );
    }
    assert!((caster.take_spent() - traded).abs() < 1e-4, "the cast reports its own cost");
    assert_eq!(caster.take_spent(), 0.0, "spend is reported once, not accumulated");
}
