//! The magic core: catalyst scaling, spell validation, the cast timeline, and exactly-once
//! impact correlation. Fixtures here are local to the tests; the first slice's catalysts and
//! spells are defined separately.

use er_combat_core::cast::*;
use er_combat_core::consequences::{DamageElement, ElementalProfile};
use er_combat_core::effects::{ModifiedStat, ModifierOperation, StackRule, StatusKind, STATUS_COUNT};
use er_combat_core::magic::*;

const DT: f32 = 1.0 / 60.0;

fn staff(quality: f32) -> Catalyst {
    Catalyst {
        id: 1,
        kind: CatalystKind::Staff,
        school: School::Destruction,
        quality,
        upgrade_level: 0,
        passive: None,
        cost_multiplier: 1.0,
    }
}

fn ring(passive: Option<Passive>) -> Catalyst {
    Catalyst {
        id: 2,
        kind: CatalystKind::Ring,
        school: School::Restoration,
        quality: 1.0,
        upgrade_level: 0,
        passive,
        cost_multiplier: 1.0,
    }
}

fn fire() -> ElementalProfile {
    ElementalProfile::from_channels(&[(DamageElement::Fire, 1.0)]).unwrap()
}

fn payload() -> SpellPayload {
    SpellPayload { elements: fire(), poise_pressure: 10.0, stance_pressure: 5.0, status_buildup: [0.0; STATUS_COUNT], modifiers: vec![] }
}

fn timing() -> CastTiming {
    CastTiming { release_at: 0.5, cancel_until: 0.2, recovery: 0.4, hyperarmour_from: 0.3, hyperarmour_to: 0.6 }
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
        payload: payload(),
    }
}

fn charged() -> SpellDefinition {
    SpellDefinition { id: 101, charge: Some(Charge { max_seconds: 1.0, power_at_full: 1.8 }), ..bolt() }
}

fn zone() -> SpellDefinition {
    SpellDefinition {
        id: 102,
        delivery: SpellDelivery::Area,
        area: Some(Area { radius: 4.0, duration: 3.0, tick_interval: 1.0 }),
        ..bolt()
    }
}

fn buff() -> SpellDefinition {
    let modifier = ModifierTemplate {
        on: ModifierTarget::Caster,
        group: 1,
        stat: ModifiedStat::DamageDealt,
        operation: ModifierOperation::Multiply,
        magnitude: 1.1,
        duration: 20.0,
        priority: 0,
        stack_rule: StackRule::Refresh,
    };
    SpellDefinition {
        id: 103,
        delivery: SpellDelivery::SelfCast,
        payload: SpellPayload { elements: ElementalProfile::default(), poise_pressure: 0.0, stance_pressure: 0.0, status_buildup: [0.0; STATUS_COUNT], modifiers: vec![modifier] },
        ..bolt()
    }
}

fn request<'a>(spell: &'a SpellDefinition, casting: &'a Catalyst, pool: f32) -> CastRequest<'a> {
    CastRequest { hand: Hand::Right, casting, offhand: None, spell, skill: 0.0, magicka_current: pool, magicka_max: 100.0, cost_scale: 1.0 }
}

fn input(pool: f32) -> CastInput {
    CastInput { magicka_current: pool, ..Default::default() }
}

/// Ticks until `done` returns true for an event, returning (ticks, all events).
fn run(caster: &mut Caster, mut make: impl FnMut(usize) -> CastInput, max: usize) -> (usize, Vec<CastEvent>) {
    let mut all = Vec::new();
    for tick in 0..max {
        let events = caster.tick(DT, make(tick));
        let finished = events.iter().any(|e| matches!(e, CastEvent::Finished | CastEvent::Cancelled | CastEvent::Interrupted | CastEvent::Fizzled { .. }));
        all.extend(events);
        if finished {
            return (tick + 1, all);
        }
    }
    (max, all)
}

fn released(events: &[CastEvent]) -> Vec<&SpellSnapshot> {
    events.iter().filter_map(|e| if let CastEvent::Released(s) = e { Some(s) } else { None }).collect()
}

// --- scaling: the gear axis ---------------------------------------------------------------

