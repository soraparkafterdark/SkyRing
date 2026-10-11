//! One delivery-neutral hit contract for melee, magic, throwables, hazards,
//! and creatures.
//!
//! This layer resolves semantic consequences but never subtracts health.
//! Skyrim receives the bounded health-damage multiplier and elemental shares,
//! then applies its normal weapon/spell, resistance, perk, and difficulty path.

use crate::balance::MAX_ARMAMENT_MULTIPLIER;
use crate::effects::{
    BuildupResult, ModifiedStat, Modifier, ModifierSet, StatusDefinition, StatusKind, StatusProc,
    StatusSet, STATUS_COUNT,
};
use crate::posture::{
    PoiseDefinition, PoiseResult, PoiseState, StanceDefinition, StanceResult, StanceState,
};

pub const ELEMENT_COUNT: usize = 5;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageElement {
    Physical = 0,
    Magic = 1,
    Fire = 2,
    Lightning = 3,
    Holy = 4,
}

impl DamageElement {
    pub const ALL: [Self; ELEMENT_COUNT] = [
        Self::Physical,
        Self::Magic,
        Self::Fire,
        Self::Lightning,
        Self::Holy,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Shares of one Skyrim-owned damage request. The shares must add to at most
/// one; the bridge decides how each semantic channel maps to Skyrim damage
/// types and resistances. Holy deliberately remains distinct in the contract.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ElementalProfile {
    shares: [f32; ELEMENT_COUNT],
}

impl ElementalProfile {
    pub fn from_channels(channels: &[(DamageElement, f32)]) -> Option<Self> {
        let mut profile = Self::default();
        for &(element, share) in channels {
            if !share.is_finite() || share < 0.0 {
                return None;
            }
            profile.shares[element.index()] += share;
        }
        let total = profile.shares.iter().sum::<f32>();
        (total <= 1.0 + f32::EPSILON).then_some(profile)
    }

    pub fn share(self, element: DamageElement) -> f32 {
        self.shares[element.index()]
    }

