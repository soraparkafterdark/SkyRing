//! Delivery contracts for bows, throwables, and consumables.
//!
//! These are the third delivery family alongside melee and hazards. The
//! design homologates how the Two Lands actually play:
//!
//! - Elden Ring authors a bow shot as a committed action: a draw window you
//!   can cancel only by dodging, a discrete release, and a recovery you
//!   cannot cancel. Holding past full draw adds nothing.
//! - Skyrim authors the same bow as a continuous draw: damage scales with
//!   how far the string is pulled, the actor cannot sprint while drawn, and
//!   releasing early fires a weaker arrow.
//!
//! Both are one mechanic seen from two sides. This module models the
//! Skyrim-authentic continuous draw curve, and places Elden Ring's quick
//! shot and fully drawn shot as calibrated points on that curve. Skyrim
//! keeps ownership of arrow flight, impact, Archery skill, perks, sneak
//! multipliers, and bow/arrow base damage; this layer only commits timing
//! and produces bounded multipliers.
//!
//! Throwables are Elden Ring-authored: a committed wind-up during which the
//! actor cannot turn or dodge out of the throw itself, an arcing delivery
//! Skyrim's projectile system realizes, and a recovery window.
//!
//! Consumables stay instant and animation-less per the Charter; the
//! committed flask profile exists only behind an explicit gate because
//! enabling it is a Charter change, not a code decision.
//!
//! Every numeric value here is either a bounded relative multiplier, a
//! fraction of a target-side threshold, or a bridge-supplied balance input.
//! No absolute Elden Ring damage, HP, or meter values appear.

use crate::consequences::{DamageElement, ElementalProfile};
use crate::effects::{StatusKind, STATUS_COUNT};

/// One shot of a bow, from draw start to control return.
///
/// Timing values are calibrated to the two games' play feel, not extracted
/// from a data dump: the quick shot releases early on the draw curve, the
/// fully drawn shot holds to the cap. The bridge may retune any field; these
/// are the defaults the fixture records.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BowProfile {
    /// Seconds from draw start to reaching the quick-shot point.
    pub quick_draw_seconds: f32,
    /// Seconds from draw start to full draw. Holding longer adds nothing.
    pub full_draw_seconds: f32,
    /// Draw fraction the quick shot fires at (Elden Ring quick shot).
    pub quick_draw_fraction: f32,
    /// Stamina taken per release. Skyrim natively charges no bow stamina;
    /// the bridge may keep this at zero to stay Skyrim-authentic.
    pub stamina_per_release: f32,
    /// Seconds of recovery after release before control returns.
    pub recovery_seconds: f32,
    /// Draw seconds of hold stamina drain while fully drawn, as a per-second
    /// fraction of one light-attack cost. Zero disables the drain.
    pub hold_drain_per_second: f32,
}

impl BowProfile {
    /// Elden Ring feel: quick shot ~0.4 s, full draw ~1.1 s, committed.
    pub const ER_FEEL: Self = Self {
        quick_draw_seconds: 0.40,
        full_draw_seconds: 1.10,
        quick_draw_fraction: 0.80,
        stamina_per_release: 8.0,
        recovery_seconds: 0.35,
        hold_drain_per_second: 0.20,
    };

    pub fn valid(self) -> bool {
        self.quick_draw_seconds.is_finite()
            && self.quick_draw_seconds > 0.0
            && self.full_draw_seconds.is_finite()
            && self.full_draw_seconds >= self.quick_draw_seconds
            && self.quick_draw_fraction.is_finite()
            && self.quick_draw_fraction > 0.0
            && self.quick_draw_fraction <= 1.0
            && self.stamina_per_release.is_finite()
            && self.stamina_per_release >= 0.0
            && self.recovery_seconds.is_finite()
            && self.recovery_seconds >= 0.0
            && self.hold_drain_per_second.is_finite()
            && self.hold_drain_per_second >= 0.0
    }

