//! Translation policy between Elden Ring action data and Skyrim damage.
//!
//! Skyrim owns absolute weapon damage, perks, armour, difficulty, and health.
//! The combat core exports only a bounded multiplier describing the relative
//! value of the action that produced a hit. Source motion values are evidence,
//! not permission to bypass Skyrim's damage economy.

use crate::sim::data::AttackKind;

/// Gameplay role used to budget an action across damage, safety, stamina,
/// reach, and stance pressure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionRole {
    Light,
    Heavy,
    ChargedHeavy,
    CrouchPoke,
    Running,
    Evasive,
    JumpLight,
    JumpHeavy,
    GuardCounter,
    Paired,
}

/// Allowed Skyrim-facing damage range for an action role.
///
/// These are deliberately conservative starting envelopes. Telemetry may move
/// an envelope, but individual weapons should not escape it through ad-hoc
/// patches. Stance pressure is separate so heavies can feel powerful without
/// also dominating sustained health damage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionEnvelope {
    pub damage_min: f32,
    pub damage_default: f32,
    pub damage_max: f32,
    pub stance_multiplier: f32,
}

impl ActionRole {
    pub const fn envelope(self) -> ActionEnvelope {
        use ActionRole::*;
        match self {
            Light => ActionEnvelope {
                damage_min: 0.90,
                damage_default: 1.00,
                damage_max: 1.15,
                stance_multiplier: 1.00,
            },
            Heavy => ActionEnvelope {
                damage_min: 1.35,
                damage_default: 1.55,
                damage_max: 1.85,
                stance_multiplier: 1.80,
            },
            ChargedHeavy => ActionEnvelope {
                damage_min: 1.80,
                damage_default: 2.10,
                damage_max: 2.50,
                stance_multiplier: 2.80,
            },
            CrouchPoke => ActionEnvelope {
                damage_min: 0.80,
                damage_default: 0.90,
                damage_max: 1.00,
                stance_multiplier: 0.75,
            },
            Running => ActionEnvelope {
                damage_min: 1.00,
                damage_default: 1.10,
                damage_max: 1.25,
                stance_multiplier: 1.15,
            },
            Evasive => ActionEnvelope {
                damage_min: 0.90,
                damage_default: 1.00,
                damage_max: 1.10,
                stance_multiplier: 0.90,
            },
            JumpLight => ActionEnvelope {
                damage_min: 1.15,
                damage_default: 1.25,
                damage_max: 1.45,
                stance_multiplier: 1.50,
            },
            JumpHeavy => ActionEnvelope {
                damage_min: 1.65,
                damage_default: 1.85,
                damage_max: 2.20,
                stance_multiplier: 2.40,
            },
            GuardCounter => ActionEnvelope {
                damage_min: 1.50,
                damage_default: 1.75,
                damage_max: 2.00,
                stance_multiplier: 2.80,
            },
            // Paired attacks can produce more than one contact. This envelope
            // is per contact; the action-level harness must enforce the total.
            Paired => ActionEnvelope {
                damage_min: 0.65,
                damage_default: 0.75,
                damage_max: 0.90,
                stance_multiplier: 0.65,
            },
        }
    }
}

pub const fn role(kind: AttackKind) -> ActionRole {
    use AttackKind::*;
    match kind {
        Light1 | Light2 | Light3 | Light4 | Light5 | Light6 | LeftLight1 | LeftLight2
        | LeftLight3 | LeftLight4 | LeftLight5 | LeftLight6 => ActionRole::Light,
        Heavy1 | Heavy2 | RunHeavy => ActionRole::Heavy,
        Heavy1Charge | Heavy2Charge => ActionRole::ChargedHeavy,
        CrouchAttack => ActionRole::CrouchPoke,
        RunLight => ActionRole::Running,
        RollAttack | BackstepAttack => ActionRole::Evasive,
        JumpLightLand | JumpLightLandShort => ActionRole::JumpLight,
        JumpHeavyLand | JumpHeavyLandShort => ActionRole::JumpHeavy,
        GuardCounter => ActionRole::GuardCounter,
        PairedLight1 | PairedLight2 | PairedLight3 | PairedLight4 | PairedLight5 | PairedLight6
        | PairedRun | PairedRoll | PairedBackstep | PairedJumpLand | PairedJumpLandShort => {
            ActionRole::Paired
        }
    }
}

/// Translate an extracted motion value into a bounded multiplier over the
/// damage Skyrim would normally assign to the weapon hit.
pub fn damage_multiplier(kind: AttackKind, source_motion_value: f32) -> f32 {
    let envelope = role(kind).envelope();
    let source = if source_motion_value.is_finite() && source_motion_value > 0.0 {
        source_motion_value
    } else {
        envelope.damage_default
    };
    source.clamp(envelope.damage_min, envelope.damage_max)
}

pub fn stance_multiplier(kind: AttackKind) -> f32 {
    role(kind).envelope().stance_multiplier
}

/// Comparable, engine-independent measurements for one complete action.
/// Values are expressed in light-hit equivalents so they can be compared
/// before a particular Skyrim weapon, perk loadout, or difficulty is chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionMetrics {
    pub total_damage: f32,
    pub damage_per_second: f32,
    pub damage_per_stamina: f32,
    pub stance_per_second: f32,
}

