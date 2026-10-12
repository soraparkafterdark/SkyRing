//! Camera-relative movement through the C-facing core, including heading
//! wraparound. The bridge normalizes the heading it sends, but the core must
//! also treat a heading and the same heading plus whole turns as one heading,
//! and must never depend on which perspective produced it.

use er_combat_core::ffi::{ErAction, ErCombat, ErInput, ErVec3, ErWorld};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

const DT: f32 = 1.0 / 60.0;
const TICKS: usize = 90;

/// Where the player ends up after holding the stick for `TICKS` frames.
fn run(heading: f32, move_x: f32, move_y: f32) -> ErVec3 {
    let mut combat = ErCombat::new(ErVec3::default());
    let mut world = ErWorld::default();
    let input = ErInput { move_x, move_y, camera_heading: heading, ..Default::default() };
    let mut position = ErVec3::default();
    for _ in 0..TICKS {
        let out = combat.update(DT, &input, &world);
        world.position = out.position;
        position = out.position;
    }
    position
}

fn horizontal_length(p: ErVec3) -> f32 {
    (p.x * p.x + p.y * p.y).sqrt()
}

/// Cosine of the angle between the travelled direction and `(dx, dy)`.
fn alignment(p: ErVec3, dx: f32, dy: f32) -> f32 {
    (p.x * dx + p.y * dy) / (horizontal_length(p) * (dx * dx + dy * dy).sqrt())
}

#[test]
fn forward_stick_follows_heading_in_skyrim_axes() {
    // Skyrim: heading 0 faces +Y and turning clockwise goes toward +X.
    for heading in [0.0, 0.7, FRAC_PI_2, 2.4, PI, -PI, -FRAC_PI_2, -2.4] {
        let end = run(heading, 0.0, 1.0);
        assert!(horizontal_length(end) > 100.0, "heading {heading}: barely moved, {end:?}");
        assert!(
            alignment(end, heading.sin(), heading.cos()) > 0.98,
            "heading {heading}: travelled {end:?}, expected direction ({}, {})",
            heading.sin(),
            heading.cos()
        );
    }
}

#[test]
fn right_stick_is_clockwise_of_the_heading() {
    for heading in [0.0, FRAC_PI_2, 2.4, -1.0] {
        let end = run(heading, 1.0, 0.0);
        assert!(
            alignment(end, heading.cos(), -heading.sin()) > 0.98,
            "heading {heading}: strafing right travelled {end:?}"
        );
    }
}

#[test]
fn headings_that_differ_by_whole_turns_move_identically() {
    for base in [0.3, FRAC_PI_2, 3.0, -3.0] {
        let reference = run(base, 0.0, 1.0);
        for turns in [-2.0, -1.0, 1.0, 2.0] {
            let wrapped = run(base + turns * TAU, 0.0, 1.0);
            let drift = ((wrapped.x - reference.x).powi(2) + (wrapped.y - reference.y).powi(2)).sqrt();
            assert!(drift < 1.5, "heading {base} + {turns} turns drifted {drift} units: {wrapped:?} vs {reference:?}");
        }
    }
}

#[test]
fn strafing_is_not_mirrored() {
    // Regression: Skyrim is right-handed, the simulation is Y-up. Mapping one
    // into the other with a reflection made the right stick strafe toward -X
    // at heading 0 (facing +Y, where right is +X).
    let right = run(0.0, 1.0, 0.0);
    assert!(right.x > 100.0 && right.y.abs() < 20.0, "right stick at heading 0 went {right:?}");
    let left = run(0.0, -1.0, 0.0);
    assert!(left.x < -100.0 && left.y.abs() < 20.0, "left stick at heading 0 went {left:?}");
}

/// Taps dodge with the stick held while locked onto a target 10 m north, and
/// reports the roll's direction label and where the player ended up. Left and
/// right labels only exist while locked on; otherwise a roll turns to face its
/// direction and is always "front".
fn roll(heading: f32, move_x: f32, move_y: f32) -> (ErAction, u32, ErVec3, ErVec3) {
    let mut combat = ErCombat::new(ErVec3::default());
    let mut world = ErWorld { has_target: 1, target: ErVec3 { x: 0.0, y: 700.0, z: 0.0 }, ..Default::default() };
    let mut seen = (ErAction::None, 0, ErVec3::default(), ErVec3::default());
    for tick in 0..60 {
        let mut input = ErInput { move_x, move_y, camera_heading: heading, ..Default::default() };
        match tick {
            2 => {
                input.dodge.pressed = 1;
                input.dodge.held = 1;
            }
            3 => input.dodge.released = 1,
            _ => {}
        }
        input.lock = u8::from(tick == 0);
        let out = combat.update(DT, &input, &world);
        world.position = out.position;
        if out.action_kind == ErAction::Roll && seen.0 != ErAction::Roll {
            seen.0 = out.action_kind;
            seen.1 = out.action_dir;
        }
        // Furthest the player got to either side: the roll's own displacement.
        seen.2.x = seen.2.x.min(out.position.x);
        seen.3.x = seen.3.x.max(out.position.x);
    }
    seen
}

#[test]
fn roll_direction_label_matches_the_way_the_player_moves() {
    // action_dir: 0 front, 1 back, 2 left, 3 right, relative to the player.
    // The bridge picks the Skyrim animation from this label, so it has to
    // agree with the displacement, not just with the simulation's own sense of left.
    let (kind, dir, min, max) = roll(0.0, 1.0, 0.0);
    assert_eq!(kind, ErAction::Roll);
    assert_eq!(dir, 3, "rolling toward +X while facing the target to the north is a right roll");
    assert!(max.x > 10.0 && min.x > -5.0, "right roll moved x in [{}, {}]", min.x, max.x);

    let (kind, dir, min, max) = roll(0.0, -1.0, 0.0);
    assert_eq!(kind, ErAction::Roll);
    assert_eq!(dir, 2, "rolling toward -X while facing the target to the north is a left roll");
    assert!(min.x < -10.0 && max.x < 5.0, "left roll moved x in [{}, {}]", min.x, max.x);

    // The stick is camera-relative: with the camera looking east, stick-forward
    // is east, which is a right roll relative to a player facing north.
    let (_, dir, _, max) = roll(FRAC_PI_2, 0.0, 1.0);
    assert_eq!(dir, 3, "camera-forward toward +X is still a right roll");
    assert!(max.x > 10.0);
}

#[test]
fn camera_heading_alone_decides_movement() {
    // Perspective is not an input: two runs with the same heading are
    // bit-identical, so first and third person cannot diverge in the core.
    let first = run(1.2, 0.3, 0.9);
    let second = run(1.2, 0.3, 0.9);
    assert_eq!((first.x, first.y, first.z), (second.x, second.y, second.z));
}