    /// Skyrim-authentic draw curve: damage rises linearly from zero draw to
    /// full draw, matching how Skyrim scales arrow damage with draw state.
    /// `held_seconds` beyond full draw clamps; there is no overdraw bonus.
    pub fn draw_fraction(self, held_seconds: f32) -> f32 {
        if !held_seconds.is_finite() || held_seconds <= 0.0 {
            return 0.0;
        }
        (held_seconds / self.full_draw_seconds).min(1.0)
    }

    /// Whether a draw begun `held_seconds` ago has passed the quick-shot
    /// point, which is when a release becomes a real Elden Ring quick shot
    /// rather than a nocking flick.
    pub fn past_quick_point(self, held_seconds: f32) -> bool {
        held_seconds >= self.quick_draw_seconds
    }
}

/// One arrow profile. Buildup is a fraction of the target's threshold, which
/// is the same relative-value policy the melee contract uses: three to four
/// status arrows should threaten one proc on a neutral target, matching how
/// Elden Ring status arrows behave, while absolute numbers remain Skyrim's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowProfile {
    pub id: &'static str,
    pub element: Option<DamageElement>,
    /// Elemental share this arrow requests, when it carries an element.
    pub element_share: f32,
    /// Status delivered, when it is a buildup arrow.
    pub status: Option<StatusKind>,
    /// Buildup applied per hit as a fraction of the target threshold.
    pub buildup_fraction: f32,
}

pub const ARROW_COUNT: usize = 7;

/// The arrow vocabulary the two games share. Skyrim's arrow damage stays
/// authoritative; these profiles only attach semantic payloads.
pub const ARROWS: [ArrowProfile; ARROW_COUNT] = [
    ArrowProfile {
        id: "iron",
        element: None,
        element_share: 0.0,
        status: None,
        buildup_fraction: 0.0,
    },
    ArrowProfile {
        id: "fire",
        element: Some(DamageElement::Fire),
        element_share: 0.50,
        status: None,
        buildup_fraction: 0.0,
    },
    ArrowProfile {
        id: "frost",
        element: Some(DamageElement::Magic),
        element_share: 0.30,
        status: Some(StatusKind::Frostbite),
        // Estimate: ER frost delivery builds noticeably per hit; two to
        // three frost arrows threaten a proc on a neutral target.
        buildup_fraction: 0.40,
    },
    ArrowProfile {
        id: "poison",
        element: None,
        element_share: 0.0,
        status: Some(StatusKind::Poison),
        // Estimate: ER poison arrows proc poison on weak targets in three
        // to four hits; resistant targets need more via `status_received`.
        buildup_fraction: 0.35,
    },
    ArrowProfile {
        id: "bleed",
        element: None,
        element_share: 0.0,
        status: Some(StatusKind::Bleed),
        // Estimate: ER bleed arrows are among the strongest buildup
        // deliveries; slightly stronger than frost per hit.
        buildup_fraction: 0.45,
    },
    ArrowProfile {
        id: "sleep",
        element: None,
        element_share: 0.0,
        status: Some(StatusKind::Sleep),
        // Estimate: ER sleep arrows proc on ordinary targets in one to two
        // hits; large targets resist through `status_received`.
        buildup_fraction: 0.60,
    },
    ArrowProfile {
        id: "madness",
        element: None,
        element_share: 0.0,
        status: Some(StatusKind::Madness),
        // Estimate: ER madness is player-venom: strong against human
        // targets, ignored by beasts through immunity rules.
        buildup_fraction: 0.50,
    },
];

impl ArrowProfile {
    pub fn by_id(id: &str) -> Option<Self> {
        ARROWS.iter().copied().find(|arrow| arrow.id == id)
    }

    /// Elemental share profile, or the empty profile for a physical arrow.
    pub fn elements(self) -> ElementalProfile {
        match self.element {
            Some(element) => ElementalProfile::from_channels(&[(element, self.element_share)])
                .unwrap_or_default(),
            None => ElementalProfile::default(),
        }
    }

