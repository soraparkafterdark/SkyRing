//! A live magic session and its headless replay are the same bytes.

use std::collections::{BTreeMap, BTreeSet};

use er_combat_core::cast::CastInput;
use er_combat_core::consequences::{DamageElement, ElementalProfile, TargetRules};
use er_combat_core::effects::{ModifiedStat, ModifierOperation, StackRule, StatusDefinition, StatusKind, STATUS_COUNT};
use er_combat_core::magic::*;
use er_combat_core::magic_trace::*;
use er_combat_core::magic_world::*;
use er_combat_core::posture::{PoiseDefinition, StanceDefinition};

const DT: f32 = 1.0 / 60.0;

fn rules(poise: f32) -> TargetRules {
    TargetRules {
        poise: PoiseDefinition { threshold: poise, recovery_delay_seconds: 2.0, recovery_per_second: 20.0 },
        stance: StanceDefinition { threshold: 100.0, decay_delay_seconds: 2.0, decay_per_second: 10.0, break_lockout_seconds: 3.0 },
        statuses: [StatusDefinition { threshold: 100.0, decay_delay_seconds: 2.0, decay_per_second: 10.0, proc_lockout_seconds: 5.0 }; STATUS_COUNT],
        status_received: [1.0; STATUS_COUNT],
        stance_received: 1.0,
    }
}

fn staff() -> Catalyst {
    Catalyst { id: 1, kind: CatalystKind::Staff, school: School::Destruction, quality: 1.1, upgrade_level: 2, passive: None, cost_multiplier: 1.0 }
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
        castable_from: FROM_STAFF,
        power_ratio: 0.8,
        cost_fraction: 0.1,
        timing: CastTiming { release_at: 0.4, cancel_until: 0.1, recovery: 0.3, hyperarmour_from: 0.2, hyperarmour_to: 0.5 },
        charge: None,
        area: None,
        payload: SpellPayload {
            elements: ElementalProfile::from_channels(&[(DamageElement::Fire, 1.0)]).unwrap(),
            poise_pressure: 40.0,
            stance_pressure: 30.0,
            ..none()
        },
    }
}

fn frost() -> SpellDefinition {
    let mut buildup = [0.0; STATUS_COUNT];
    buildup[StatusKind::Frostbite.index()] = 60.0;
    SpellDefinition { id: 102, delivery: SpellDelivery::Instant, power_ratio: 0.0, payload: SpellPayload { status_buildup: buildup, ..none() }, ..bolt() }
}

fn buff() -> SpellDefinition {
    let m = ModifierTemplate {
        on: ModifierTarget::Caster,
        group: 1,
        stat: ModifiedStat::DamageDealt,
        operation: ModifierOperation::Multiply,
        magnitude: 1.25,
        duration: 10.0,
        priority: 0,
        stack_rule: StackRule::Refresh,
    };
    SpellDefinition { id: 103, delivery: SpellDelivery::SelfCast, power_ratio: 0.0, payload: SpellPayload { modifiers: vec![m], ..none() }, ..bolt() }
}

fn zone() -> SpellDefinition {
    SpellDefinition { id: 104, delivery: SpellDelivery::Area, area: Some(Area { radius: 4.0, duration: 2.0, tick_interval: 1.0 }), ..bolt() }
}

fn spec(spell: SpellDefinition, pool: f32) -> CastSpec {
    CastSpec { hand: Hand::Right, casting: staff(), offhand: None, spell, skill: 35.0, magicka_current: pool, magicka_max: 100.0 }
}

fn idle() -> BTreeMap<u32, CastInput> {
    BTreeMap::new()
}