#[test]
fn skill_factor_is_a_soft_cap_that_never_goes_below_one() {
    assert_eq!(skill_factor(0.0), 1.0);
    assert!((skill_factor(100.0) - MAX_SKILL_FACTOR).abs() < 1e-6);
    let mut previous = skill_factor(0.0);
    let mut previous_gain = f32::MAX;
    for skill in 1..=100 {
        let value = skill_factor(skill as f32);
        assert!(value >= previous, "monotonic at {skill}");
        let gain = value - previous;
        assert!(gain <= previous_gain + 1e-6, "diminishing returns at {skill}");
        previous = value;
        previous_gain = gain;
    }
    for odd in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -50.0, 1e9] {
        let value = skill_factor(odd);
        assert!((1.0..=MAX_SKILL_FACTOR).contains(&value), "{odd} gave {value}");
    }
    assert_eq!(skill_factor(f32::NAN), 1.0, "unreadable skill is a level-one character");
}

#[test]
fn a_level_one_character_gets_the_catalysts_full_quality() {
    // No skill gate: skill 0 gives exactly quality, never less.
    for quality in [MIN_QUALITY, 0.8, 1.0, 1.2, MAX_QUALITY] {
        assert_eq!(staff(quality).scaling(0.0), quality);
    }
}

#[test]
fn scaling_grows_with_skill_and_upgrade_and_is_bounded() {
    let base = staff(1.0);
    assert!(base.scaling(100.0) > base.scaling(50.0) && base.scaling(50.0) > base.scaling(0.0));
    let mut previous = base.scaling(30.0);
    for level in 1..=MAX_UPGRADE_LEVEL {
        let now = Catalyst { upgrade_level: level, ..base }.scaling(30.0);
        assert!(now > previous, "upgrade {level}");
        previous = now;
    }
    let maxed = Catalyst { upgrade_level: MAX_UPGRADE_LEVEL, ..staff(MAX_QUALITY) };
    assert!(maxed.scaling(100.0) <= MAX_SCALING);
    assert_eq!(maxed.scaling(f32::NAN), maxed.scaling(0.0));
}

#[test]
fn catalyst_ordering_and_relative_gaps_survive_below_the_cap() {
    // Elden Ring supplies ordering and ratios; 413 against 345 is about 1.197.
    let (lusat, academy) = (staff(1.197), staff(1.0));
    for skill in [0.0, 25.0, 60.0] {
        let ratio = lusat.scaling(skill) / academy.scaling(skill);
        assert!((ratio - 1.197).abs() < 1e-4, "skill {skill}: {ratio}");
    }
}

#[test]
fn catalyst_validity_bounds() {
    assert!(staff(1.0).valid());
    for bad in [
        Catalyst { quality: f32::NAN, ..staff(1.0) },
        Catalyst { quality: MAX_QUALITY + 0.01, ..staff(1.0) },
        Catalyst { quality: MIN_QUALITY - 0.01, ..staff(1.0) },
        Catalyst { upgrade_level: MAX_UPGRADE_LEVEL + 1, ..staff(1.0) },
        Catalyst { cost_multiplier: 0.9, ..staff(1.0) },
        Catalyst { cost_multiplier: MAX_COST_MULTIPLIER + 0.1, ..staff(1.0) },
        Catalyst { passive: Some(Passive { family: SpellFamily(1), bonus: 0.0 }), ..staff(1.0) },
        Catalyst { passive: Some(Passive { family: SpellFamily(1), bonus: MAX_PASSIVE_BONUS + 0.01 }), ..staff(1.0) },
        Catalyst { passive: Some(Passive { family: SpellFamily(1), bonus: f32::NAN }), ..staff(1.0) },
    ] {
        assert!(!bad.valid(), "{bad:?}");
    }
}

#[test]
fn passives_boost_only_their_family_from_either_hand_and_do_not_stack() {
    let gravity = SpellFamily(7);
    let other = SpellFamily(9);
    let meteor = Catalyst { passive: Some(Passive { family: gravity, bonus: 0.3 }), ..staff(1.0) };
    let weaker = ring(Some(Passive { family: gravity, bonus: 0.1 }));
    let elsewhere = ring(Some(Passive { family: other, bonus: 0.4 }));
    assert_eq!(passive_bonus(gravity, &staff(1.0), None), 0.0);
    assert_eq!(passive_bonus(gravity, &meteor, None), 0.3, "the casting catalyst's own passive");
    assert_eq!(passive_bonus(gravity, &staff(1.0), Some(&meteor)), 0.3, "the off hand supplies a passive");
    assert_eq!(passive_bonus(gravity, &meteor, Some(&weaker)), 0.3, "the stronger of two, not the sum");
    assert_eq!(passive_bonus(gravity, &meteor, Some(&elsewhere)), 0.3, "another family's passive is not applied");
    assert_eq!(passive_bonus(SpellFamily(1), &meteor, Some(&elsewhere)), 0.0);
}