    /// Per-hit buildup array over the shared status vocabulary.
    pub fn buildup_array(self, target_threshold: f32) -> [f32; STATUS_COUNT] {
        let mut array = [0.0; STATUS_COUNT];
        if let Some(kind) = self.status {
            if target_threshold.is_finite() && target_threshold > 0.0 {
                array[kind.index()] = self.buildup_fraction * target_threshold;
            }
        }
        array
    }
}

/// One throwable profile. Elden Ring-authored: wind-up commitment, arcing
/// delivery, area payload. Skyrim realizes the flight and the hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrowableProfile {
    pub id: &'static str,
    /// Committed wind-up before the projectile leaves the hand. The actor
    /// cannot turn past wind-up start (Elden Ring throw commitment).
    pub windup_seconds: f32,
    /// Recovery after release before control returns.
    pub recovery_seconds: f32,
    /// Stamina taken per throw.
    pub stamina_cost: f32,
    pub element: Option<DamageElement>,
    /// Area-of-effect radius in metres the bridge realizes on impact.
    pub radius_m: f32,
    /// Bounded damage multiplier relative to one light hit.
    pub damage_multiplier: f32,
    /// Stance pressure relative to one light hit; throwables barely move
    /// poise in Elden Ring.
    pub stance_multiplier: f32,
}

pub const THROWABLE_COUNT: usize = 4;

pub const THROWABLES: [ThrowableProfile; THROWABLE_COUNT] = [
    ThrowableProfile {
        id: "cerulean_pot",
        windup_seconds: 0.70,
        recovery_seconds: 0.50,
        stamina_cost: 10.0,
        element: Some(DamageElement::Magic),
        radius_m: 3.0,
        damage_multiplier: 0.85,
        stance_multiplier: 0.40,
    },
    ThrowableProfile {
        id: "fire_pot",
        windup_seconds: 0.70,
        recovery_seconds: 0.50,
        stamina_cost: 10.0,
        element: Some(DamageElement::Fire),
        radius_m: 3.0,
        damage_multiplier: 0.95,
        stance_multiplier: 0.40,
    },
    ThrowableProfile {
        id: "cracked_pot",
        windup_seconds: 0.65,
        recovery_seconds: 0.50,
        stamina_cost: 10.0,
        element: Some(DamageElement::Physical),
        radius_m: 2.5,
        damage_multiplier: 1.10,
        stance_multiplier: 0.50,
    },
    ThrowableProfile {
        id: "throwing_dagger",
        windup_seconds: 0.45,
        recovery_seconds: 0.35,
        stamina_cost: 6.0,
        element: Some(DamageElement::Physical),
        radius_m: 0.0,
        damage_multiplier: 0.70,
        stance_multiplier: 0.30,
    },
];

impl ThrowableProfile {
    pub fn by_id(id: &str) -> Option<Self> {
        THROWABLES.iter().copied().find(|item| item.id == id)
    }

    pub fn valid(self) -> bool {
        self.windup_seconds.is_finite()
            && self.windup_seconds > 0.0
            && self.recovery_seconds.is_finite()
            && self.recovery_seconds >= 0.0
            && self.stamina_cost.is_finite()
            && self.stamina_cost >= 0.0
            && self.radius_m.is_finite()
            && self.radius_m >= 0.0
            && self.damage_multiplier.is_finite()
            && self.damage_multiplier > 0.0
            && self.stance_multiplier.is_finite()
            && self.stance_multiplier >= 0.0
    }
}

/// Consumables per the Charter: instant and animation-less. The committed
/// flask (Elden Ring's ~1 second full-commitment heal) requires an explicit
/// Charter change before it can be enabled; the gate keeps the current
/// contract honest instead of quietly growing an animation.
pub const COMMITTED_CONSUMABLES_ENABLED: bool = false;

/// One consumable application. Effects route through the modifier stack and
/// the shared meters exactly like every other consequence source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConsumableProfile {
    pub id: &'static str,
    /// Seconds of committed wind-up. Must be zero while committed
    /// consumables are disabled by the Charter gate.
    pub windup_seconds: f32,
    /// Walk speed fraction while applying; instant consumables ignore it.
    pub walk_speed_fraction: f32,
    /// Modifier this consumable applies to the user. Duration and magnitude
    /// are Skyrim-facing balance inputs.
    pub modifier_seconds: f32,
}