/// A fight that touches every event kind: spawns, casts, refusals, fizzles, impacts, refusals,
/// a buff, an area, an interrupt, deaths, unloads and a respawn.
fn fight(r: &mut MagicRecorder) {
    for (actor, poise) in [(1, 100.0), (2, 100.0), (3, 10.0)] {
        r.spawn(actor, rules(poise)).unwrap();
    }
    assert!(r.spawn(2, rules(100.0)).is_err(), "an existing actor cannot be spawned twice");
    // Refused for lack of magicka, then a cast, a buff, a frost, a debuffed bolt.
    r.begin_cast(1, &spec(bolt(), 2.0));
    let mut releases = Vec::new();
    for (index, (spell, pool)) in [(buff(), 100.0), (bolt(), 100.0), (frost(), 100.0), (zone(), 100.0)].into_iter().enumerate() {
        r.begin_cast(1, &spec(spell, pool));
        for _ in 0..90 {
            r.step(DT, &idle());
        }
        let id = r.world().events().iter().rev().find_map(|e| match e {
            MagicEvent::SpellReleased { release_id, .. } => Some(*release_id),
            _ => None,
        });
        releases.push(id.expect("released"));
        let _ = index;
    }
    // Impacts: good, duplicate, unknown release, area ticks, an unknown target.
    r.native_impact(releases[1], 2, 0).ok();
    r.native_impact(releases[1], 2, 0).ok();
    r.native_impact(releases[2], 2, 0).ok();
    r.native_impact(releases[2], 2, 0).ok();
    r.native_impact(999, 2, 0).ok();
    for tick in 0..3 {
        r.native_impact(releases[3], 2, tick).ok();
        r.native_impact(releases[3], 3, tick).ok();
    }
    r.native_impact(releases[1], 77, 0).ok();
    // The weak target is mid-cast when struck hard, and gets interrupted.
    let mut slow = bolt();
    slow.timing = CastTiming { release_at: 2.0, cancel_until: 0.1, recovery: 0.3, hyperarmour_from: 1.5, hyperarmour_to: 1.9 };
    slow.id = 110;
    r.begin_cast(3, &spec(slow, 100.0));
    r.step(DT, &idle());
    r.begin_cast(1, &spec(bolt(), 100.0));
    for _ in 0..40 {
        r.step(DT, &idle());
    }
    let strike = r.world().events().iter().rev().find_map(|e| match e {
        MagicEvent::SpellReleased { release_id, actor: 1, .. } => Some(*release_id),
        _ => None,
    });
    r.native_impact(strike.unwrap(), 3, 0).ok();
    for _ in 0..5 {
        r.step(DT, &idle());
    }
    // A fizzle: the pool drops to nothing mid-cast.
    r.begin_cast(2, &spec(bolt(), 100.0));
    let mut drained = BTreeMap::new();
    drained.insert(2, CastInput { magicka_current: 0.0, ..Default::default() });
    for _ in 0..40 {
        r.step(DT, &drained);
    }
    r.kill(3).unwrap();
    r.respawn(3).unwrap();
    r.unload(2).unwrap();
    r.native_impact(releases[1], 2, 0).ok();
    for _ in 0..30 {
        r.step(DT, &idle());
    }
    assert!(r.kill(99).is_err());
}

fn run() -> (MagicScript, Vec<MagicEvent>) {
    let mut recorder = MagicRecorder::new();
    fight(&mut recorder);
    recorder.finish()
}

fn kind(event: &MagicEvent) -> &'static str {
    match event {
        MagicEvent::ActorSpawned { .. } => "ActorSpawned",
        MagicEvent::ActorDied { .. } => "ActorDied",
        MagicEvent::ActorRespawned { .. } => "ActorRespawned",
        MagicEvent::ActorUnloaded { .. } => "ActorUnloaded",
        MagicEvent::CastStarted { .. } => "CastStarted",
        MagicEvent::CastRefused { .. } => "CastRefused",
        MagicEvent::SpellReleased { .. } => "SpellReleased",
        MagicEvent::MagickaSpent { .. } => "MagickaSpent",
        MagicEvent::ReleaseUntracked { .. } => "ReleaseUntracked",
        MagicEvent::SelfEffectsApplied { .. } => "SelfEffectsApplied",
        MagicEvent::CastEnded { .. } => "CastEnded",
        MagicEvent::ImpactResolved { .. } => "ImpactResolved",
        MagicEvent::ImpactRefused { .. } => "ImpactRefused",
    }
}

/// More simultaneous releases than the ledger can track: the overflow is reported, not silent.
fn flood() -> (MagicScript, Vec<MagicEvent>) {
    let mut recorder = MagicRecorder::new();
    for actor in 1..=70u32 {
        recorder.spawn(actor, rules(100.0)).unwrap();
    }
    for actor in 1..=70u32 {
        recorder.begin_cast(actor, &spec(bolt(), 100.0));
    }
    for _ in 0..40 {
        recorder.step(DT, &idle());
    }
    recorder.finish()
}

