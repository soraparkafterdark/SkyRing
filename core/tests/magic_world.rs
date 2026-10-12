//! Magic through the shared effect pipeline: casters, releases in flight, native impacts,
//! buffs and debuffs, statuses, and actor lifecycles.

use std::collections::BTreeMap;

use er_combat_core::cast::{CastInput, CastRefusal, ImpactError, PROJECTILE_MAX_FLIGHT_SECONDS};
use er_combat_core::consequences::{DamageElement, ElementalProfile, HitOutcome, TargetRules};
use er_combat_core::effects::{ModifiedStat, ModifierOperation, StackRule, StatusDefinition, StatusKind, STATUS_COUNT};
use er_combat_core::magic::*;
use er_combat_core::magic_world::*;
use er_combat_core::posture::{PoiseDefinition, PoiseResult, StanceDefinition};

const DT: f32 = 1.0 / 60.0;
const CASTER: u32 = 1;
const TARGET: u32 = 2;
const OTHER: u32 = 3;

fn rules() -> TargetRules {
    TargetRules {
        poise: PoiseDefinition { threshold: 100.0, recovery_delay_seconds: 2.0, recovery_per_second: 20.0 },
        stance: StanceDefinition { threshold: 100.0, decay_delay_seconds: 2.0, decay_per_second: 10.0, break_lockout_seconds: 3.0 },
        statuses: [StatusDefinition { threshold: 100.0, decay_delay_seconds: 2.0, decay_per_second: 10.0, proc_lockout_seconds: 5.0 }; STATUS_COUNT],
        status_received: [1.0; STATUS_COUNT],
        stance_received: 1.0,
    }
}

fn world() -> MagicWorld {
    let mut w = MagicWorld::new();
    for actor in [CASTER, TARGET, OTHER] {
        w.spawn(actor, rules()).unwrap();
    }
    w
}

fn staff(quality: f32) -> Catalyst {
    Catalyst { id: 1, kind: CatalystKind::Staff, school: School::Destruction, quality, upgrade_level: 0, passive: None, cost_multiplier: 1.0 }
}

fn timing() -> CastTiming {
    CastTiming { release_at: 0.5, cancel_until: 0.2, recovery: 0.4, hyperarmour_from: 0.3, hyperarmour_to: 0.6 }
}

fn fire() -> ElementalProfile {
    ElementalProfile::from_channels(&[(DamageElement::Fire, 1.0)]).unwrap()
}

fn none() -> SpellPayload {
    SpellPayload { elements: ElementalProfile::default(), poise_pressure: 0.0, stance_pressure: 0.0, status_buildup: [0.0; STATUS_COUNT], modifiers: vec![] }
}

fn bolt() -> SpellDefinition {
    SpellDefinition {
        id: 100,
        school: School::Destruction,
        family: SpellFamily(7),
        delivery: SpellDelivery::Projectile,
        castable_from: FROM_STAFF | FROM_RING,
        power_ratio: 0.8,
        cost_fraction: 0.1,
        timing: timing(),
        charge: None,
        area: None,
        payload: SpellPayload { elements: fire(), poise_pressure: 10.0, stance_pressure: 5.0, ..none() },
    }
}

fn modifier(on: ModifierTarget, stat: ModifiedStat, operation: ModifierOperation, magnitude: f32, group: u32, duration: f32) -> ModifierTemplate {
    ModifierTemplate { on, group, stat, operation, magnitude, duration, priority: 0, stack_rule: StackRule::Refresh }
}

fn debuff() -> SpellDefinition {
    SpellDefinition {
        id: 101,
        delivery: SpellDelivery::Instant,
        power_ratio: 0.0,
        payload: SpellPayload {
            modifiers: vec![modifier(ModifierTarget::Target, ModifiedStat::DamageTaken, ModifierOperation::Multiply, 1.1, 5, 12.0)],
            ..none()
        },
        ..bolt()
    }
}

fn frost() -> SpellDefinition {
    let mut buildup = [0.0; STATUS_COUNT];
    buildup[StatusKind::Frostbite.index()] = 40.0;
    SpellDefinition { id: 102, delivery: SpellDelivery::Instant, power_ratio: 0.0, payload: SpellPayload { status_buildup: buildup, ..none() }, ..bolt() }
}