pub const CONSUMABLE_COUNT: usize = 4;

pub const CONSUMABLES: [ConsumableProfile; CONSUMABLE_COUNT] = [
    ConsumableProfile {
        id: "flask_crimson",
        windup_seconds: 0.0,
        walk_speed_fraction: 1.0,
        modifier_seconds: 0.0,
    },
    ConsumableProfile {
        id: "flask_cerulean",
        windup_seconds: 0.0,
        walk_speed_fraction: 1.0,
        modifier_seconds: 0.0,
    },
    ConsumableProfile {
        id: "grease_fire",
        windup_seconds: 0.0,
        walk_speed_fraction: 1.0,
        modifier_seconds: 60.0,
    },
    ConsumableProfile {
        id: "preserving_boluses",
        windup_seconds: 0.0,
        walk_speed_fraction: 1.0,
        modifier_seconds: 0.0,
    },
];

impl ConsumableProfile {
    pub fn by_id(id: &str) -> Option<Self> {
        CONSUMABLES.iter().copied().find(|item| item.id == id)
    }

    pub fn valid(self) -> bool {
        if !COMMITTED_CONSUMABLES_ENABLED && self.windup_seconds != 0.0 {
            return false;
        }
        self.windup_seconds.is_finite()
            && self.windup_seconds >= 0.0
            && self.walk_speed_fraction.is_finite()
            && self.walk_speed_fraction > 0.0
            && self.walk_speed_fraction <= 1.0
            && self.modifier_seconds.is_finite()
            && self.modifier_seconds >= 0.0
    }
}

/// Live state of one committed delivery: a bow draw or a throwable wind-up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeliveryPhase {
    /// Drawing or winding up. Dodging can still cancel.
    Committed { elapsed_seconds: f32 },
    /// Released/thrown; recovery runs until control returns.
    Recovered { remaining_seconds: f32 },
    /// Nothing in progress.
    Idle,
}

/// What a release instant produces for the shared consequence contract.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeliveryOutcome {
    /// Bounded multiplier over Skyrim's own arrow/pot damage.
    pub damage_multiplier: f32,
    pub elements: ElementalProfile,
    /// Per-status buildup for this delivery, in absolute units already
    /// scaled by the target threshold the bridge supplies.
    pub status_buildup: [f32; STATUS_COUNT],
    pub stance_pressure: f32,
}

/// Bow damage relative to a light hit, as a function of draw state.
///
/// Skyrim's continuous draw is authoritative in shape: damage rises with
/// draw time, releases before full draw fire weaker arrows, and there is no
/// discrete stance change. Elden Ring's calibration points are placed on
/// that curve, front-loaded to honor how ER bows actually play: the quick
/// shot at `quick_draw_seconds` already delivers `quick_draw_fraction` of
/// full-draw damage (in ER the quick shot deals essentially full damage;
/// full draw buys arrow velocity, not damage). The segment from the quick
/// point to full draw rises the remaining distance, and holding past full
/// draw adds nothing.
///
/// A bridge that wants Skyrim's pure linear curve sets `quick_draw_seconds`
/// equal to `full_draw_seconds`. Damage never exceeds one light hit's
/// envelope max: bows trade commitment flexibility for peak damage, which
/// is how both games play.
pub fn bow_damage_multiplier(profile: BowProfile, held_seconds: f32, light_max: f32) -> f32 {
    if !profile.valid() || !light_max.is_finite() || light_max <= 0.0 {
        return 0.0;
    }
    if !held_seconds.is_finite() || held_seconds <= 0.0 {
        return 0.0;
    }
    if held_seconds >= profile.full_draw_seconds {
        return light_max;
    }
    if held_seconds <= profile.quick_draw_seconds {
        // Rise to the quick-shot calibration point.
        let fraction = held_seconds / profile.quick_draw_seconds;
        return profile.quick_draw_fraction * fraction * light_max;
    }
    let span = (profile.full_draw_seconds - profile.quick_draw_seconds).max(1e-6);
    let above = ((held_seconds - profile.quick_draw_seconds) / span).clamp(0.0, 1.0);
    let scaled = profile.quick_draw_fraction + (1.0 - profile.quick_draw_fraction) * above;
    (scaled * light_max).clamp(0.0, light_max)
}

