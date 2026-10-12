//! Perspective markers cannot alter core combat output.
//!
//! First- and third-person are presentations of one authoritative action clock
//! and geometry. This test records the same scripted combat twice with the
//! perspective marker toggled and asserts that every action, motion, i-frame,
//! stamina and contact byte is identical. A perspective marker that reached
//! the simulation would fail this test on sight.

use er_combat_core::ffi::{ErAction, ErCombat, ErInput, ErOutput, ErWorld};
use er_combat_core::trace::{canonical_bytes, record_trace, PerspectiveMarker};

const TICKS: u32 = 600;

fn scripted_input(tick: u32) -> ErInput {
    let mut input = ErInput {
        move_y: 1.0,
        ..Default::default()
    };

    match tick {
        60 => input.dodge.pressed = 1,
        61 => input.dodge.held = 1,
        62 => input.dodge.released = 1,
        120 => input.light.pressed = 1,
        121 => input.light.held = 1,
        122 => input.light.released = 1,
        180..=240 => {
            if tick == 180 {
                input.heavy.pressed = 1;
            } else {
                input.heavy.held = 1;
            }
        }
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

/// The combat-state fields that perspective must never touch.
fn core_fields(out: &ErOutput) -> (ErAction, f32, f32, u8, u8) {
    (
        out.action_kind,
        out.action_frame,
        out.stamina,
        out.hit_live,
        out.invincible,
    )
}

#[test]
fn perspective_markers_are_mechanically_inert() {
    let first_person = PerspectiveMarker {
        first_person: true,
        camera_distance: 300,
    };
    let third_person = PerspectiveMarker {
        first_person: false,
        camera_distance: 120,
    };

    let first = record_trace(TICKS, &scripted_input, first_person);
    let third = record_trace(TICKS, &scripted_input, third_person);

    assert_eq!(
        first.len(),
        third.len(),
        "both recordings must cover the same number of ticks"
    );

    // Byte-for-byte: the whole recording, including motion and contact geometry.
    assert_eq!(
        canonical_bytes(&first),
        canonical_bytes(&third),
        "perspective markers must not alter any core byte"
    );

    // Field-by-field, so a future refactor that happens to canonicalize
    // differently still catches a real regression.
    for (tick, (a, b)) in first.iter().zip(third.iter()).enumerate() {
        assert_eq!(
            core_fields(a),
            core_fields(b),
            "tick {tick} diverged between first- and third-person"
        );
        assert_eq!(a.position, b.position, "tick {tick} motion diverged");
        assert_eq!(a.action, b.action, "tick {tick} action identity diverged");
        assert_eq!(a.action_serial, b.action_serial, "tick {tick} serial diverged");
        assert_eq!(a.sweep_a, b.sweep_a, "tick {tick} contact geometry diverged");
        assert_eq!(a.sweep_b, b.sweep_b, "tick {tick} contact geometry diverged");
    }

    // And the trace has to actually exercise the core, or the comparison is
    // vacuous.
    let mut saw_roll = false;
    let mut saw_light = false;
    let mut saw_heavy = false;
    for out in &first {
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
fn perspective_marker_does_not_change_a_single_tick() {
    // One tick, two markers: the output must be indistinguishable.
    let a = record_trace(1, &scripted_input, PerspectiveMarker { first_person: true, camera_distance: 999 });
    let b = record_trace(1, &scripted_input, PerspectiveMarker { first_person: false, camera_distance: 1 });
    assert_eq!(canonical_bytes(&a), canonical_bytes(&b));
}