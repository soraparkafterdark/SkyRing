//! Homologation of environmental hazards across the Two Lands.
//!
//! Elden Ring exposes hazards as authored volumes: poison and scarlet-rot
//! swamps, poison and exhaust gas clouds, breath attacks, and lava. Skyrim
//! exposes them as traps and effects: gas vents, poison rune clouds, spore
//! clouds, and dragon breath. Both traditions ultimately express the same
//! small set of physical facts, so this module models the canonical facts and
//! leaves game-specific naming to the bridge adapters.
//!
//! A hazard here is a spatial exposure that re-applies contact consequences
//! over time. It does not subtract health itself; it produces the same
//! delivery-neutral `Consequences` payload that melee, spells, throwables,
//! and creature attacks produce, so Skyrim's authority over damage,
//! resistances, and visuals is never bypassed.
//!
//! Nothing in this module reads absolute source-game values. Thresholds,
//! per-second buildup, and tick rates are Skyrim-facing balance inputs.

use crate::consequences::DamageElement;
use crate::effects::{StatusDefinition, StatusKind, StatusSet, STATUS_COUNT};

/// Canonical environmental medium of a hazard volume or cloud.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HazardMedium {
    /// Poison gas or vapour (ER poison swamps/clouds, Skyrim gas vents).
    PoisonGas = 0,
    /// Scarlet rot or creeping decay (ER rot pools; no Skyrim-native twin).
    RotFume = 1,
    /// Frost or cold vapor (Skyrim frost traps, ER frost-breath).
    FrostVapor = 2,
    /// Fire, flame jets, and burning ground.
    Flame = 3,
    /// Shock arcanum (Skyrim shock runes, ER lightning fields).
    ShockField = 4,
    /// Molten rock or lava surfaces (ER; Skyrim lava via mods only).
    Lava = 5,
    /// Exhaust or asphyxiant gas that drains stamina rather than health.
    ExhaustFume = 6,
    /// Spore or fungal bursts (Skyrim spore traps; ER mushroom groves).
    SporeBurst = 7,
    /// Water that conducts shock or slows movement without direct damage.
    DeepWater = 8,
    /// Deepwater quicksands, mud, and tar that impede commitment actions.
    Mire = 9,
}

pub const HAZARD_MEDIUM_COUNT: usize = 10;

impl HazardMedium {
    pub const ALL: [Self; HAZARD_MEDIUM_COUNT] = [
        Self::PoisonGas,
        Self::RotFume,
        Self::FrostVapor,
        Self::Flame,
        Self::ShockField,
        Self::Lava,
        Self::ExhaustFume,
        Self::SporeBurst,
        Self::DeepWater,
        Self::Mire,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    /// The single semantic status this medium builds toward, if any. A
    /// hazard never applies more than one status, which keeps the "grossness
    /// you breathe or step in" vocabulary auditable.
    pub const fn status(self) -> Option<StatusKind> {
        match self {
            Self::PoisonGas | Self::SporeBurst => Some(StatusKind::Poison),
            Self::RotFume => Some(StatusKind::Rot),
            Self::FrostVapor => Some(StatusKind::Frostbite),
            Self::ExhaustFume | Self::Mire => Some(StatusKind::Sleep),
            Self::Flame | Self::ShockField | Self::Lava | Self::DeepWater => None,
        }
    }

    /// The semantic elemental channel this medium delivers, if any.
    pub const fn element(self) -> Option<DamageElement> {
        match self {
            Self::Flame => Some(DamageElement::Fire),
            Self::ShockField => Some(DamageElement::Lightning),
            Self::Lava => Some(DamageElement::Fire),
            // FrostVapor deliberately carries only Frostbite buildup: the
            // frost proc is the semantic payload, and Skyrim's native frost
            // resistance path stays reserved for discrete attacks.
            Self::FrostVapor => None,
            Self::PoisonGas
            | Self::RotFume
            | Self::ExhaustFume
            | Self::SporeBurst
            | Self::DeepWater
            | Self::Mire => None,
        }
    }
}

/// How an actor interacts with a hazard volume at one evaluation instant.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExposureState {
    /// Fully inside the volume.
    Immersed,
    /// Partially inside (shallow wading, edge overlap).
    Wading,
    /// Brief contact at the boundary; weaker than wading.
    Brushed,
    /// Outside the volume.
    Clear,
}