    pub fn total(self) -> f32 {
        self.shares.iter().sum()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HitEvent {
    pub source: u64,
    pub target: u64,
    pub action_id: u32,
    pub hit_id: u32,
    /// Bounded action multiplier produced by the balance translator.
    pub damage_multiplier: f32,
    /// One small imported-armament bonus, hard-capped again at resolution.
    pub armament_multiplier: f32,
    pub elements: ElementalProfile,
    pub poise_pressure: f32,
    pub stance_pressure: f32,
    pub status_buildup: [f32; STATUS_COUNT],
    /// Target-side modifiers begin after this hit resolves, so a debuff does
    /// not retroactively amplify the contact that applied it.
    pub target_modifiers: Vec<Modifier>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetRules {
    pub poise: PoiseDefinition,
    pub stance: StanceDefinition,
    pub statuses: [StatusDefinition; STATUS_COUNT],
    pub status_received: [f32; STATUS_COUNT],
    pub stance_received: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HitOutcome {
    /// Applied to the damage Skyrim would normally assign to this contact.
    pub skyrim_damage_multiplier: f32,
    pub elements: ElementalProfile,
    pub poise: PoiseResult,
    pub stance: StanceResult,
    pub status_procs: [Option<StatusProc>; STATUS_COUNT],
    pub modifiers_applied: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConsequenceState {
    pub statuses: StatusSet,
    pub modifiers: ModifierSet,
    pub poise: PoiseState,
    pub stance: StanceState,
}

impl ConsequenceState {
    pub fn resolve(&mut self, event: &HitEvent, rules: TargetRules) -> HitOutcome {
        let action = finite_nonnegative(event.damage_multiplier);
        let armament =
            finite_nonnegative(event.armament_multiplier).clamp(1.0, MAX_ARMAMENT_MULTIPLIER);
        let damage_taken =
            finite_nonnegative(self.modifiers.evaluate(ModifiedStat::DamageTaken, 1.0));
        let skyrim_damage_multiplier = finite_nonnegative(action * armament * damage_taken);

        let poise_multiplier = finite_positive(self.modifiers.evaluate(ModifiedStat::Poise, 1.0));
        let poise = self.poise.apply(
            event.poise_pressure,
            poise_multiplier,
            rules.poise,
            event.source,
        );
        let stance = self.stance.apply(
            event.stance_pressure,
            rules.stance_received,
            rules.stance,
            event.source,
        );

        let status_taken = finite_positive(
            self.modifiers
                .evaluate(ModifiedStat::StatusBuildupTaken, 1.0),
        );
        let mut status_procs = [None; STATUS_COUNT];
        for kind in StatusKind::ALL {
            let received = finite_nonnegative(rules.status_received[kind.index()] * status_taken);
            if let BuildupResult::Proc(proc) = self.statuses.apply(
                kind,
                event.status_buildup[kind.index()],
                received,
                rules.statuses[kind.index()],
                event.source,
            ) {
                status_procs[kind.index()] = Some(proc);
            }
        }

        let modifiers_applied = event
            .target_modifiers
            .iter()
            .filter(|&&modifier| self.modifiers.apply(modifier))
            .count();

        HitOutcome {
            skyrim_damage_multiplier,
            elements: event.elements,
            poise,
            stance,
            status_procs,
            modifiers_applied,
        }
    }

    pub fn tick(&mut self, delta_seconds: f32, rules: TargetRules) {
        self.statuses.tick(delta_seconds, &rules.statuses);
        self.modifiers.tick(delta_seconds);
        self.poise.tick(delta_seconds, rules.poise);
        self.stance.tick(delta_seconds, rules.stance);
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn finite_positive(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{ModifierOperation, StackRule};

    const STATUS: StatusDefinition = StatusDefinition {
        threshold: 100.0,
        decay_delay_seconds: 2.0,
        decay_per_second: 10.0,
        proc_lockout_seconds: 1.0,
    };
    const RULES: TargetRules = TargetRules {
        poise: PoiseDefinition {
            threshold: 50.0,
            recovery_delay_seconds: 1.0,
            recovery_per_second: 10.0,
        },
        stance: StanceDefinition {
            threshold: 100.0,
            decay_delay_seconds: 2.0,
            decay_per_second: 10.0,
            break_lockout_seconds: 1.0,
        },
        statuses: [STATUS; STATUS_COUNT],
        status_received: [1.0; STATUS_COUNT],
        stance_received: 1.0,
    };

    fn event() -> HitEvent {
        let elements = ElementalProfile::from_channels(&[
            (DamageElement::Physical, 0.8),
            (DamageElement::Fire, 0.2),
        ])
        .unwrap();
        let mut status_buildup = [0.0; STATUS_COUNT];
        status_buildup[StatusKind::Frostbite.index()] = 60.0;
        HitEvent {
            source: 7,
            target: 9,
            action_id: 11,
            hit_id: 1,
            damage_multiplier: 1.5,
            armament_multiplier: 1.1,
            elements,
            poise_pressure: 20.0,
            stance_pressure: 30.0,
            status_buildup,
            target_modifiers: vec![Modifier {
                source: 7,
                group: 41,
                stat: ModifiedStat::DamageTaken,
                operation: ModifierOperation::Multiply,
                magnitude: 1.1,
                remaining_seconds: 5.0,
                priority: 0,
                stack_rule: StackRule::Refresh,
            }],
        }
    }

    #[test]
    fn elemental_profiles_are_explicit_bounded_shares() {
        let profile = ElementalProfile::from_channels(&[
            (DamageElement::Physical, 0.6),
            (DamageElement::Magic, 0.2),
            (DamageElement::Magic, 0.2),
        ])
        .unwrap();
        assert!((profile.total() - 1.0).abs() < 1e-6);
        assert!((profile.share(DamageElement::Magic) - 0.4).abs() < 1e-6);
        assert!(ElementalProfile::from_channels(&[(DamageElement::Fire, 1.1)]).is_none());
        assert!(ElementalProfile::from_channels(&[(DamageElement::Fire, f32::NAN)]).is_none());
    }

    #[test]
    fn one_hit_keeps_all_consequence_channels_independent() {
        let mut state = ConsequenceState::default();
        let outcome = state.resolve(&event(), RULES);
        assert!((outcome.skyrim_damage_multiplier - 1.65).abs() < 1e-6);
        assert_eq!(outcome.elements.share(DamageElement::Fire), 0.2);
        assert!(matches!(
            outcome.poise,
            PoiseResult::Withstood {
                accumulated: 20.0,
                ..
            }
        ));
        assert!(matches!(
            outcome.stance,
            StanceResult::Accumulated { current: 30.0, .. }
        ));
        assert_eq!(state.statuses.meter(StatusKind::Frostbite).buildup(), 60.0);
        assert_eq!(state.statuses.meter(StatusKind::Bleed).buildup(), 0.0);
        assert_eq!(outcome.modifiers_applied, 1);
        assert!(outcome.status_procs.iter().all(Option::is_none));
    }

    #[test]
    fn a_debuff_starts_after_the_contact_that_applies_it() {
        let mut state = ConsequenceState::default();
        let first = state.resolve(&event(), RULES);
        let second = state.resolve(&event(), RULES);
        assert!((first.skyrim_damage_multiplier - 1.65).abs() < 1e-6);
        assert!((second.skyrim_damage_multiplier - 1.815).abs() < 1e-6);
        assert_eq!(
            state.modifiers.len(),
            1,
            "refresh must not duplicate the debuff"
        );
    }

    #[test]
    fn status_proc_and_stance_break_preserve_source_attribution() {
        let mut state = ConsequenceState::default();
        let mut finishing = event();
        finishing.status_buildup[StatusKind::Frostbite.index()] = 100.0;
        finishing.stance_pressure = 100.0;
        let outcome = state.resolve(&finishing, RULES);
        assert_eq!(
            outcome.status_procs[StatusKind::Frostbite.index()],
            Some(StatusProc {
                kind: StatusKind::Frostbite,
                source: 7
            })
        );
        assert_eq!(outcome.stance, StanceResult::Broken { source: 7 });
    }

    #[test]
    fn malformed_numeric_payloads_are_never_forwarded_to_skyrim() {
        let mut broken = event();
        broken.damage_multiplier = f32::NAN;
        broken.armament_multiplier = f32::INFINITY;
        broken.poise_pressure = f32::NAN;
        let mut state = ConsequenceState::default();
        let outcome = state.resolve(&broken, RULES);
        assert_eq!(outcome.skyrim_damage_multiplier, 0.0);
        assert_eq!(outcome.poise, PoiseResult::Ignored);
    }
}