#[test]
fn the_scripts_exercise_every_kind_of_event() {
    let (_, events) = run();
    let (_, flooded) = flood();
    let seen: BTreeSet<&str> = events.iter().chain(flooded.iter()).map(kind).collect();
    assert_eq!(seen.len(), 13, "missing kinds: {seen:?}");
    assert!(flooded.iter().any(|e| matches!(e, MagicEvent::ReleaseUntracked { reason: er_combat_core::cast::Registration::Full, .. })));
    assert_eq!(flooded.iter().filter(|e| matches!(e, MagicEvent::ReleaseUntracked { .. })).count(), 6, "70 releases, 64 tracked");
}

#[test]
fn the_overflow_replays_identically() {
    let (script, live) = flood();
    assert_eq!(script.replay(), live);
    assert_eq!(canonical_event_bytes(&script.replay()), canonical_event_bytes(&live));
}

#[test]
fn a_live_run_and_its_replay_are_the_same_log_and_the_same_bytes() {
    let (script, live) = run();
    assert!(script.len() > 300, "{}", script.len());
    let replayed = script.replay();
    assert_eq!(live, replayed);
    assert_eq!(canonical_event_bytes(&live), canonical_event_bytes(&replayed));
    // Replaying twice changes nothing.
    assert_eq!(canonical_event_bytes(&script.replay()), canonical_event_bytes(&replayed));
}

#[test]
fn recording_does_not_change_what_a_world_does() {
    // The same calls made on a bare world give the same log as the recorder's.
    let (script, recorded) = run();
    let mut bare = MagicWorld::new();
    for step in script.steps() {
        match step {
            ScriptStep::Spawn { actor, rules } => drop(bare.spawn(*actor, *rules)),
            ScriptStep::Unload { actor } => drop(bare.unload(*actor)),
            ScriptStep::Kill { actor } => drop(bare.kill(*actor)),
            ScriptStep::Respawn { actor } => drop(bare.respawn(*actor)),
            ScriptStep::BeginCast { actor, spec } => drop(bare.begin_cast(*actor, spec)),
            ScriptStep::Step { dt, inputs } => bare.step(*dt, inputs),
            ScriptStep::Impact { release_id, target, tick } => drop(bare.native_impact(*release_id, *target, *tick)),
        }
    }
    assert_eq!(bare.events(), recorded.as_slice());
}

#[test]
fn any_difference_in_the_session_changes_the_bytes() {
    let (script, live) = run();
    let baseline = canonical_event_bytes(&live);
    // A different step size changes the world, and so the log.
    let mut altered = script.clone();
    let first_step = altered.steps().iter().position(|s| matches!(s, ScriptStep::Step { .. })).unwrap();
    let mut steps = altered.steps().to_vec();
    if let ScriptStep::Step { dt, .. } = &mut steps[first_step] {
        *dt = DT * 12.0; // the release frame moves several ticks earlier
    }
    let mut again = MagicRecorder::new();
    for step in &steps {
        match step {
            ScriptStep::Spawn { actor, rules } => drop(again.spawn(*actor, *rules)),
            ScriptStep::Unload { actor } => drop(again.unload(*actor)),
            ScriptStep::Kill { actor } => drop(again.kill(*actor)),
            ScriptStep::Respawn { actor } => drop(again.respawn(*actor)),
            ScriptStep::BeginCast { actor, spec } => drop(again.begin_cast(*actor, spec)),
            ScriptStep::Step { dt, inputs } => again.step(*dt, inputs),
            ScriptStep::Impact { release_id, target, tick } => drop(again.native_impact(*release_id, *target, *tick)),
        }
    }
    let (_, changed) = again.finish();
    assert_ne!(canonical_event_bytes(&changed), baseline);
    // And a single event's single field does too.
    let mut tampered = live.clone();
    for event in &mut tampered {
        if let MagicEvent::SpellReleased { damage_multiplier, .. } = event {
            *damage_multiplier += f32::EPSILON * 8.0;
            break;
        }
    }
    assert_ne!(canonical_event_bytes(&tampered), baseline);
    altered = MagicScript::default();
    assert!(altered.is_empty());
}