fn buff() -> SpellDefinition {
    SpellDefinition {
        id: 103,
        delivery: SpellDelivery::SelfCast,
        power_ratio: 0.0,
        payload: SpellPayload {
            modifiers: vec![
                modifier(ModifierTarget::Caster, ModifiedStat::MagickaCost, ModifierOperation::Multiply, 0.5, 1, 20.0),
                modifier(ModifierTarget::Caster, ModifiedStat::CastSpeed, ModifierOperation::Multiply, 2.0, 2, 20.0),
                modifier(ModifierTarget::Caster, ModifiedStat::DamageDealt, ModifierOperation::Multiply, 1.5, 3, 20.0),
            ],
            ..none()
        },
        ..bolt()
    }
}

fn zone() -> SpellDefinition {
    SpellDefinition { id: 104, delivery: SpellDelivery::Area, area: Some(Area { radius: 4.0, duration: 3.0, tick_interval: 1.0 }), ..bolt() }
}

fn spec(spell: SpellDefinition) -> CastSpec {
    CastSpec { hand: Hand::Right, casting: staff(1.0), offhand: None, spell, skill: 0.0, magicka_current: 100.0, magicka_max: 100.0 }
}

fn idle() -> BTreeMap<u32, CastInput> {
    BTreeMap::new()
}

fn step_for(w: &mut MagicWorld, seconds: f32) {
    let ticks = (seconds / DT).round() as usize;
    for _ in 0..ticks {
        w.step(DT, &idle());
    }
}

/// Begins `cast` for `actor` and steps until it releases; returns (release id, ticks taken).
fn cast(w: &mut MagicWorld, actor: u32, cast: CastSpec) -> (u64, usize) {
    assert_eq!(w.begin_cast(actor, &cast), CastStart::Started);
    let start = w.events().len();
    for tick in 1..=600 {
        w.step(DT, &idle());
        for event in &w.events()[start..] {
            if let MagicEvent::SpellReleased { actor: who, release_id, .. } = event {
                if *who == actor {
                    return (*release_id, tick);
                }
            }
        }
    }
    panic!("the cast never released");
}

/// Steps until the actor's current cast has ended.
fn finish(w: &mut MagicWorld) {
    step_for(w, 1.0);
}

#[test]
fn a_projectile_impact_is_the_same_outcome_melee_would_produce() {
    let mut w = world();
    let (id, _) = cast(&mut w, CASTER, spec(bolt()));
    let outcome = w.native_impact(id, TARGET, 0).unwrap();
    assert!((outcome.skyrim_damage_multiplier - 0.8).abs() < 1e-6, "ratio 0.8 of scaling 1.0");
    assert_eq!(outcome.elements, fire());
    assert_eq!(outcome.poise, PoiseResult::Withstood { accumulated: 10.0, threshold: 100.0 });
    assert_eq!(w.state(TARGET).unwrap().poise.accumulated(), 10.0);
    assert_eq!(w.state(OTHER).unwrap().poise.accumulated(), 0.0, "only the struck target is affected");
    assert_eq!(w.take_magicka_spent(CASTER), 10.0);
    assert_eq!(w.take_magicka_spent(CASTER), 0.0, "reported once");
}

#[test]
fn an_impact_is_refused_for_unknown_and_dead_targets_without_consuming_the_release() {
    let mut w = world();
    let (id, _) = cast(&mut w, CASTER, spec(bolt()));
    assert_eq!(w.native_impact(id, 99, 0).unwrap_err(), ImpactRefusal::UnknownTarget);
    w.kill(TARGET).unwrap();
    assert_eq!(w.native_impact(id, TARGET, 0).unwrap_err(), ImpactRefusal::TargetDead);
    assert_eq!(w.releases_in_flight(), 1, "refusals do not consume the release");
    w.respawn(TARGET).unwrap();
    assert!(w.native_impact(id, TARGET, 0).is_ok(), "the same release can still resolve");
    assert_eq!(w.native_impact(id, TARGET, 0).unwrap_err(), ImpactRefusal::Ledger(ImpactError::Duplicate));
    assert_eq!(w.native_impact(id + 1, TARGET, 0).unwrap_err(), ImpactRefusal::Ledger(ImpactError::Unknown));
}