#[test]
fn power_is_a_ratio_of_scaling_and_the_multiplier_is_bounded() {
    let spell = bolt();
    let academy = staff(1.0);
    let uncharged = spell_damage_multiplier(&spell, &academy, None, 0.0, 1.0);
    assert!((uncharged - 0.8).abs() < 1e-6, "ratio 0.8 of scaling 1.0");
    let better = spell_damage_multiplier(&spell, &staff(1.2), None, 0.0, 1.0);
    assert!((better / uncharged - 1.2).abs() < 1e-5);
    let full = spell_damage_multiplier(&spell, &academy, None, 0.0, 1.8);
    assert!((full / uncharged - 1.8).abs() < 1e-5);
    // Nothing can exceed the bound, however the inputs conspire.
    let stacked = Catalyst { upgrade_level: MAX_UPGRADE_LEVEL, passive: Some(Passive { family: spell.family, bonus: MAX_PASSIVE_BONUS }), ..staff(MAX_QUALITY) };
    let huge = SpellDefinition { power_ratio: MAX_POWER_RATIO, ..spell.clone() };
    assert!(spell_damage_multiplier(&huge, &stacked, Some(&stacked), 100.0, MAX_CHARGE_POWER) <= MAX_SPELL_DAMAGE_MULTIPLIER);
    for odd in [f32::NAN, f32::INFINITY, -3.0] {
        let value = spell_damage_multiplier(&spell, &academy, None, odd, odd);
        assert!(value.is_finite() && (0.0..=MAX_SPELL_DAMAGE_MULTIPLIER).contains(&value), "{odd}: {value}");
    }
}

#[test]
fn magicka_is_frozen_in_combat_and_normal_out_of_it() {
    assert!(magicka_regen_allowed(false));
    assert!(!magicka_regen_allowed(true));
}

// --- spell validation -----------------------------------------------------------------------

#[test]
fn a_well_formed_spell_of_every_delivery_validates() {
    for spell in [bolt(), charged(), zone(), buff()] {
        assert_eq!(spell.validate(), Ok(()), "{}", spell.id);
    }
}