#[test]
fn the_encoding_has_a_header_and_distinguishes_refusal_reasons() {
    let bytes = canonical_event_bytes(&[]);
    assert_eq!(&bytes[..8], b"SRMAGEV\0");
    assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
    assert_eq!(&bytes[10..18], &0u64.to_le_bytes());
    assert_eq!(bytes.len(), 18);
    let a = MagicEvent::ImpactRefused { tick: 1, release_id: 2, target: 3, reason: ImpactRefusal::TargetDead };
    let b = MagicEvent::ImpactRefused { tick: 1, release_id: 2, target: 3, reason: ImpactRefusal::UnknownTarget };
    assert_ne!(canonical_event_bytes(&[a.clone()]), canonical_event_bytes(&[b]));
    assert_eq!(canonical_event_bytes(&[a.clone()]), canonical_event_bytes(&[a]));
}

#[test]
fn refused_calls_are_part_of_the_replay() {
    // A refusal in the live run must be a refusal in the replay, with the same reason.
    let mut recorder = MagicRecorder::new();
    recorder.spawn(1, rules(100.0)).unwrap();
    assert!(matches!(recorder.begin_cast(1, &spec(bolt(), 1.0)), CastStart::Refused(_)));
    assert_eq!(recorder.begin_cast(9, &spec(bolt(), 100.0)), CastStart::Failed(WorldError::UnknownActor));
    let (script, live) = recorder.finish();
    assert_eq!(script.replay(), live);
    assert!(live.iter().any(|e| matches!(e, MagicEvent::CastRefused { .. })));
}