#[test]
fn a_debuff_amplifies_the_next_hit_from_any_delivery_until_it_expires() {
    let mut w = world();
    let (id, _) = cast(&mut w, CASTER, spec(debuff()));
    let applied = w.native_impact(id, TARGET, 0).unwrap();
    assert_eq!(applied.modifiers_applied, 1);
    assert_eq!(applied.skyrim_damage_multiplier, 0.0, "the debuff itself asks for no damage");
    finish(&mut w);

    let (bolt_id, _) = cast(&mut w, CASTER, spec(bolt()));
    let amplified = w.native_impact(bolt_id, TARGET, 0).unwrap();
    assert!((amplified.skyrim_damage_multiplier - 0.8 * 1.1).abs() < 1e-5, "{}", amplified.skyrim_damage_multiplier);
    finish(&mut w);
    let unaffected = {
        let (second, _) = cast(&mut w, CASTER, spec(bolt()));
        w.native_impact(second, OTHER, 0).unwrap()
    };
    assert!((unaffected.skyrim_damage_multiplier - 0.8).abs() < 1e-5, "another actor is not debuffed");

    finish(&mut w);
    step_for(&mut w, 13.0);
    assert!(w.state(TARGET).unwrap().modifiers.is_empty(), "the 12 s debuff expired");
    let (third, _) = cast(&mut w, CASTER, spec(bolt()));
    let after = w.native_impact(third, TARGET, 0).unwrap();
    assert!((after.skyrim_damage_multiplier - 0.8).abs() < 1e-5);
}

#[test]
fn a_buff_makes_the_next_cast_cheaper_faster_and_stronger() {
    // Baseline on a fresh world.
    let mut base = world();
    let (base_id, base_ticks) = cast(&mut base, CASTER, spec(bolt()));
    let base_hit = base.native_impact(base_id, TARGET, 0).unwrap();
    let base_cost = base.take_magicka_spent(CASTER);

    let mut w = world();
    let (buff_id, _) = cast(&mut w, CASTER, spec(buff()));
    assert!(w.native_impact(buff_id, TARGET, 0).is_err(), "a self-cast has no impact");
    assert_eq!(w.releases_in_flight(), 0, "nothing to correlate");
    assert_eq!(w.state(CASTER).unwrap().modifiers.len(), 3, "the buff sits on the caster");
    assert_eq!(w.state(TARGET).unwrap().modifiers.len(), 0);
    finish(&mut w);
    w.take_magicka_spent(CASTER);

    let (id, ticks) = cast(&mut w, CASTER, spec(bolt()));
    let hit = w.native_impact(id, TARGET, 0).unwrap();
    let cost = w.take_magicka_spent(CASTER);
    assert!((cost - base_cost * 0.5).abs() < 1e-4, "half the magicka: {cost} vs {base_cost}");
    assert!(ticks * 2 <= base_ticks + 2, "twice the cast speed: {ticks} ticks vs {base_ticks}");
    assert!((hit.skyrim_damage_multiplier / base_hit.skyrim_damage_multiplier - 1.5).abs() < 1e-4, "1.5x power");
}

#[test]
fn a_cost_buff_may_cheapen_casting_but_never_make_it_free() {
    let mut w = world();
    let mut free = buff();
    free.payload.modifiers = vec![modifier(ModifierTarget::Caster, ModifiedStat::MagickaCost, ModifierOperation::Multiply, 0.0, 1, 20.0)];
    cast(&mut w, CASTER, spec(free));
    finish(&mut w);
    w.take_magicka_spent(CASTER);
    cast(&mut w, CASTER, spec(bolt()));
    let cost = w.take_magicka_spent(CASTER);
    assert!((cost - 1.0).abs() < 1e-4, "clamped to a tenth of the normal 10: {cost}");
}