#[test]
fn every_way_to_be_malformed_is_named() {
    let cases: Vec<(SpellDefinition, SpellError)> = vec![
        (SpellDefinition { power_ratio: f32::NAN, ..bolt() }, SpellError::NotFinite),
        (SpellDefinition { power_ratio: MAX_POWER_RATIO + 0.1, ..bolt() }, SpellError::PowerRatioOutOfRange),
        (SpellDefinition { cost_fraction: 1.5, ..bolt() }, SpellError::CostOutOfRange),
        (SpellDefinition { castable_from: 0, ..bolt() }, SpellError::NoCatalystFamily),
        (SpellDefinition { castable_from: 0b100, ..bolt() }, SpellError::NoCatalystFamily),
        (SpellDefinition { timing: CastTiming { cancel_until: 0.9, ..timing() }, ..bolt() }, SpellError::BadTiming),
        (SpellDefinition { timing: CastTiming { release_at: 0.0, ..timing() }, ..bolt() }, SpellError::BadTiming),
        (SpellDefinition { timing: CastTiming { hyperarmour_to: 9.0, ..timing() }, ..bolt() }, SpellError::BadTiming),
        (SpellDefinition { charge: Some(Charge { max_seconds: 0.0, power_at_full: 1.5 }), ..bolt() }, SpellError::BadCharge),
        (SpellDefinition { charge: Some(Charge { max_seconds: 1.0, power_at_full: 0.5 }), ..bolt() }, SpellError::BadCharge),
        (SpellDefinition { delivery: SpellDelivery::Area, area: None, ..bolt() }, SpellError::AreaMismatch),
        (SpellDefinition { area: Some(Area { radius: 1.0, duration: 1.0, tick_interval: 1.0 }), ..bolt() }, SpellError::AreaMismatch),
        (SpellDefinition { area: Some(Area { radius: 1.0, duration: 1.0, tick_interval: 2.0 }), ..zone() }, SpellError::BadArea),
        (SpellDefinition { payload: SpellPayload { poise_pressure: -1.0, ..payload() }, ..bolt() }, SpellError::BadPayload),
        (SpellDefinition { payload: SpellPayload { elements: ElementalProfile::default(), ..payload() }, ..bolt() }, SpellError::EmptyPayload),
        (SpellDefinition { payload: SpellPayload { modifiers: vec![buff().payload.modifiers[0]; MAX_SPELL_MODIFIERS + 1], ..payload() }, ..bolt() }, SpellError::BadPayload),
        (SpellDefinition { delivery: SpellDelivery::SelfCast, ..bolt() }, SpellError::SelfCastCannotHitTargets),
        (SpellDefinition { payload: SpellPayload { elements: ElementalProfile::default(), ..payload() }, delivery: SpellDelivery::SelfCast, ..bolt() }, SpellError::EmptyPayload),
    ];
    for (spell, expected) in cases {
        assert_eq!(spell.validate(), Err(expected), "{spell:?}");
    }
    // A targeted spell that only buffs its caster does nothing to anyone.
    let mut only_self = buff();
    only_self.delivery = SpellDelivery::Instant;
    assert_eq!(only_self.validate(), Err(SpellError::TargetedNeedsTargetEffect));
    // A self-cast with only a target effect has nothing for the caster.
    let mut wrong_side = buff();
    wrong_side.payload.modifiers[0].on = ModifierTarget::Target;
    assert_eq!(wrong_side.validate(), Err(SpellError::SelfCastCannotHitTargets));
    let mut empty_status = bolt();
    empty_status.payload.elements = ElementalProfile::default();
    empty_status.payload.status_buildup[StatusKind::Frostbite.index()] = 30.0;
    assert_eq!(empty_status.validate(), Ok(()), "a pure status spell is valid");
}

#[test]
fn the_hit_event_matches_what_melee_and_arrows_produce() {
    let mut caster = Caster::new(7);
    let spell = bolt();
    let catalyst = staff(1.0);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    let (_, events) = run(&mut caster, |_| input(100.0), 120);
    let snapshot = released(&events)[0].clone();
    let hit = snapshot.hit_event(42, 3);
    assert_eq!((hit.source, hit.target, hit.action_id, hit.hit_id), (7, 42, 100, 3));
    assert!((hit.damage_multiplier - 0.8).abs() < 1e-6);
    assert_eq!(hit.armament_multiplier, 1.0, "magic carries no imported-armament bonus");
    assert_eq!(hit.elements, fire());
    assert!(hit.target_modifiers.is_empty());
    // A non-damaging spell asks Skyrim for no damage at all.
    let mut status = bolt();
    status.payload.elements = ElementalProfile::default();
    status.payload.status_buildup[StatusKind::Frostbite.index()] = 30.0;
    let mut caster = Caster::new(8);
    caster.begin(request(&status, &catalyst, 100.0)).unwrap();
    let (_, events) = run(&mut caster, |_| input(100.0), 120);
    let hit = released(&events)[0].hit_event(1, 1);
    assert_eq!(hit.damage_multiplier, 0.0);
    assert_eq!(hit.status_buildup[StatusKind::Frostbite.index()], 30.0);
}

// --- the cast timeline -----------------------------------------------------------------------