/// Measure an action across all of its contacts. Paired and multi-hit moves
/// must fit one action budget rather than receiving an unexamined full weapon
/// hit for every contact.
pub fn action_metrics(
    kind: AttackKind,
    source_motion_values: &[f32],
    duration_seconds: f32,
    stamina_cost: f32,
) -> Option<ActionMetrics> {
    if source_motion_values.is_empty()
        || !duration_seconds.is_finite()
        || duration_seconds <= 0.0
        || !stamina_cost.is_finite()
        || stamina_cost <= 0.0
    {
        return None;
    }
    let total_damage = source_motion_values
        .iter()
        .map(|&source| damage_multiplier(kind, source))
        .sum::<f32>();
    let total_stance = stance_multiplier(kind) * source_motion_values.len() as f32;
    Some(ActionMetrics {
        total_damage,
        damage_per_second: total_damage / duration_seconds,
        damage_per_stamina: total_damage / stamina_cost,
        stance_per_second: total_stance / duration_seconds,
    })
}

/// Imported gear gets one small bonus at the armament layer. Unique skills
/// and unusually strong status access consume that budget rather than stacking
/// freely on top of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArmamentTier {
    Standard,
    Exceptional,
    Legendary,
}

pub const MAX_ARMAMENT_MULTIPLIER: f32 = 1.15;

pub fn armament_multiplier(tier: ArmamentTier, feature_budget: f32) -> f32 {
    let requested = match tier {
        ArmamentTier::Standard => 1.00,
        ArmamentTier::Exceptional => 1.05,
        ArmamentTier::Legendary => 1.10,
    };
    let spent_elsewhere = feature_budget.clamp(0.0, requested - 1.0);
    (requested - spent_elsewhere).clamp(1.0, MAX_ARMAMENT_MULTIPLIER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_roles_have_the_intended_per_hit_order() {
        let light = damage_multiplier(AttackKind::Light1, 1.0);
        let crouch = damage_multiplier(AttackKind::CrouchAttack, 1.0);
        let heavy = damage_multiplier(AttackKind::Heavy1, 1.0);
        let charged = damage_multiplier(AttackKind::Heavy1Charge, 1.0);
        assert!(
            crouch <= light,
            "crouch poke earns reach and speed, not free damage"
        );
        assert!(heavy > light);
        assert!(charged > heavy);
    }

    #[test]
    fn hostile_or_broken_source_values_cannot_escape_role_envelopes() {
        for source in [
            f32::NEG_INFINITY,
            -10.0,
            0.0,
            f32::NAN,
            999.0,
            f32::INFINITY,
        ] {
            let value = damage_multiplier(AttackKind::Light1, source);
            let envelope = ActionRole::Light.envelope();
            assert!(value.is_finite());
            assert!((envelope.damage_min..=envelope.damage_max).contains(&value));
        }
    }

    #[test]
    fn heavies_get_more_stance_budget_than_repeatable_attacks() {
        assert!(stance_multiplier(AttackKind::Heavy1) > stance_multiplier(AttackKind::Light1));
        assert!(
            stance_multiplier(AttackKind::GuardCounter) > stance_multiplier(AttackKind::Heavy1)
        );
        assert!(
            stance_multiplier(AttackKind::CrouchAttack) < stance_multiplier(AttackKind::Light1)
        );
    }

    #[test]
    fn fun_bias_is_small_single_and_feature_aware() {
        assert_eq!(armament_multiplier(ArmamentTier::Standard, 0.0), 1.0);
        assert_eq!(armament_multiplier(ArmamentTier::Exceptional, 0.0), 1.05);
        assert_eq!(armament_multiplier(ArmamentTier::Legendary, 0.0), 1.10);
        assert!((armament_multiplier(ArmamentTier::Legendary, 0.04) - 1.06).abs() < 1e-6);
        assert_eq!(armament_multiplier(ArmamentTier::Legendary, 1.0), 1.0);
        assert!(armament_multiplier(ArmamentTier::Legendary, -100.0) <= MAX_ARMAMENT_MULTIPLIER);
    }

    #[test]
    fn metrics_budget_the_whole_multi_hit_action() {
        let light = action_metrics(AttackKind::Light1, &[1.0], 0.8, 12.0).unwrap();
        let paired = action_metrics(AttackKind::PairedLight1, &[1.0, 1.0], 1.1, 20.0).unwrap();
        assert_eq!(light.total_damage, 1.0);
        assert_eq!(paired.total_damage, 1.8);
        assert!(paired.damage_per_second < light.damage_per_second * 1.4);
        assert!(paired.damage_per_stamina < light.damage_per_stamina * 1.1);
    }

    #[test]
    fn invalid_action_measurements_fail_instead_of_inventing_results() {
        assert!(action_metrics(AttackKind::Light1, &[], 1.0, 10.0).is_none());
        assert!(action_metrics(AttackKind::Light1, &[1.0], 0.0, 10.0).is_none());
        assert!(action_metrics(AttackKind::Light1, &[1.0], 1.0, 0.0).is_none());
    }
}