#[test]
fn status_buildup_accumulates_procs_locks_out_and_decays_like_any_delivery() {
    let mut w = world();
    let mut procs = Vec::new();
    for round in 0..4 {
        let (id, _) = cast(&mut w, CASTER, spec(frost()));
        let outcome = w.native_impact(id, TARGET, 0).unwrap();
        procs.push(outcome.status_procs[StatusKind::Frostbite.index()].is_some());
        finish(&mut w);
        let buildup = w.state(TARGET).unwrap().statuses.meter(StatusKind::Frostbite).buildup();
        match round {
            0 => assert!((buildup - 40.0).abs() < 1.0, "{buildup}"),
            1 => assert!((buildup - 80.0).abs() < 4.0, "{buildup}"),
            2 => assert_eq!(buildup, 0.0, "the proc resets the meter"),
            _ => assert_eq!(buildup, 0.0, "buildup during the lockout is ignored"),
        }
    }
    assert_eq!(procs, vec![false, false, true, false]);
    let mask = w.events().iter().filter_map(|e| if let MagicEvent::ImpactResolved { status_procs, .. } = e { Some(*status_procs) } else { None }).collect::<Vec<_>>();
    assert_eq!(mask[2], 1 << StatusKind::Frostbite.index());
    // Out of lockout, buildup returns and then decays on its own.
    step_for(&mut w, 6.0);
    let (id, _) = cast(&mut w, CASTER, spec(frost()));
    w.native_impact(id, TARGET, 0).unwrap();
    let fresh = w.state(TARGET).unwrap().statuses.meter(StatusKind::Frostbite).buildup();
    assert!(fresh > 30.0, "{fresh}");
    step_for(&mut w, 8.0);
    assert!(w.state(TARGET).unwrap().statuses.meter(StatusKind::Frostbite).buildup() < fresh - 20.0, "decayed");
}

#[test]
fn nothing_carries_over_a_death_a_respawn_or_a_form_reuse() {
    let mut w = world();
    let (d, _) = cast(&mut w, CASTER, spec(debuff()));
    w.native_impact(d, TARGET, 0).unwrap();
    finish(&mut w);
    let (f, _) = cast(&mut w, CASTER, spec(frost()));
    w.native_impact(f, TARGET, 0).unwrap();
    finish(&mut w);
    let (b, _) = cast(&mut w, CASTER, spec(bolt()));
    w.native_impact(b, TARGET, 0).unwrap();
    finish(&mut w);
    let state = w.state(TARGET).unwrap();
    assert!(!state.modifiers.is_empty() && state.statuses.meter(StatusKind::Frostbite).buildup() > 0.0 && state.poise.accumulated() > 0.0);

    w.kill(TARGET).unwrap();
    for state in [w.state(TARGET).unwrap()] {
        assert!(state.modifiers.is_empty());
        assert_eq!(state.statuses.meter(StatusKind::Frostbite).buildup(), 0.0);
        assert_eq!(state.poise.accumulated(), 0.0);
    }
    w.respawn(TARGET).unwrap();
    let state = w.state(TARGET).unwrap();
    assert!(state.modifiers.is_empty() && state.poise.accumulated() == 0.0);

    // Form reuse: the id is unloaded and a different actor takes it.
    finish(&mut w);
    let (d2, _) = cast(&mut w, CASTER, spec(debuff()));
    w.native_impact(d2, OTHER, 0).unwrap();
    w.unload(OTHER).unwrap();
    assert!(w.state(OTHER).is_none());
    w.spawn(OTHER, rules()).unwrap();
    assert!(w.state(OTHER).unwrap().modifiers.is_empty(), "the new actor inherits nothing");
    assert_eq!(w.spawn(OTHER, rules()).unwrap_err(), WorldError::ActorExists);
}

#[test]
fn a_dead_actor_cannot_cast_and_death_ends_a_cast_in_progress() {
    let mut w = world();
    assert_eq!(w.begin_cast(CASTER, &spec(bolt())), CastStart::Started);
    step_for(&mut w, 0.2);
    assert_eq!(w.dedicated_hand(CASTER), Some(Hand::Right));
    w.kill(CASTER).unwrap();
    assert_eq!(w.dedicated_hand(CASTER), None);
    assert_eq!(w.begin_cast(CASTER, &spec(bolt())), CastStart::Failed(WorldError::ActorDead));
    assert_eq!(w.begin_cast(99, &spec(bolt())), CastStart::Failed(WorldError::UnknownActor));
    step_for(&mut w, 2.0);
    assert!(!w.events().iter().any(|e| matches!(e, MagicEvent::SpellReleased { .. })), "a dead actor released nothing");
    w.respawn(CASTER).unwrap();
    assert_eq!(w.begin_cast(CASTER, &spec(bolt())), CastStart::Started);
}