#[test]
fn begin_refuses_for_each_named_reason() {
    let (spell, catalyst) = (bolt(), staff(1.0));
    let mut caster = Caster::new(1);
    assert!(caster.begin(request(&spell, &catalyst, 100.0)).is_ok());
    assert_eq!(caster.begin(request(&spell, &catalyst, 100.0)), Err(CastRefusal::Busy));

    let mut fresh = Caster::new(2);
    assert_eq!(fresh.begin(request(&spell, &Catalyst { quality: f32::NAN, ..catalyst }, 100.0)), Err(CastRefusal::InvalidCatalyst));
    assert_eq!(fresh.begin(request(&SpellDefinition { power_ratio: 9.0, ..spell.clone() }, &catalyst, 100.0)), Err(CastRefusal::InvalidSpell(SpellError::PowerRatioOutOfRange)));
    let staff_only = SpellDefinition { castable_from: FROM_STAFF, ..spell.clone() };
    assert_eq!(fresh.begin(request(&staff_only, &ring(None), 100.0)), Err(CastRefusal::WrongCatalystFamily));
    assert_eq!(fresh.begin(CastRequest { magicka_max: f32::NAN, ..request(&spell, &catalyst, 100.0) }), Err(CastRefusal::InvalidPool));
    assert_eq!(fresh.begin(CastRequest { magicka_max: -1.0, ..request(&spell, &catalyst, 100.0) }), Err(CastRefusal::InvalidPool));
    assert_eq!(fresh.begin(request(&spell, &catalyst, f32::NAN)), Err(CastRefusal::InvalidPool));
    match fresh.begin(request(&spell, &catalyst, 5.0)) {
        Err(CastRefusal::NotEnoughMagicka { needed, available }) => assert_eq!((needed, available), (10.0, 5.0)),
        other => panic!("{other:?}"),
    }
    assert!(fresh.is_idle(), "refusals leave the caster idle");
    assert!(fresh.begin(request(&spell, &catalyst, 10.0)).is_ok(), "exactly the cost is enough");
}

#[test]
fn a_level_one_character_can_cast_with_any_matching_catalyst() {
    // No skill, level or perk gate: only the catalyst family decides.
    for catalyst in [staff(MIN_QUALITY), ring(None)] {
        let mut caster = Caster::new(1);
        assert!(caster.begin(CastRequest { skill: 0.0, ..request(&bolt(), &catalyst, 100.0) }).is_ok());
    }
}

#[test]
fn the_cast_releases_on_its_frame_charges_once_and_recovers() {
    let (spell, catalyst) = (bolt(), staff(1.0));
    let mut caster = Caster::new(5);
    let started = caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    assert_eq!(started, CastEvent::Started { hand: Hand::Right, spell_id: 100 });
    let mut tick_released = None;
    let mut tick_finished = None;
    let mut all = Vec::new();
    for tick in 0..120 {
        let events = caster.tick(DT, input(100.0));
        for event in &events {
            match event {
                CastEvent::Released(_) => tick_released = Some(tick),
                CastEvent::Finished => tick_finished = Some(tick),
                _ => {}
            }
        }
        all.extend(events);
    }
    // release_at 0.5 s = 30 ticks; recovery 0.4 s = 24 more.
    assert_eq!(tick_released, Some(29), "the release frame");
    assert_eq!(tick_finished, Some(53), "recovery ends");
    assert_eq!(released(&all).len(), 1);
    assert!(caster.is_idle());
    assert_eq!(caster.take_spent(), 10.0, "100 magicka x 0.1 fraction x 1.0 cost multiplier");
    assert_eq!(caster.take_spent(), 0.0, "reported once");
}

#[test]
fn the_catalysts_cost_trade_and_cost_modifiers_scale_the_spend() {
    let spell = bolt();
    let lusat = Catalyst { cost_multiplier: 1.5, ..staff(1.2) };
    let mut caster = Caster::new(1);
    caster.begin(CastRequest { cost_scale: 0.8, ..request(&spell, &lusat, 100.0) }).unwrap();
    run(&mut caster, |_| input(100.0), 120);
    assert!((caster.take_spent() - 12.0).abs() < 1e-4, "0.1 x 1.5 x 0.8 x 100");
}

#[test]
fn the_hand_stays_dedicated_from_startup_through_recovery() {
    let (spell, catalyst) = (bolt(), ring(None));
    let mut caster = Caster::new(1);
    assert_eq!(caster.dedicated_hand(), None);
    caster.begin(CastRequest { hand: Hand::Left, ..request(&spell, &catalyst, 100.0) }).unwrap();
    for tick in 0..53 {
        assert_eq!(caster.dedicated_hand(), Some(Hand::Left), "tick {tick}");
        caster.tick(DT, input(100.0));
    }
    caster.tick(DT, input(100.0));
    assert_eq!(caster.dedicated_hand(), None, "control returns when recovery ends");
}

