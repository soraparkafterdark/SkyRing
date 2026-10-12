//! A host-pool session records and replays identically: the trace carries the pool Skyrim
//! supplied and the core's spend, so the runtime log can be compared with headless replay.

use er_combat_core::ffi::{ErAction, ErCombat, ErInput, ErVec3, ErWorld};
use er_combat_core::sim::data::{MAX_STAMINA, ROLL_COST};
use er_combat_core::trace::{CombatEvent, CombatTrace, TraceError, TraceRecorder, TraceReplay, TraceTick};

const DT: f32 = 1.0 / 60.0;
const TICKS: usize = 120;
const POOL: f32 = 100.0;

fn host_world() -> ErWorld {
    ErWorld { stamina_current: POOL, stamina_max: POOL, ..Default::default() }
}

/// A roll (stick forward, dodge tapped on ticks 5-6), then idling, against a 100-point host pool.
fn recorded_trace() -> (CombatTrace, usize) {
    let initial = ErVec3::default();
    let mut recorder = TraceRecorder::new(initial).expect("valid initial position");
    let mut source = ErCombat::new(initial);
    let mut world = host_world();
    let mut roll_tick = None;
    for tick in 0..TICKS {
        let mut input = ErInput { move_y: 1.0, ..Default::default() };
        match tick {
            5 => {
                input.dodge.pressed = 1;
                input.dodge.held = 1;
            }
            6 => input.dodge.released = 1,
            _ => {}
        }
        let output = source.update(DT, &input, &world);
        if output.action_kind == ErAction::Roll && roll_tick.is_none() {
            roll_tick = Some(tick);
        }
        recorder.record(TraceTick::new(input, world)).expect("valid facts");
        world.position = output.position;
    }
    (recorder.finish(), roll_tick.expect("the scripted roll starts"))
}

#[test]
fn the_pool_and_the_spend_are_in_the_replayed_log() {
    let (trace, roll_tick) = recorded_trace();
    let log = TraceReplay::replay(&trace).expect("replays");

    let observed: Vec<(f32, f32)> = log
        .events()
        .iter()
        .filter_map(|event| match event {
            CombatEvent::WorldObserved { stamina_current, stamina_max, .. } => Some((*stamina_current, *stamina_max)),
            _ => None,
        })
        .collect();
    assert_eq!(observed.len(), TICKS);
    assert!(observed.iter().all(|&pool| pool == (POOL, POOL)), "every tick records the host pool");

    let spent: Vec<(u64, f32)> = log
        .events()
        .iter()
        .filter_map(|event| match event {
            CombatEvent::StaminaSpent { tick, amount } => Some((*tick, *amount)),
            _ => None,
        })
        .collect();
    assert_eq!(spent.len(), 1, "one roll, one spend: {spent:?}");
    assert_eq!(spent[0].0, roll_tick as u64, "the spend is logged on the tick the roll starts");
    let expected = ROLL_COST * POOL / MAX_STAMINA;
    assert!((spent[0].1 - expected).abs() < 1e-3, "spent {} expected {expected}", spent[0].1);
}

#[test]
fn a_host_pool_session_is_byte_stable_through_encode_decode_and_replay() {
    let (trace, _) = recorded_trace();
    let bytes = trace.canonical_bytes();
    let decoded = CombatTrace::from_canonical_bytes(&bytes).expect("decodes");
    assert_eq!(bytes, decoded.canonical_bytes());
    let first = TraceReplay::replay(&trace).expect("first");
    let second = TraceReplay::replay(&decoded).expect("second");
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(first.mechanical_bytes(), second.mechanical_bytes());
}

#[test]
fn the_decoded_trace_gives_the_core_the_same_pool() {
    let (trace, _) = recorded_trace();
    let decoded = CombatTrace::from_canonical_bytes(&trace.canonical_bytes()).expect("decodes");
    for tick in decoded.ticks() {
        assert_eq!((tick.world.stamina_current, tick.world.stamina_max), (POOL, POOL));
    }
}

#[test]
fn a_session_without_a_host_pool_records_the_internal_one_and_still_replays() {
    let initial = ErVec3::default();
    let mut recorder = TraceRecorder::new(initial).expect("valid");
    let mut source = ErCombat::new(initial);
    let mut world = ErWorld::default();
    for tick in 0..60 {
        let mut input = ErInput { move_y: 1.0, ..Default::default() };
        if tick == 5 {
            input.dodge.pressed = 1;
            input.dodge.held = 1;
        }
        if tick == 6 {
            input.dodge.released = 1;
        }
        let output = source.update(DT, &input, &world);
        recorder.record(TraceTick::new(input, world)).expect("valid");
        world.position = output.position;
    }
    let log = TraceReplay::replay(&recorder.finish()).expect("replays");
    let total: f32 = log
        .events()
        .iter()
        .filter_map(|event| match event {
            CombatEvent::StaminaSpent { amount, .. } => Some(*amount),
            _ => None,
        })
        .sum();
    assert!((total - ROLL_COST).abs() < 1e-3, "without a pool the spend is in the core's own units");
}

#[test]
fn non_finite_pool_facts_are_not_recordable() {
    for (current, max) in [(f32::NAN, POOL), (POOL, f32::NAN), (f32::INFINITY, POOL), (POOL, f32::NEG_INFINITY)] {
        let mut recorder = TraceRecorder::new(ErVec3::default()).expect("valid");
        let world = ErWorld { stamina_current: current, stamina_max: max, ..Default::default() };
        let result = recorder.record(TraceTick::new(ErInput::default(), world));
        assert!(matches!(result, Err(TraceError::NonFinite { .. })), "({current}, {max}) -> {result:?}");
    }
}