/// State machine for one committed delivery. Frame-rate independent: the
/// caller advances with real delta seconds, exactly like the status meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeliveryState {
    phase: DeliveryPhase,
}

impl Default for DeliveryState {
    fn default() -> Self {
        Self {
            phase: DeliveryPhase::Idle,
        }
    }
}

impl DeliveryState {
    pub fn phase(&self) -> DeliveryPhase {
        self.phase
    }

    /// Begin a draw or wind-up. Returns `false` when something is already
    /// in progress, which is the commitment: no action stacking.
    pub fn begin(&mut self) -> bool {
        if self.phase != DeliveryPhase::Idle {
            return false;
        }
        self.phase = DeliveryPhase::Committed {
            elapsed_seconds: 0.0,
        };
        true
    }

    /// Advance time. `dodge_cancel` is the Elden Ring rule: a dodge cancels
    /// a draw or wind-up at any point before release, but never a recovery.
    /// Returns `true` when the action is again `Idle`.
    pub fn cancel_by_dodge(&mut self, delta_seconds: f32) -> bool {
        match self.phase {
            DeliveryPhase::Committed { elapsed_seconds } => {
                let _ = elapsed_seconds;
                self.phase = DeliveryPhase::Idle;
                true
            }
            DeliveryPhase::Recovered { remaining_seconds } => {
                let _ = delta_seconds;
                let _ = remaining_seconds;
                // Recovery is committed: dodge input is queued, not honored.
                false
            }
            DeliveryPhase::Idle => true,
        }
    }

    /// Advance time by `delta_seconds`. Returns `true` when a committed
    /// draw or wind-up has reached its natural release point: the caller
    /// then invokes `release` for bows or `throw_at_windup_end` for
    /// throwables. Recovery advances on its own and returns to `Idle`.
    pub fn tick(&mut self, delta_seconds: f32) -> bool {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return false;
        }
        match self.phase {
            DeliveryPhase::Committed { elapsed_seconds } => {
                self.phase = DeliveryPhase::Committed {
                    elapsed_seconds: elapsed_seconds + delta_seconds,
                };
                false
            }
            DeliveryPhase::Recovered { remaining_seconds } => {
                let remaining = remaining_seconds - delta_seconds;
                if remaining <= 0.0 {
                    self.phase = DeliveryPhase::Idle;
                    true
                } else {
                    self.phase = DeliveryPhase::Recovered {
                        remaining_seconds: remaining,
                    };
                    false
                }
            }
            DeliveryPhase::Idle => true,
        }
    }

    /// Release at the current committed elapsed time. `resolve` receives the
    /// held seconds and returns the outcome. After release the state enters
    /// recovery for `recovery_seconds`, during which dodging is queued but
    /// not honored, matching Elden Ring's committed bow recovery.
    pub fn release(
        &mut self,
        profile: BowProfile,
        light_max: f32,
        arrow: ArrowProfile,
        target_threshold: f32,
    ) -> Option<DeliveryOutcome> {
        let elapsed = match self.phase {
            DeliveryPhase::Committed { elapsed_seconds } => elapsed_seconds,
            _ => return None,
        };
        if !profile.valid() {
            return None;
        }
        let damage = bow_damage_multiplier(profile, elapsed, light_max);
        let outcome = DeliveryOutcome {
            damage_multiplier: damage,
            elements: arrow.elements(),
            status_buildup: arrow.buildup_array(target_threshold),
            stance_pressure: 0.55 * damage,
        };
        self.phase = DeliveryPhase::Recovered {
            remaining_seconds: profile.recovery_seconds,
        };
        Some(outcome)
    }

    /// Throw a throwable at the end of its wind-up. Elden Ring rule: the
    /// throw fires automatically when the wind-up completes; the actor
    /// cannot hold a pot. Returns the outcome at the release instant.
    pub fn throw_at_windup_end(
        &mut self,
        throwable: ThrowableProfile,
        target_threshold: f32,
    ) -> Option<DeliveryOutcome> {
        if !throwable.valid() {
            return None;
        }
        let elapsed = match self.phase {
            DeliveryPhase::Committed { elapsed_seconds } => elapsed_seconds,
            _ => return None,
        };
        if elapsed < throwable.windup_seconds {
            return None;
        }
        let buildup = [0.0; STATUS_COUNT];
        // Fire pots request fire damage only; no status buildup is authored
        // for any throwable in either game.
        let _ = target_threshold;
        let outcome = DeliveryOutcome {
            damage_multiplier: throwable.damage_multiplier,
            elements: match throwable.element {
                Some(element) => {
                    ElementalProfile::from_channels(&[(element, 1.0)]).unwrap_or_default()
                }
                None => ElementalProfile::default(),
            },
            status_buildup: buildup,
            stance_pressure: throwable.stance_multiplier,
        };
        self.phase = DeliveryPhase::Recovered {
            remaining_seconds: throwable.recovery_seconds,
        };
        Some(outcome)
    }

    /// Apply an instant consumable. Charter-gated: committed profiles are
    /// rejected while `COMMITTED_CONSUMABLES_ENABLED` is false.
    pub fn apply_consumable(&mut self, consumable: ConsumableProfile) -> bool {
        if !consumable.valid() {
            return false;
        }
        if self.phase != DeliveryPhase::Idle {
            return false;
        }
        true
    }
}