/// A slow cast (2 s to release) whose hyperarmour covers `armour`.
fn slow(armour: (f32, f32)) -> SpellDefinition {
    SpellDefinition {
        id: 110,
        timing: CastTiming { release_at: 2.0, cancel_until: 0.2, recovery: 0.4, hyperarmour_from: armour.0, hyperarmour_to: armour.1 },
        ..bolt()
    }
}

fn target_casting_when_struck(armour: (f32, f32)) -> (MagicWorld, HitOutcome) {
    let mut weak = rules();
    weak.poise.threshold = 5.0;
    let mut w = MagicWorld::new();
    w.spawn(CASTER, rules()).unwrap();
    w.spawn(TARGET, weak).unwrap();
    assert_eq!(w.begin_cast(TARGET, &spec(slow(armour))), CastStart::Started);
    let (id, ticks) = cast(&mut w, CASTER, spec(bolt()));
    assert!(ticks < 40, "the strike lands well inside the target's startup");
    let outcome = w.native_impact(id, TARGET, 0).unwrap();
    (w, outcome)
}

#[test]
fn a_poise_break_interrupts_the_targets_own_cast_outside_hyperarmour() {
    let (mut w, outcome) = target_casting_when_struck((1.0, 1.5));
    assert!(matches!(outcome.poise, PoiseResult::Interrupted { .. }));
    w.step(DT, &idle());
    assert!(w.events().iter().any(|e| matches!(e, MagicEvent::CastEnded { actor: TARGET, how: CastEnd::Interrupted, .. })));
    assert_eq!(w.dedicated_hand(TARGET), None, "the hand is free again");
    assert_eq!(w.take_magicka_spent(TARGET), 0.0, "interrupted before release costs nothing");
}

#[test]
fn hyperarmour_carries_a_cast_through_a_poise_break() {
    // The strike lands about half a second in; hyperarmour covers 0.3 s to 1.0 s.
    let (mut w, outcome) = target_casting_when_struck((0.3, 1.0));
    assert!(matches!(outcome.poise, PoiseResult::Interrupted { .. }), "poise still broke");
    w.step(DT, &idle());
    assert!(!w.events().iter().any(|e| matches!(e, MagicEvent::CastEnded { actor: TARGET, .. })));
    assert_eq!(w.dedicated_hand(TARGET), Some(Hand::Right), "the cast carries on");
    step_for(&mut w, 3.0);
    assert!(w.events().iter().any(|e| matches!(e, MagicEvent::SpellReleased { actor: TARGET, .. })), "and releases");
}

#[test]
fn an_area_resolves_each_target_once_per_tick_and_stops_for_the_dead() {
    let mut w = world();
    let (id, _) = cast(&mut w, CASTER, spec(zone()));
    for tick in 0..3u32 {
        assert!(w.native_impact(id, TARGET, tick).is_ok(), "tick {tick}");
        if tick == 1 {
            w.kill(OTHER).unwrap();
        }
        let other = w.native_impact(id, OTHER, tick);
        if tick == 0 {
            assert!(other.is_ok());
        } else {
            assert_eq!(other.unwrap_err(), ImpactRefusal::TargetDead, "the dead take no more ticks");
        }
        assert_eq!(w.native_impact(id, TARGET, tick).unwrap_err(), ImpactRefusal::Ledger(ImpactError::Duplicate));
    }
    assert_eq!(w.native_impact(id, TARGET, 3).unwrap_err(), ImpactRefusal::Ledger(ImpactError::TickOutOfRange));
}