#[test]
fn a_dodge_cancels_only_inside_the_cancel_window() {
    let (spell, catalyst) = (bolt(), staff(1.0));
    // Inside: cancel_until 0.2 s.
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    assert!(caster.dodge_allowed());
    let events = caster.tick(DT, CastInput { dodge: true, ..input(100.0) });
    assert_eq!(events, vec![CastEvent::Cancelled]);
    assert!(caster.is_idle());
    assert_eq!(caster.take_spent(), 0.0, "a cancelled cast costs nothing");

    // Past it: committed.
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    for _ in 0..13 {
        caster.tick(DT, input(100.0));
    }
    assert!(!caster.dodge_allowed(), "past 0.2 s a roll is refused");
    let events = caster.tick(DT, CastInput { dodge: true, ..input(100.0) });
    assert!(!events.contains(&CastEvent::Cancelled));
    let (_, rest) = run(&mut caster, |_| CastInput { dodge: true, ..input(100.0) }, 120);
    assert_eq!(released(&rest).len(), 1, "the dodge presses did not cancel the cast");
    assert!(!caster.dodge_allowed() || caster.is_idle());
}

#[test]
fn poise_breaks_interrupt_outside_hyperarmour_and_are_ignored_inside_it() {
    let (spell, catalyst) = (bolt(), staff(1.0));
    // Before hyperarmour (0.3-0.6 s): interrupted, free.
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    let events = caster.tick(DT, CastInput { poise_broken: true, ..input(100.0) });
    assert_eq!(events, vec![CastEvent::Interrupted]);
    assert_eq!(caster.take_spent(), 0.0);

    // Inside hyperarmour: carried through.
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    for _ in 0..19 {
        caster.tick(DT, input(100.0));
    }
    let (_, events) = run(&mut caster, |tick| CastInput { poise_broken: tick < 5, ..input(100.0) }, 120);
    assert!(!events.contains(&CastEvent::Interrupted));
    assert_eq!(released(&events).len(), 1);

    // After the window, in recovery: ends recovery without a second release.
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    for _ in 0..40 {
        caster.tick(DT, input(100.0));
    }
    let events = caster.tick(DT, CastInput { poise_broken: true, ..input(100.0) });
    assert_eq!(events, vec![CastEvent::Interrupted]);
    assert_eq!(caster.take_spent(), 10.0, "the release already happened and was paid for");
}

#[test]
fn a_charged_spell_scales_with_the_hold_and_full_charge_releases_itself() {
    let (spell, catalyst) = (charged(), staff(1.0));
    let power = |hold_ticks: usize| {
        let mut caster = Caster::new(1);
        caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
        let (_, events) = run(&mut caster, |tick| CastInput { held: tick < 30 + hold_ticks, ..input(100.0) }, 300);
        let snapshot = released(&events)[0].clone();
        (snapshot.damage_multiplier, snapshot.charge_fraction)
    };
    let (tap, tap_fraction) = power(0);
    // The release frame falls partway through a tick, so a tap carries that sliver of charge.
    assert!((tap - 0.8).abs() < 1e-3 && tap_fraction < 0.02, "tapped: uncharged power, got {tap} / {tap_fraction}");
    let (half, half_fraction) = power(30);
    assert!((half_fraction - 0.5).abs() < 0.02, "{half_fraction}");
    assert!(half > tap);
    let (full, full_fraction) = power(60);
    let (way_past, past_fraction) = power(500);
    assert!((full_fraction - 1.0).abs() < 1e-3 && (past_fraction - 1.0).abs() < 1e-6);
    assert!((way_past - 0.8 * 1.8).abs() < 1e-5, "full charge is the cap");
    assert!(way_past >= full - 1e-4, "holding longer adds nothing");
    assert!(tap < half && half < way_past, "power rises with the hold");
}

#[test]
fn the_pool_is_checked_again_at_the_release_frame() {
    let (spell, catalyst) = (bolt(), staff(1.0));
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    // Skyrim drains the pool mid-cast (a potion of ... something unfortunate).
    let (_, events) = run(&mut caster, |tick| input(if tick < 10 { 100.0 } else { 3.0 }), 120);
    assert_eq!(events.last(), Some(&CastEvent::Fizzled { needed: 10.0, available: 3.0 }));
    assert!(released(&events).is_empty());
    assert_eq!(caster.take_spent(), 0.0);
    assert!(caster.is_idle());
    // A non-finite pool reading is an empty pool, not a free cast.
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    let (_, events) = run(&mut caster, |_| input(f32::NAN), 120);
    assert!(matches!(events.last(), Some(CastEvent::Fizzled { .. })));
}