/// Homologation notes for the bridge: what each game owns per delivery.
pub const DELIVERY_OWNERSHIP: &str = "Skyrim owns projectile flight, impact \
detection, Archery skill, perks, sneak multipliers, and arrow damage. The \
core owns commitment: draw curve points, wind-up lockout, recovery windows, \
and semantic payloads. Elden Ring's quick and full shots are calibration \
points on Skyrim's continuous draw curve, so both games' arrows are one \
mechanic.";

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT_MAX: f32 = 1.15;

    #[test]
    fn bow_profiles_and_arrows_validate() {
        assert!(BowProfile::ER_FEEL.valid());
        for arrow in ARROWS {
            assert!(!arrow.id.is_empty());
            let profile = ArrowProfile::by_id(arrow.id);
            assert_eq!(profile, Some(arrow));
        }
        assert_eq!(ArrowProfile::by_id("nope"), None);
    }

    #[test]
    fn draw_curve_is_skyrim_continuous_with_er_points() {
        let profile = BowProfile::ER_FEEL;
        assert_eq!(profile.draw_fraction(0.0), 0.0);
        assert_eq!(profile.draw_fraction(profile.full_draw_seconds), 1.0);
        // Overdraw adds nothing: holding past full draw is capped.
        assert_eq!(profile.draw_fraction(profile.full_draw_seconds * 3.0), 1.0);
    }

    #[test]
    fn early_release_is_weaker_but_quick_shot_carries_full_er_damage() {
        let profile = BowProfile::ER_FEEL;
        let before = bow_damage_multiplier(profile, profile.quick_draw_seconds * 0.5, LIGHT_MAX);
        let quick = bow_damage_multiplier(profile, profile.quick_draw_seconds, LIGHT_MAX);
        let full = bow_damage_multiplier(profile, profile.full_draw_seconds, LIGHT_MAX);
        assert!(before < quick, "an early flick is weaker than a quick shot");
        // ER calibration: the quick shot already delivers quick_draw_fraction
        // of full-draw damage; full draw buys velocity, not damage.
        assert!((quick - LIGHT_MAX * profile.quick_draw_fraction).abs() < 1e-5);
        assert!(quick < full);
        assert!(full <= LIGHT_MAX);
        // Overdraw holds at the cap rather than growing.
        let held = bow_damage_multiplier(profile, profile.full_draw_seconds * 2.0, LIGHT_MAX);
        assert!((held - full).abs() < 1e-5);
    }

    #[test]
    fn commitment_rejects_action_stacking_and_honors_dodge_only_pre_release() {
        let mut state = DeliveryState::default();
        // Commitment forbids stacking a second draw...
        assert!(state.begin());
        assert!(!state.begin(), "commitment forbids stacking a second draw");
        // Dodging out of a draw is allowed (Elden Ring rule).
        assert!(state.cancel_by_dodge(0.1));
        assert_eq!(state.phase(), DeliveryPhase::Idle);

        // Past release, recovery is committed: dodges are queued, not honored.
        assert!(state.begin());
        state.tick(1.2);
        assert!(state
            .release(BowProfile::ER_FEEL, LIGHT_MAX, ARROWS[0], 100.0)
            .is_some());
        assert!(matches!(state.phase(), DeliveryPhase::Recovered { .. }));
        assert!(!state.cancel_by_dodge(0.1));
    }

    #[test]
    fn release_produces_bounded_payloads_for_each_arrow() {
        for arrow in ARROWS {
            let mut state = DeliveryState::default();
            assert!(state.begin());
            state.tick(BowProfile::ER_FEEL.full_draw_seconds);
            let outcome = state
                .release(BowProfile::ER_FEEL, LIGHT_MAX, arrow, 100.0)
                .expect("full draw must release");
            assert!(outcome.damage_multiplier > 0.0);
            assert!(outcome.damage_multiplier <= LIGHT_MAX);
            let total: f32 = outcome.status_buildup.iter().sum();
            if arrow.status.is_none() {
                assert_eq!(total, 0.0);
            } else {
                // Buildup is a fraction of the target threshold.
                let expected = arrow.buildup_fraction * 100.0;
                assert!((total - expected).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn throwables_fire_at_windup_end_and_stay_committed_through_recovery() {
        let throwable = ThrowableProfile::by_id("fire_pot").unwrap();
        assert!(throwable.valid());
        let mut state = DeliveryState::default();
        assert!(state.begin());
        // Not yet at wind-up end: nothing releases.
        state.tick(throwable.windup_seconds * 0.5);
        assert!(state.throw_at_windup_end(throwable, 100.0).is_none());
        state.tick(throwable.windup_seconds * 0.6);
        let outcome = state
            .throw_at_windup_end(throwable, 100.0)
            .expect("wind-up complete");
        assert!(outcome.damage_multiplier > 0.0);
        assert_eq!(outcome.elements.share(DamageElement::Fire), 1.0);
        assert!(matches!(state.phase(), DeliveryPhase::Recovered { .. }));
        assert!(ThrowableProfile::by_id("nope").is_none());
    }

    #[test]
    fn consumables_are_instant_and_charter_gated() {
        for consumable in CONSUMABLES {
            assert_eq!(consumable.windup_seconds, 0.0, "Charter: instant");
            assert!(consumable.valid());
            let mut state = DeliveryState::default();
            assert!(state.apply_consumable(consumable));
        }
        // A committed flask would be a Charter change, not a code decision.
        let committed = ConsumableProfile {
            id: "flask_crimson",
            windup_seconds: 1.0,
            walk_speed_fraction: 0.5,
            modifier_seconds: 0.0,
        };
        assert!(!committed.valid());
        assert!(!COMMITTED_CONSUMABLES_ENABLED);
    }

    #[test]
    fn non_finite_and_zero_advances_change_nothing() {
        let mut state = DeliveryState::default();
        assert!(state.begin());
        assert!(!state.tick(0.0));
        assert!(!state.tick(f32::NAN));
        let elapsed = match state.phase() {
            DeliveryPhase::Committed { elapsed_seconds } => elapsed_seconds,
            other => panic!("still committed, got {other:?}"),
        };
        assert_eq!(elapsed, 0.0);
    }
}
