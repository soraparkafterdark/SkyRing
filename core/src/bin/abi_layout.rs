//! Prints the Rust side of the C ABI as JSON: struct sizes, alignments and
//! field offsets, plus enum discriminants. `tools/abi_gate.py` compares this
//! with the C header's own layout and with the pinned snapshot.

use er_combat_core::ffi::{
    ErAction, ErButton, ErHitResult, ErHurt, ErInput, ErOutput, ErState, ErVec3, ErWorld,
};
use std::mem::{align_of, offset_of, size_of};

fn strukt(name: &str, size: usize, align: usize, fields: &[(&str, usize)]) -> String {
    let fields: Vec<String> = fields.iter().map(|(field, offset)| format!("\"{field}\":{offset}")).collect();
    format!("\"{name}\":{{\"size\":{size},\"align\":{align},\"fields\":{{{}}}}}", fields.join(","))
}

fn enumeration(name: &str, size: usize, values: &[(&str, u32)]) -> String {
    let values: Vec<String> = values.iter().map(|(variant, value)| format!("\"{variant}\":{value}")).collect();
    format!("\"{name}\":{{\"size\":{size},\"values\":{{{}}}}}", values.join(","))
}

macro_rules! layout {
    ($ty:ident: $($field:ident),+ $(,)?) => {
        strukt(
            stringify!($ty),
            size_of::<$ty>(),
            align_of::<$ty>(),
            &[$((stringify!($field), offset_of!($ty, $field))),+],
        )
    };
}

macro_rules! values {
    ($ty:ident: $($variant:ident),+ $(,)?) => {
        enumeration(
            stringify!($ty),
            size_of::<$ty>(),
            &[$((stringify!($variant), $ty::$variant as u32)),+],
        )
    };
}

fn main() {
    let structs = [
        layout!(ErVec3: x, y, z),
        layout!(ErButton: held, pressed, released),
        layout!(
            ErInput: move_x, move_y, camera_heading, dodge, jump, light, heavy, guard, crouch, walk, lock,
            two_hand_right, two_hand_left
        ),
        layout!(ErWorld: position, ground_z, has_target, target, in_combat),
        layout!(
            ErOutput: position, heading, state, action, action_frame, action_kind, action_dir, action_serial,
            speed, sprinting, crouching, guarding, invincible, airborne, interruptible, stamina, hit_live,
            sweep_a, sweep_b, hit_radius, damage_multiplier, stance_multiplier, hit_stop
        ),
    ];
    let enums = [
        values!(ErState: Ground, Action, Air, Dead),
        values!(
            ErAction: None, Roll, CrouchRoll, Backstep, SprintStop, Jump, Land, LightAttack, HeavyCharge,
            HeavyAttack, RunAttack, RollAttack, BackstepAttack, CrouchAttack, JumpAttackLand, GuardCounter,
            LeftAttack, GuardHit, GuardBreak, Hurt
        ),
        values!(ErHurt: Small, Middle, Large, Knockdown),
        values!(ErHitResult: Ignored, Dodged, Jumped, Blocked, GuardBroken, Hit),
    ];
    println!("{{\"structs\":{{{}}},\"enums\":{{{}}}}}", structs.join(","), enums.join(","));
}