/// Per-medium tuning for one continuous exposure second. All values are
/// Skyrim-facing balance inputs supplied by the bridge, never constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazardDefinition {
    pub medium: HazardMedium,
    /// Buildup applied per second while immersed.
    pub buildup_per_second: f32,
    /// Semantic damage share requested from Skyrim per second while
    /// immersed. The bridge maps it onto native resistances.
    pub damage_per_second: f32,
    /// Multiplier applied when exposure is `Wading`.
    pub wading_scale: f32,
    /// Multiplier applied when exposure is `Brushed`.
    pub brushed_scale: f32,
    /// Status tuning used when this medium builds a status meter.
    pub status: StatusDefinition,
}

impl HazardDefinition {
    pub fn valid(self) -> bool {
        self.buildup_per_second.is_finite()
            && self.buildup_per_second >= 0.0
            && self.damage_per_second.is_finite()
            && self.damage_per_second >= 0.0
            && self.wading_scale.is_finite()
            && self.wading_scale >= 0.0
            && self.wading_scale <= 1.0
            && self.brushed_scale.is_finite()
            && self.brushed_scale >= 0.0
            && self.brushed_scale <= self.wading_scale
            && self.status.valid()
    }

    fn exposure_scale(self, state: ExposureState) -> f32 {
        match state {
            ExposureState::Immersed => 1.0,
            ExposureState::Wading => self.wading_scale,
            ExposureState::Brushed => self.brushed_scale,
            ExposureState::Clear => 0.0,
        }
    }
}

/// What one hazard evaluation instant asked of the shared consequence
/// contract. The bridge decides how to realize it; this layer never touches
/// health or actor values directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazardExposure {
    pub medium: HazardMedium,
    pub status: Option<StatusKind>,
    pub element: Option<DamageElement>,
    /// Scaled buildup for this instant, already multiplied by the exposure
    /// scale and any actor-specific resistance multiplier.
    pub buildup: f32,
    /// Scaled damage share for this instant.
    pub damage: f32,
}

/// Per-actor hazard tracking: one status set plus per-medium tick clocks so
/// continuous exposure accrues smoothly without frame-rate dependence.
#[derive(Clone, Debug)]
pub struct HazardVulnerability {
    statuses: StatusSet,
    seconds_exposed: [f32; HAZARD_MEDIUM_COUNT],
    seconds_immunity_remaining: [f32; HAZARD_MEDIUM_COUNT],
}

impl Default for HazardVulnerability {
    fn default() -> Self {
        Self {
            statuses: StatusSet::default(),
            seconds_exposed: [0.0; HAZARD_MEDIUM_COUNT],
            seconds_immunity_remaining: [0.0; HAZARD_MEDIUM_COUNT],
        }
    }
}

impl HazardVulnerability {
    /// Evaluate one instant of exposure to `medium`. Returns `None` when the
    /// actor is clear, immune, or the definition is invalid. `resistance`
    /// follows the status contract: 1 neutral, 0 immune, above 1 vulnerable.
    pub fn expose(
        &mut self,
        medium: HazardMedium,
        state: ExposureState,
        definition: HazardDefinition,
        resistance: f32,
    ) -> Option<HazardExposure> {
        if definition.medium != medium || !definition.valid() {
            return None;
        }
        if state == ExposureState::Clear {
            return None;
        }
        if !resistance.is_finite() || resistance <= 0.0 {
            return None;
        }
        if self.seconds_immunity_remaining[medium.index()] > 0.0 {
            return None;
        }

        let scale = definition.exposure_scale(state);
        let buildup = definition.buildup_per_second * scale * resistance;
        let damage = definition.damage_per_second * scale * resistance;
        let status = medium.status();
        let element = medium.element();

        if let Some(kind) = status {
            // Continuous exposure re-applies small per-second doses through
            // the same meters that discrete attacks use, so decay, lockout,
            // and source attribution stay uniform across deliveries.
            let _ = self
                .statuses
                .apply(kind, buildup, 1.0, definition.status, medium as u64);
        }

        self.seconds_exposed[medium.index()] += scale;
        Some(HazardExposure {
            medium,
            status,
            element,
            buildup,
            damage,
        })
    }