#[test]
fn release_ids_stay_unique_when_a_caster_unloads_with_a_projectile_in_flight() {
    let mut w = world();
    let (first, _) = cast(&mut w, CASTER, spec(bolt()));
    w.unload(CASTER).unwrap();
    // The projectile keeps flying and still resolves.
    assert!(w.native_impact(first, TARGET, 0).is_ok());
    w.spawn(CASTER, rules()).unwrap();
    let (second, _) = cast(&mut w, CASTER, spec(bolt()));
    assert_ne!(first, second, "a respawned caster does not reuse a release id");
    assert!(w.native_impact(second, TARGET, 0).is_ok());
}

#[test]
fn refusals_and_fizzles_are_recorded_and_cost_nothing() {
    let mut w = world();
    let low = CastSpec { magicka_current: 3.0, ..spec(bolt()) };
    assert!(matches!(w.begin_cast(CASTER, &low), CastStart::Refused(CastRefusal::NotEnoughMagicka { .. })));
    assert!(w.events().iter().any(|e| matches!(e, MagicEvent::CastRefused { actor: CASTER, spell: 100, .. })));
    assert_eq!(w.take_magicka_spent(CASTER), 0.0);
    // The pool drops mid-cast: it fizzles at the release frame.
    assert_eq!(w.begin_cast(CASTER, &spec(bolt())), CastStart::Started);
    let mut drained = BTreeMap::new();
    drained.insert(CASTER, CastInput { magicka_current: 1.0, ..Default::default() });
    for _ in 0..40 {
        w.step(DT, &drained);
    }
    assert!(w.events().iter().any(|e| matches!(e, MagicEvent::CastEnded { actor: CASTER, how: CastEnd::Fizzled, .. })));
    assert_eq!(w.take_magicka_spent(CASTER), 0.0);
    assert_eq!(w.releases_in_flight(), 0);
}

#[test]
fn dodge_is_refused_past_the_cancel_window_through_the_world() {
    let mut w = world();
    assert!(w.dodge_allowed(CASTER));
    w.begin_cast(CASTER, &spec(bolt()));
    assert!(w.dodge_allowed(CASTER));
    step_for(&mut w, 0.3);
    assert!(!w.dodge_allowed(CASTER), "committed");
    step_for(&mut w, 2.0);
    assert!(w.dodge_allowed(CASTER), "control returned");
}

#[test]
fn the_whole_scripted_fight_is_deterministic() {
    let run = || {
        let mut w = world();
        for spell in [debuff(), frost(), bolt(), buff(), bolt(), zone()] {
            let (id, _) = cast(&mut w, CASTER, spec(spell));
            for tick in 0..3 {
                let _ = w.native_impact(id, TARGET, tick);
                let _ = w.native_impact(id, OTHER, tick);
            }
            finish(&mut w);
        }
        (w.events().to_vec(), w.take_magicka_spent(CASTER).to_bits())
    };
    let (a, spent_a) = run();
    let (b, spent_b) = run();
    assert_eq!(a, b);
    assert_eq!(spent_a, spent_b);
    assert!(a.len() > 40, "the script exercised the world: {}", a.len());
}

#[test]
fn nonsense_steps_change_nothing() {
    let mut w = world();
    w.begin_cast(CASTER, &spec(bolt()));
    w.step(f32::NAN, &idle());
    w.step(-3.0, &idle());
    assert_eq!(w.dedicated_hand(CASTER), Some(Hand::Right));
    assert!(!w.events().iter().any(|e| matches!(e, MagicEvent::SpellReleased { .. })));
}

#[test]
fn a_miss_expires_with_the_world_and_a_late_impact_is_refused() {
    // The bridge never reports an impact for a spell that hit nothing: after the maximum
    // flight time the release is gone, and a stale native event cannot revive it.
    let mut w = world();
    let (id, _) = cast(&mut w, CASTER, spec(bolt()));
    assert_eq!(w.releases_in_flight(), 1);
    step_for(&mut w, PROJECTILE_MAX_FLIGHT_SECONDS + 1.0);
    assert_eq!(w.releases_in_flight(), 0);
    assert_eq!(w.native_impact(id, TARGET, 0).unwrap_err(), ImpactRefusal::Ledger(ImpactError::Unknown));
    assert_eq!(w.state(TARGET).unwrap().poise.accumulated(), 0.0, "a miss produced no consequence");
}