#[test]
#[allow(unused_assignments)]
fn equipment_changes_cannot_alter_a_cast_in_progress() {
    let spell = bolt();
    let mut catalyst = staff(1.0);
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    catalyst.quality = MAX_QUALITY; // the player swaps staves mid-cast
    let (_, events) = run(&mut caster, |_| input(100.0), 120);
    let snapshot = released(&events)[0];
    assert_eq!(snapshot.catalyst_id, 1);
    assert!((snapshot.damage_multiplier - 0.8).abs() < 1e-6, "the original catalyst's scaling");
}

#[test]
fn casting_is_deterministic_bit_for_bit() {
    let trace = || {
        let mut caster = Caster::new(9);
        let spell = charged();
        let catalyst = staff(1.1);
        caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
        let mut log = Vec::new();
        for tick in 0..200 {
            let events = caster.tick(DT, CastInput { held: tick < 55, poise_broken: tick == 12, magicka_current: 100.0, ..Default::default() });
            for event in events {
                log.push((tick, format!("{event:?}")));
            }
        }
        (log, caster.take_spent().to_bits())
    };
    assert_eq!(trace(), trace());
}

#[test]
fn a_long_step_transitions_in_order() {
    let (spell, catalyst) = (bolt(), staff(1.0));
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    let events = caster.tick(5.0, input(100.0));
    assert!(matches!(events.as_slice(), [CastEvent::Released(_), CastEvent::Finished]), "{events:?}");
    assert_eq!(caster.take_spent(), 10.0);
    // Nonsense steps change nothing.
    let mut caster = Caster::new(1);
    caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
    assert!(caster.tick(f32::NAN, input(100.0)).is_empty());
    assert!(caster.tick(-1.0, input(100.0)).is_empty());
    assert!(!caster.is_idle());
}

#[test]
fn release_ids_are_unique_across_casts_and_actors() {
    let (spell, catalyst) = (bolt(), staff(1.0));
    let mut ids = Vec::new();
    for actor in [1u32, 2] {
        let mut caster = Caster::new(actor);
        for _ in 0..3 {
            caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
            let (_, events) = run(&mut caster, |_| input(100.0), 120);
            ids.push(released(&events)[0].release_id);
        }
    }
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 6, "{ids:?}");
}

// --- exactly-once impact correlation --------------------------------------------------------

fn snapshot_of(spell: &SpellDefinition, actor: u32) -> SpellSnapshot {
    let mut caster = Caster::new(actor);
    let catalyst = staff(1.0);
    caster.begin(request(spell, &catalyst, 100.0)).unwrap();
    let (_, events) = run(&mut caster, |_| input(100.0), 120);
    released(&events)[0].clone()
}

#[test]
fn a_projectile_resolves_once_per_target() {
    let mut ledger = ImpactLedger::default();
    let snapshot = snapshot_of(&bolt(), 1);
    let id = snapshot.release_id;
    assert!(ledger.register(snapshot));
    assert!(ledger.resolve(id, 10, 0).is_ok());
    assert_eq!(ledger.resolve(id, 10, 0).unwrap_err(), ImpactError::Duplicate);
    assert!(ledger.resolve(id, 11, 0).is_ok(), "a different target is a different resolution");
    assert_eq!(ledger.resolve(id, 12, 1).unwrap_err(), ImpactError::TickOutOfRange, "a projectile has no later ticks");
    assert_eq!(ledger.resolve(id + 1, 10, 0).unwrap_err(), ImpactError::Unknown);
    assert_eq!(ledger.resolve(0, 10, 0).unwrap_err(), ImpactError::Unknown);
}