    pub fn statuses(&self) -> &StatusSet {
        &self.statuses
    }

    /// Total scaled exposure seconds accumulated per medium.
    pub fn exposure_seconds(&self, medium: HazardMedium) -> f32 {
        self.seconds_exposed[medium.index()]
    }

    /// Grant temporary immunity to one medium (wading boots, resist potions,
    /// breath talismans). `seconds` must be finite and non-negative.
    pub fn grant_immunity(&mut self, medium: HazardMedium, seconds: f32) {
        if seconds.is_finite() && seconds >= 0.0 {
            self.seconds_immunity_remaining[medium.index()] = seconds;
        }
    }

    pub fn immunity_remaining(&self, medium: HazardMedium) -> f32 {
        self.seconds_immunity_remaining[medium.index()]
    }

    /// Advance immunity timers and the underlying status meters.
    pub fn tick(&mut self, delta_seconds: f32, definitions: &[StatusDefinition; STATUS_COUNT]) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        for remaining in &mut self.seconds_immunity_remaining {
            *remaining = (*remaining - delta_seconds).max(0.0);
        }
        self.statuses.tick(delta_seconds, definitions);
    }

    /// Clear all exposure state (respawn, save load, hazard dispel).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Homologation table: for each canonical medium, the Two Lands' native
/// manifestations. The bridge uses this to decide which engine objects map
/// onto which medium; this module only records the crosswalk.
#[derive(Clone, Copy, Debug)]
pub struct MediumCrosswalk {
    pub medium: HazardMedium,
    pub elden_ring: &'static str,
    pub skyrim: &'static str,
}

pub const CROSSWALK: [MediumCrosswalk; HAZARD_MEDIUM_COUNT] = [
    MediumCrosswalk {
        medium: HazardMedium::PoisonGas,
        elden_ring: "poison swamps, poison mist clouds, poison-breath foes",
        skyrim: "gas vents, poison rune traps, chaurus spitters",
    },
    MediumCrosswalk {
        medium: HazardMedium::RotFume,
        elden_ring: "scarlet rot pools and rot-breath",
        skyrim: "no native twin; reserve for future content",
    },
    MediumCrosswalk {
        medium: HazardMedium::FrostVapor,
        elden_ring: "frost-breath and freezing mist",
        skyrim: "frost traps and frost-breath dragons",
    },
    MediumCrosswalk {
        medium: HazardMedium::Flame,
        elden_ring: "flame jets, burning ground, fire-breath",
        skyrim: "flame jets, fire rune traps, fire-breath",
    },
    MediumCrosswalk {
        medium: HazardMedium::ShockField,
        elden_ring: "lightning fields and stormcaller zones",
        skyrim: "shock runes and lightning-vent traps",
    },
    MediumCrosswalk {
        medium: HazardMedium::Lava,
        elden_ring: "lava pools",
        skyrim: "absent natively; modded content only",
    },
    MediumCrosswalk {
        medium: HazardMedium::ExhaustFume,
        elden_ring: "sleep-inducing gas clouds",
        skyrim: "vaporizer traps and sleep-gas effects",
    },
    MediumCrosswalk {
        medium: HazardMedium::SporeBurst,
        elden_ring: "spore-burst flora",
        skyrim: "spore traps and glowing-mushroom bursts",
    },
    MediumCrosswalk {
        medium: HazardMedium::DeepWater,
        elden_ring: "water that conducts lightning",
        skyrim: "shallow conductive water surfaces",
    },
    MediumCrosswalk {
        medium: HazardMedium::Mire,
        elden_ring: "mud and tar pits that slow commitment",
        skyrim: "sloped mud and tar zones",
    },
];