/// Every way to change exactly one field of an event. The exhaustive match means a new
/// variant or field must be added here, or this file stops compiling.
fn perturbations(event: &MagicEvent) -> Vec<MagicEvent> {
    use MagicEvent::*;
    match event.clone() {
        ActorSpawned { tick, actor } => vec![ActorSpawned { tick: tick + 1, actor }, ActorSpawned { tick, actor: actor + 1 }],
        ActorDied { tick, actor } => vec![ActorDied { tick: tick + 1, actor }, ActorDied { tick, actor: actor + 1 }],
        ActorRespawned { tick, actor } => vec![ActorRespawned { tick: tick + 1, actor }, ActorRespawned { tick, actor: actor + 1 }],
        ActorUnloaded { tick, actor } => vec![ActorUnloaded { tick: tick + 1, actor }, ActorUnloaded { tick, actor: actor + 1 }],
        CastStarted { tick, actor, spell } => vec![
            CastStarted { tick: tick + 1, actor, spell },
            CastStarted { tick, actor: actor + 1, spell },
            CastStarted { tick, actor, spell: spell + 1 },
        ],
        CastRefused { tick, actor, spell, reason } => {
            let mut variants = vec![];
            if let er_combat_core::cast::CastRefusal::NotEnoughMagicka { needed, available } = reason {
                variants.push(CastRefused { tick, actor, spell, reason: er_combat_core::cast::CastRefusal::NotEnoughMagicka { needed: needed + 0.5, available } });
                variants.push(CastRefused { tick, actor, spell, reason: er_combat_core::cast::CastRefusal::NotEnoughMagicka { needed, available: available + 0.5 } });
            }
            variants.extend(vec![
            CastRefused { tick: tick + 1, actor, spell, reason },
            CastRefused { tick, actor: actor + 1, spell, reason },
            CastRefused { tick, actor, spell: spell + 1, reason },
            CastRefused { tick, actor, spell, reason: er_combat_core::cast::CastRefusal::Busy },
            CastRefused { tick, actor, spell, reason: er_combat_core::cast::CastRefusal::NotEnoughMagicka { needed: 1.0, available: 0.5 } },
            ]);
            variants
        }
        SpellReleased { tick, actor, release_id, spell, damage_multiplier, charge_fraction } => vec![
            SpellReleased { tick: tick + 1, actor, release_id, spell, damage_multiplier, charge_fraction },
            SpellReleased { tick, actor: actor + 1, release_id, spell, damage_multiplier, charge_fraction },
            SpellReleased { tick, actor, release_id: release_id + 1, spell, damage_multiplier, charge_fraction },
            SpellReleased { tick, actor, release_id, spell: spell + 1, damage_multiplier, charge_fraction },
            SpellReleased { tick, actor, release_id, spell, damage_multiplier: damage_multiplier + 0.001, charge_fraction },
            SpellReleased { tick, actor, release_id, spell, damage_multiplier, charge_fraction: charge_fraction + 0.001 },
        ],
        MagickaSpent { tick, actor, amount } => vec![
            MagickaSpent { tick: tick + 1, actor, amount },
            MagickaSpent { tick, actor: actor + 1, amount },
            MagickaSpent { tick, actor, amount: amount + 0.001 },
        ],
        ReleaseUntracked { tick, actor, release_id, reason } => vec![
            ReleaseUntracked { tick: tick + 1, actor, release_id, reason },
            ReleaseUntracked { tick, actor: actor + 1, release_id, reason },
            ReleaseUntracked { tick, actor, release_id: release_id + 1, reason },
            ReleaseUntracked { tick, actor, release_id, reason: er_combat_core::cast::Registration::Duplicate },
            ReleaseUntracked { tick, actor, release_id, reason: er_combat_core::cast::Registration::Full },
            ReleaseUntracked { tick, actor, release_id, reason: er_combat_core::cast::Registration::Invalid },
        ],
        SelfEffectsApplied { tick, actor, applied } => vec![
            SelfEffectsApplied { tick: tick + 1, actor, applied },
            SelfEffectsApplied { tick, actor: actor + 1, applied },
            SelfEffectsApplied { tick, actor, applied: applied + 1 },
        ],
        CastEnded { tick, actor, how } => vec![
            CastEnded { tick: tick + 1, actor, how },
            CastEnded { tick, actor: actor + 1, how },
            CastEnded { tick, actor, how: if how == CastEnd::Finished { CastEnd::Fizzled } else { CastEnd::Finished } },
        ],
        ImpactResolved { tick, release_id, target, damage_multiplier, status_procs, poise_interrupted, stance_broken, modifiers_applied } => vec![
            ImpactResolved { tick: tick + 1, release_id, target, damage_multiplier, status_procs, poise_interrupted, stance_broken, modifiers_applied },
            ImpactResolved { tick, release_id: release_id + 1, target, damage_multiplier, status_procs, poise_interrupted, stance_broken, modifiers_applied },
            ImpactResolved { tick, release_id, target: target + 1, damage_multiplier, status_procs, poise_interrupted, stance_broken, modifiers_applied },
            ImpactResolved { tick, release_id, target, damage_multiplier: damage_multiplier + 0.001, status_procs, poise_interrupted, stance_broken, modifiers_applied },
            ImpactResolved { tick, release_id, target, damage_multiplier, status_procs: status_procs ^ 1, poise_interrupted, stance_broken, modifiers_applied },
            ImpactResolved { tick, release_id, target, damage_multiplier, status_procs, poise_interrupted: !poise_interrupted, stance_broken, modifiers_applied },
            ImpactResolved { tick, release_id, target, damage_multiplier, status_procs, poise_interrupted, stance_broken: !stance_broken, modifiers_applied },
            ImpactResolved { tick, release_id, target, damage_multiplier, status_procs, poise_interrupted, stance_broken, modifiers_applied: modifiers_applied + 1 },
        ],
        ImpactRefused { tick, release_id, target, reason } => vec![
            ImpactRefused { tick: tick + 1, release_id, target, reason },
            ImpactRefused { tick, release_id: release_id + 1, target, reason },
            ImpactRefused { tick, release_id, target: target + 1, reason },
            ImpactRefused { tick, release_id, target, reason: ImpactRefusal::TargetDead },
            ImpactRefused { tick, release_id, target, reason: ImpactRefusal::UnknownTarget },
            ImpactRefused { tick, release_id, target, reason: ImpactRefusal::Ledger(er_combat_core::cast::ImpactError::Duplicate) },
            ImpactRefused { tick, release_id, target, reason: ImpactRefusal::Ledger(er_combat_core::cast::ImpactError::TickOutOfRange) },
        ],
    }
}

#[test]
fn every_field_of_every_event_changes_the_bytes() {
    let (_, mut events) = run();
    events.extend(flood().1);
    let mut checked = BTreeSet::new();
    for event in &events {
        let original = canonical_event_bytes(std::slice::from_ref(event));
        for changed in perturbations(event) {
            if changed == *event {
                continue; // a perturbation that landed on the same value (e.g. the same refusal)
            }
            assert_ne!(canonical_event_bytes(&[changed.clone()]), original, "{event:?} -> {changed:?}");
            checked.insert(kind(event));
        }
    }
    assert_eq!(checked.len(), 13, "every kind was perturbed: {checked:?}");
    // Order matters too.
    let (first, second) = (events[0].clone(), events[1].clone());
    assert_ne!(canonical_event_bytes(&[first.clone(), second.clone()]), canonical_event_bytes(&[second, first]));
}