#[test]
fn an_area_resolves_each_target_once_per_tick_within_its_life() {
    let mut ledger = ImpactLedger::default();
    let snapshot = snapshot_of(&zone(), 1);
    let id = snapshot.release_id;
    assert!(ledger.register(snapshot));
    for tick in 0..3 {
        let hit = ledger.resolve(id, 10, tick).unwrap();
        assert_eq!(hit.hit_id, tick + 1, "each tick is its own resolution");
        assert_eq!(ledger.resolve(id, 10, tick).unwrap_err(), ImpactError::Duplicate);
    }
    assert_eq!(ledger.resolve(id, 10, 3).unwrap_err(), ImpactError::TickOutOfRange, "3 s at 1 s ticks is 3 ticks");
    assert!(ledger.resolve(id, 99, 2).is_ok(), "another target, same tick");
}

#[test]
fn a_miss_expires_without_consequence_and_a_late_impact_is_stale() {
    let mut ledger = ImpactLedger::default();
    let snapshot = snapshot_of(&bolt(), 1);
    let id = snapshot.release_id;
    ledger.register(snapshot);
    ledger.tick(PROJECTILE_MAX_FLIGHT_SECONDS - 0.1);
    assert_eq!(ledger.len(), 1);
    ledger.tick(0.2);
    assert!(ledger.is_empty(), "never resolved, so no consequence was ever produced");
    assert_eq!(ledger.resolve(id, 10, 0).unwrap_err(), ImpactError::Unknown, "stale");
}

#[test]
fn a_resolved_release_still_expires_and_instants_expire_quickly() {
    let mut ledger = ImpactLedger::default();
    let instant = SpellDefinition { delivery: SpellDelivery::Instant, ..bolt() };
    let snapshot = snapshot_of(&instant, 1);
    let id = snapshot.release_id;
    ledger.register(snapshot);
    assert!(ledger.resolve(id, 10, 0).is_ok());
    ledger.tick(INSTANT_RESOLVE_SECONDS + 0.01);
    assert_eq!(ledger.resolve(id, 11, 0).unwrap_err(), ImpactError::Unknown);
}

#[test]
fn self_casts_duplicates_and_floods_are_not_tracked() {
    let mut ledger = ImpactLedger::default();
    let buff_snapshot = snapshot_of(&buff(), 1);
    assert!(!ledger.register(buff_snapshot.clone()), "a self-cast has no impact to correlate");
    assert_eq!(buff_snapshot.caster_modifiers().len(), 1);
    assert_eq!(buff_snapshot.hit_event(5, 1).target_modifiers.len(), 0);

    let snapshot = snapshot_of(&bolt(), 1);
    assert!(ledger.register(snapshot.clone()));
    assert!(!ledger.register(snapshot), "the same release id twice");
    let mut caster = Caster::new(2);
    let (spell, catalyst) = (bolt(), staff(1.0));
    let mut accepted = 1;
    for _ in 0..(MAX_TRACKED_RELEASES + 10) {
        caster.begin(request(&spell, &catalyst, 100.0)).unwrap();
        let (_, events) = run(&mut caster, |_| input(100.0), 120);
        if ledger.register(released(&events)[0].clone()) {
            accepted += 1;
        }
    }
    assert_eq!(accepted, MAX_TRACKED_RELEASES, "the table is bounded");
    assert_eq!(ledger.len(), MAX_TRACKED_RELEASES);
}

#[test]
fn the_snapshot_in_the_ledger_ignores_later_gear_changes() {
    let mut ledger = ImpactLedger::default();
    let snapshot = snapshot_of(&bolt(), 1);
    let id = snapshot.release_id;
    ledger.register(snapshot);
    let hit = ledger.resolve(id, 10, 0).unwrap();
    assert!((hit.damage_multiplier - 0.8).abs() < 1e-6);
}

#[test]
fn charge_power_is_linear_and_capped_on_its_own() {
    let charge = Charge { max_seconds: 2.0, power_at_full: 1.6 };
    assert_eq!(charge.power(0.0), 1.0);
    assert!((charge.power(1.0) - 1.3).abs() < 1e-6, "halfway is halfway");
    assert!((charge.power(2.0) - 1.6).abs() < 1e-6);
    assert_eq!(charge.power(20.0), 1.6, "holding past full adds nothing");
    assert_eq!(charge.fraction(20.0), 1.0);
    assert_eq!(charge.power(-3.0), 1.0, "negative hold is no hold");
    assert_eq!(charge.power(f32::NAN), 1.0);
    // Unusable input never earns power: an infinite hold reads as no charge, like NaN.
    assert_eq!(charge.fraction(f32::INFINITY), 0.0);
}