/// How a hazard should feel, calibrated to how the two games play. Every
/// number is an estimate recorded as such (AGENTS.md rule), expressed in
/// relative units the bridge scales. These are starting envelopes for
/// telemetry to move, not extracted constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazardPreset {
    pub medium: HazardMedium,
    /// Estimated seconds of immersed exposure to threaten one status proc
    /// on a neutral target. ER's poison swamps build poison in roughly
    /// five to eight seconds of wading; scarlet rot threatens roughly
    /// twice as fast, which is why rot swamps are feared and poison
    /// swamps are merely annoying.
    pub seconds_to_threaten: f32,
    /// Estimated damage as a fraction of one light hit per immersed
    /// second. ER swamps tick small damage; lava is the exception and
    /// hurts immediately.
    pub light_hit_fraction_per_second: f32,
}

pub const PRESET_COUNT: usize = 10;

pub const PRESETS: [HazardPreset; PRESET_COUNT] = [
    HazardPreset {
        medium: HazardMedium::PoisonGas,
        seconds_to_threaten: 7.0,
        light_hit_fraction_per_second: 0.05,
    },
    HazardPreset {
        medium: HazardMedium::RotFume,
        seconds_to_threaten: 3.5,
        light_hit_fraction_per_second: 0.08,
    },
    HazardPreset {
        medium: HazardMedium::FrostVapor,
        seconds_to_threaten: 6.0,
        light_hit_fraction_per_second: 0.06,
    },
    HazardPreset {
        medium: HazardMedium::Flame,
        seconds_to_threaten: 0.0,
        light_hit_fraction_per_second: 0.35,
    },
    HazardPreset {
        medium: HazardMedium::ShockField,
        seconds_to_threaten: 0.0,
        light_hit_fraction_per_second: 0.30,
    },
    HazardPreset {
        medium: HazardMedium::Lava,
        seconds_to_threaten: 0.0,
        light_hit_fraction_per_second: 0.80,
    },
    HazardPreset {
        medium: HazardMedium::ExhaustFume,
        seconds_to_threaten: 5.0,
        light_hit_fraction_per_second: 0.0,
    },
    HazardPreset {
        medium: HazardMedium::SporeBurst,
        seconds_to_threaten: 5.0,
        light_hit_fraction_per_second: 0.04,
    },
    HazardPreset {
        medium: HazardMedium::DeepWater,
        seconds_to_threaten: 0.0,
        light_hit_fraction_per_second: 0.0,
    },
    HazardPreset {
        medium: HazardMedium::Mire,
        seconds_to_threaten: 6.0,
        light_hit_fraction_per_second: 0.0,
    },
];

impl HazardPreset {
    pub fn by_medium(medium: HazardMedium) -> Option<Self> {
        PRESETS.iter().copied().find(|preset| preset.medium == medium)
    }

    pub fn valid(self) -> bool {
        self.seconds_to_threaten.is_finite()
            && self.seconds_to_threaten >= 0.0
            && self.light_hit_fraction_per_second.is_finite()
            && self.light_hit_fraction_per_second >= 0.0
    }

    /// Buildup per immersed second for a target whose status threshold is
    /// `threshold`. Status-bearing media only.
    pub fn buildup_per_second(self, threshold: f32) -> f32 {
        if !self.valid()
            || !threshold.is_finite()
            || threshold <= 0.0
            || self.seconds_to_threaten <= 0.0
        {
            return 0.0;
        }
        threshold / self.seconds_to_threaten
    }

