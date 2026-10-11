//! Byte-stability of the combat core over a scripted input trace.
//!
//! The same 600-tick trace is run twice from fresh `ErCombat` values and the
//! full `ErOutput` recording of every tick must match exactly. The trace also
//! has to actually exercise the core: it must produce a `Roll`, a `LightAttack`
//! and a heavy action (`HeavyCharge` or `HeavyAttack`).

use er_combat_core::ffi::{ErAction, ErCombat, ErInput, ErOutput, ErVec3, ErWorld};

const TICKS: usize = 600;
const DT: f32 = 1.0 / 60.0;

/// Build the scripted input for one full trace.
///
/// The trace is deterministic: every tick carries the same move stick, and
/// the buttons only change on the edges listed below.
///
///   tick  60: press dodge, release dodge (a tap → roll)
///   tick 120: press light, release light  (a light attack)
///   tick 180..240: hold heavy            (a charged heavy attack)
///   tick 300..360: hold guard            (blocking)
fn scripted_input(tick: usize) -> ErInput {
    let mut input = ErInput {
        move_y: 1.0,
        camera_heading: 0.0,
        ..Default::default()
    };

    match tick {
        // Dodge tap: down on 60, held on 61, up on 62.
        60 => input.dodge.pressed = 1,
        61 => input.dodge.held = 1,
        62 => {
            input.dodge.released = 1;
        }
        63..=63 => {
            // Nothing: the roll is already playing.
        }
        // Light attack tap: down on 120, held on 121, up on 122.
        120 => input.light.pressed = 1,
        121 => input.light.held = 1,
        122 => input.light.released = 1,
        // Heavy charge: held from 180 to 240 inclusive.
        180..=240 => {
            if tick == 180 {
                input.heavy.pressed = 1;
            } else {
                input.heavy.held = 1;
            }
        }
        // Guard: held from 300 to 360 inclusive.
        300..=360 => {
            if tick == 300 {
                input.guard.pressed = 1;
            } else {
                input.guard.held = 1;
            }
        }
        _ => {}
    }

    input
}

/// Run the full 600-tick trace and return one `ErOutput` per tick.
fn run_trace() -> Vec<ErOutput> {
    let mut recording = Vec::with_capacity(TICKS);
    let mut world = ErWorld::default();
    let mut combat = ErCombat::new(ErVec3::default());

    for tick in 0..TICKS {
        let input = scripted_input(tick);
        let out = combat.update(DT, &input, &world);
        // Feed the result back the way the bridge does when nothing blocks.
        world.position = out.position;
        recording.push(out);
    }

    recording
}

/// Assert the trace really exercised the combat core.
fn assert_trace_is_meaningful(recording: &[ErOutput]) {
    let mut saw_roll = false;
    let mut saw_light = false;
    let mut saw_heavy = false;

    for out in recording {
        saw_roll |= out.action_kind == ErAction::Roll;
        saw_light |= out.action_kind == ErAction::LightAttack;
        saw_heavy |= matches!(
            out.action_kind,
            ErAction::HeavyCharge | ErAction::HeavyAttack
        );
    }

    assert!(saw_roll, "no Roll in the trace");
    assert!(saw_light, "no LightAttack in the trace");
    assert!(saw_heavy, "no HeavyCharge or HeavyAttack in the trace");
}

#[test]
fn scripted_trace_is_byte_stable() {
    let first = run_trace();
    let second = run_trace();

    assert_eq!(first.len(), TICKS, "the recording must cover all 600 ticks");
    assert_eq!(second.len(), TICKS, "the recording must cover all 600 ticks");

    assert_trace_is_meaningful(&first);
    assert_trace_is_meaningful(&second);

    // Two fresh runs over the identical scripted input must produce byte-for-
    // byte identical recordings. Anything nondeterministic (uninitialised
    // memory, a global RNG, a HashMap iteration order) shows up here.
    for (tick, (a, b)) in first.iter().zip(second.iter()).enumerate() {
        assert_eq!(
            format!("{:?}", a),
            format!("{:?}", b),
            "tick {tick} diverged between the two recordings"
        );
    }
}