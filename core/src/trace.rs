//! Canonical combat trace recording and headless replay.
//!
//! A trace is a tick-indexed record of one scripted `ErCombat` run. Everything
//! is wall-clock free: the tick counter and the fixed 60 Hz step are the only
//! clocks, so the same scripted input always serializes to the same bytes.
//!
//! Presentation-observation events (first/third person, camera distance) are
//! carried alongside the recording but are never fed back into the simulation.
//! That is what makes them mechanically inert: toggling them leaves every
//! action, motion, i-frame, stamina and contact byte identical.

use crate::ffi::{ErAction, ErCombat, ErInput, ErOutput, ErVec3, ErWorld};

/// Fixed simulation step, 60 Hz. The only clock a trace carries.
pub const TRACE_DT: f32 = 1.0 / 60.0;

/// A perspective marker. The simulation never reads this; it exists so the
/// recorder and the bridge can tag a recording without affecting it, which is
/// the whole proof that perspective is presentation, not combat state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerspectiveMarker {
    pub first_person: bool,
    pub camera_distance: u32,
}

/// Record `ticks` snapshots of one scripted combat run.
///
/// `perspective` is accepted so the bridge can tag a recording, but it is
/// deliberately never looked at: it cannot reach `Player::step`.
pub fn record_trace(
    ticks: u32,
    input_for: &impl Fn(u32) -> ErInput,
    perspective: PerspectiveMarker,
) -> Vec<ErOutput> {
    let _ = perspective;
    let mut recording = Vec::with_capacity(ticks as usize);
    let mut world = ErWorld::default();
    let mut combat = ErCombat::new(ErVec3::default());

    for tick in 0..ticks {
        let input = input_for(tick);
        let out = combat.update(TRACE_DT, &input, &world);
        // Feed the result back the way the bridge does when nothing blocks.
        world.position = out.position;
        recording.push(out);
    }

    recording
}

/// Canonical byte serialization of a recording.
///
/// The `Debug` output of `ErOutput` is used on purpose: every field is a plain
/// scalar or fixed-size array with a fixed declaration order, so it is a
/// stable, sorted, map-free representation. Two recordings that are truly
/// equal produce the same bytes, and anything nondeterministic (uninitialised
/// memory, a global RNG, a HashMap iteration order) shows up as a mismatch.
pub fn canonical_bytes(recording: &[ErOutput]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(recording.len() * 64 + 16);
    buf.extend_from_slice(b"skyring-trace v1\n");
    for out in recording {
        buf.extend_from_slice(format!("{:?}", out).as_bytes());
        buf.push(b'\n');
    }
    buf
}

/// Headless replay: record twice from the same scripted input and confirm the
/// canonical bytes are byte-for-byte identical.
pub fn replay_is_deterministic(
    ticks: u32,
    input_for: &impl Fn(u32) -> ErInput,
    perspective: PerspectiveMarker,
) -> bool {
    let first = canonical_bytes(&record_trace(ticks, input_for, perspective));
    let second = canonical_bytes(&record_trace(ticks, input_for, perspective));
    first == second
}

/// A scripted input trace that exercises a roll, a light attack, a charged
/// heavy, and a guard hold -- the same shape as the byte-stability test.
pub fn scripted_trace_input(tick: u32) -> ErInput {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_is_byte_identical() {
        let ticks = 600;
        assert!(replay_is_deterministic(
            ticks,
            &scripted_trace_input,
            PerspectiveMarker::default()
        ));
    }

    #[test]
    fn canonical_bytes_are_stable_across_calls() {
        let a = record_trace(120, &scripted_trace_input, PerspectiveMarker::default());
        let once = canonical_bytes(&a);
        let twice = canonical_bytes(&a);
        assert_eq!(once, twice);
        // The header pins the format version.
        assert!(once.starts_with(b"skyring-trace v1\n"));
    }

    #[test]
    fn perspective_marker_is_carried_but_ignored() {
        // The marker is accepted and stored; the recorder must not branch on it.
        let first = record_trace(
            60,
            &scripted_trace_input,
            PerspectiveMarker {
                first_person: true,
                camera_distance: 300,
            },
        );
        let third = record_trace(
            60,
            &scripted_trace_input,
            PerspectiveMarker {
                first_person: false,
                camera_distance: 120,
            },
        );
        assert_eq!(canonical_bytes(&first), canonical_bytes(&third));
    }
}