    /// Turn a preset into a bridge-ready definition at one threshold.
    pub fn definition(self, threshold: f32) -> Option<HazardDefinition> {
        let status = self.medium.status()?;
        let per_second = self.buildup_per_second(threshold);
        if per_second <= 0.0 {
            return None;
        }
        Some(HazardDefinition {
            medium: self.medium,
            buildup_per_second: per_second,
            damage_per_second: self.light_hit_fraction_per_second,
            wading_scale: 0.5,
            brushed_scale: 0.25,
            status: StatusDefinition {
                // The bridge may pass a target-specific definition; these are
                // the neutral-target defaults: decay begins 2 s after the
                // last gain and empties the meter over ~10 s, and a proc
                // locks the meter for 5 s so continuous exposure cannot
                // chain-proc (matches ER's swamp re-proc cadence).
                threshold,
                decay_delay_seconds: 2.0,
                decay_per_second: threshold / 10.0,
                proc_lockout_seconds: 5.0,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: StatusDefinition = StatusDefinition {
        threshold: 100.0,
        decay_delay_seconds: 1.0,
        decay_per_second: 5.0,
        proc_lockout_seconds: 3.0,
    };

    const DEFINITIONS: [StatusDefinition; STATUS_COUNT] = [STATUS; STATUS_COUNT];

    fn definition(medium: HazardMedium) -> HazardDefinition {
        HazardDefinition {
            medium,
            buildup_per_second: 25.0,
            damage_per_second: 10.0,
            wading_scale: 0.5,
            brushed_scale: 0.25,
            status: STATUS,
        }
    }

    #[test]
    fn every_medium_maps_to_at_most_one_channel_of_each_kind() {
        for medium in HazardMedium::ALL {
            // No medium may deliver both a status and an element at once:
            // hazards are grossness or heat, never both through one channel.
            assert!(medium.status().is_none() || medium.element().is_none());
        }
        assert_eq!(HazardMedium::Mire.status(), Some(StatusKind::Sleep));
        assert_eq!(HazardMedium::ExhaustFume.status(), Some(StatusKind::Sleep));
        assert_eq!(HazardMedium::RotFume.status(), Some(StatusKind::Rot));
        assert_eq!(HazardMedium::Flame.status(), None);
        assert_eq!(HazardMedium::Flame.element(), Some(DamageElement::Fire));
        assert_eq!(HazardMedium::DeepWater.element(), None);
    }

    #[test]
    fn crosswalk_covers_every_medium_exactly_once() {
        let mut seen = [false; HAZARD_MEDIUM_COUNT];
        for entry in CROSSWALK {
            assert!(!seen[entry.medium.index()]);
            seen[entry.medium.index()] = true;
            assert!(!entry.elden_ring.is_empty());
            assert!(!entry.skyrim.is_empty());
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn clear_state_and_invalid_definitions_produce_no_exposure() {
        let mut vuln = HazardVulnerability::default();
        let medium = HazardMedium::PoisonGas;
        assert!(vuln.expose(medium, ExposureState::Clear, definition(medium), 1.0).is_none());

        let mut wrong = definition(medium);
        wrong.medium = HazardMedium::Flame;
        assert!(vuln.expose(medium, ExposureState::Immersed, wrong, 1.0).is_none());

        let mut invalid = definition(medium);
        invalid.buildup_per_second = f32::NAN;
        assert!(vuln.expose(medium, ExposureState::Immersed, invalid, 1.0).is_none());
    }

    #[test]
    fn resistance_gate_blocks_zero_and_negative_but_allows_vulnerability() {
        let mut vuln = HazardVulnerability::default();
        let medium = HazardMedium::RotFume;
        assert!(vuln.expose(medium, ExposureState::Immersed, definition(medium), 0.0).is_none());
        assert!(vuln.expose(medium, ExposureState::Immersed, definition(medium), -1.0).is_none());
        let hit = vuln
            .expose(medium, ExposureState::Immersed, definition(medium), 1.5)
            .expect("vulnerable actor must accrue exposure");
        assert!((hit.buildup - 37.5).abs() < 1e-4);
    }

    #[test]
    fn exposure_scales_downgrade_from_immersed_to_brushed() {
        let medium = HazardMedium::FrostVapor;
        let immersed = definition(medium)
            .exposure_scale(ExposureState::Immersed);
        let wading = definition(medium).exposure_scale(ExposureState::Wading);
        let brushed = definition(medium).exposure_scale(ExposureState::Brushed);
        assert_eq!(immersed, 1.0);
        assert_eq!(wading, 0.5);
        assert_eq!(brushed, 0.25);
    }

    #[test]
    fn continuous_exposure_procs_through_the_shared_meters() {
        let medium = HazardMedium::PoisonGas;
        let mut vuln = HazardVulnerability::default();
        // 25 buildup/second across 4 seconds reaches the 100 threshold.
        for _ in 0..4 {
            let hit = vuln
                .expose(medium, ExposureState::Immersed, definition(medium), 1.0)
                .expect("exposure inside the volume");
            assert_eq!(hit.status, Some(StatusKind::Poison));
        }
        // After the proc the meter reset and entered lockout; further buildup
        // during lockout is ignored by the shared meter rules.
        let statuses = vuln.statuses();
        let meter = statuses.meter(StatusKind::Poison);
        assert!(meter.lockout_remaining() > 0.0);
    }

    #[test]
    fn immunity_blocks_exposure_until_it_expires() {
        let medium = HazardMedium::SporeBurst;
        let mut vuln = HazardVulnerability::default();
        vuln.grant_immunity(medium, 2.0);
        assert!(vuln.expose(medium, ExposureState::Immersed, definition(medium), 1.0).is_none());
        vuln.tick(1.0, &DEFINITIONS);
        assert!(vuln.expose(medium, ExposureState::Immersed, definition(medium), 1.0).is_none());
        vuln.tick(1.0, &DEFINITIONS);
        assert!(vuln.expose(medium, ExposureState::Immersed, definition(medium), 1.0).is_some());
    }

    #[test]
    fn negative_tick_and_grant_are_rejected() {
        let medium = HazardMedium::Lava;
        let mut vuln = HazardVulnerability::default();
        vuln.grant_immunity(medium, -5.0);
        assert_eq!(vuln.immunity_remaining(medium), 0.0);
        vuln.tick(-1.0, &DEFINITIONS);
        vuln.tick(f32::NAN, &DEFINITIONS);
        assert_eq!(vuln.exposure_seconds(medium), 0.0);
    }

    #[test]
    fn clear_resets_exposure_and_immunity() {
        let medium = HazardMedium::PoisonGas;
        let mut vuln = HazardVulnerability::default();
        let _ = vuln.expose(medium, ExposureState::Immersed, definition(medium), 1.0);
        vuln.grant_immunity(medium, 10.0);
        vuln.clear();
        assert_eq!(vuln.exposure_seconds(medium), 0.0);
        assert_eq!(vuln.immunity_remaining(medium), 0.0);
    }

    #[test]
    fn presets_cover_every_medium_and_respect_the_one_channel_rule() {
        let mut seen = [false; HAZARD_MEDIUM_COUNT];
        for preset in PRESETS {
            assert!(preset.valid());
            assert!(!seen[preset.medium.index()]);
            seen[preset.medium.index()] = true;
            // A preset either threatens a status or ticks damage, and
            // status-bearing media keep the elemental share at zero.
            if preset.seconds_to_threaten > 0.0 {
                assert!(preset.medium.status().is_some());
                assert!(preset.medium.element().is_none());
            }
            assert!(HazardPreset::by_medium(preset.medium).is_some());
        }
        assert!(seen.iter().all(|&s| s));
        assert!(HazardPreset::by_medium(HazardMedium::DeepWater).is_some());
    }

    #[test]
    fn rot_threatens_faster_than_poison_like_the_games() {
        let poison = HazardPreset::by_medium(HazardMedium::PoisonGas).unwrap();
        let rot = HazardPreset::by_medium(HazardMedium::RotFume).unwrap();
        assert!(rot.seconds_to_threaten < poison.seconds_to_threaten);
        // At a 100 threshold, poison needs about 7 seconds of wading.
        let rate = poison.buildup_per_second(100.0);
        assert!((rate - 100.0 / 7.0).abs() < 1e-4);
        // Elemental media never build a status, so they have no definition.
        assert!(HazardPreset::by_medium(HazardMedium::Lava)
            .unwrap()
            .definition(100.0)
            .is_none());
        // A status medium produces a bridge-ready definition.
        let gas = poison.definition(100.0).expect("status medium");
        assert!((gas.buildup_per_second - 100.0 / 7.0).abs() < 1e-4);
    }
}
