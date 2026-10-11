//! Print action-economy measurements from the active action table.
//!
//! With no owner-generated `extracted.rs`, this reports the synthetic fixture.
//! With one present, the same command reports the locally extracted table.

use er_combat_core::balance::{self, ActionMetrics};
use er_combat_core::sim::data::{AttackKind, Moveset, ANIM_FPS, WEAPONS};

const ATTACKS: &[AttackKind] = &[
    AttackKind::Light1,
    AttackKind::Light2,
    AttackKind::Light3,
    AttackKind::Light4,
    AttackKind::Light5,
    AttackKind::Light6,
    AttackKind::CrouchAttack,
    AttackKind::RunLight,
    AttackKind::RunHeavy,
    AttackKind::RollAttack,
    AttackKind::BackstepAttack,
    AttackKind::Heavy1,
    AttackKind::Heavy1Charge,
    AttackKind::Heavy2,
    AttackKind::Heavy2Charge,
    AttackKind::GuardCounter,
    AttackKind::JumpLightLand,
    AttackKind::JumpHeavyLand,
    AttackKind::LeftLight1,
    AttackKind::PairedLight1,
    AttackKind::PairedRun,
    AttackKind::PairedRoll,
    AttackKind::PairedBackstep,
    AttackKind::PairedJumpLand,
];

fn metrics(moveset: Moveset, kind: AttackKind) -> Option<ActionMetrics> {
    let definition = moveset.attack(kind)?;
    if definition.hits.is_empty() {
        return None;
    }
    let motion_values = definition.hits.iter().map(|hit| hit.mv).collect::<Vec<_>>();
    let stamina = definition.hits.iter().map(|hit| hit.stamina).sum::<f32>();
    balance::action_metrics(kind, &motion_values, definition.total / ANIM_FPS, stamina)
}

fn ratio(value: f32, baseline: f32) -> f32 {
    if baseline > 0.0 {
        value / baseline
    } else {
        0.0
    }
}

fn main() {
    println!(
        "weapon\tgrip\taction\trole\tseconds\thits\tdamage_units\tdps\tdps_vs_light\tdamage_per_stamina\tstance_per_second"
    );
    for (weapon, info) in WEAPONS.iter().enumerate() {
        for two_hand in [false, true] {
            let moveset = Moveset {
                weapon: weapon as u8,
                two_hand,
            };
            let Some(light) = metrics(moveset, AttackKind::Light1) else {
                continue;
            };
            for &kind in ATTACKS {
                let Some(action) = metrics(moveset, kind) else {
                    continue;
                };
                let definition = moveset.attack(kind).expect("metrics require an action");
                println!(
                    "{}\t{}\t{:?}\t{:?}\t{:.3}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.4}\t{:.3}",
                    info.name,
                    if two_hand { "2H" } else { "1H" },
                    kind,
                    balance::role(kind),
                    definition.total / ANIM_FPS,
                    definition.hits.len(),
                    action.total_damage,
                    action.damage_per_second,
                    ratio(action.damage_per_second, light.damage_per_second),
                    action.damage_per_stamina,
                    action.stance_per_second,
                );
            }
        }
    }
}
