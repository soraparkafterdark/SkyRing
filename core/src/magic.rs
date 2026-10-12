//! Catalysts, spell definitions, and the gear axis for magic.
//!
//! Magic is the last delivery family. It composes the action, delivery and
//! effect systems and adds exactly one new idea from Elden Ring: spell power
//! is a *ratio* of the casting catalyst's scaling, so a catalyst is to a mage
//! what a sword is to a warrior. See `docs/Magic overhaul.md` and the
//! Charter's scaling spine.
//!
//! What this module does not own, by the Charter:
//!
//! - Skyrim owns the magicka pool, its maximum, regeneration, perks, and the
//!   absolute spell magnitude. Everything here is a bounded relative
//!   multiplier over what Skyrim would assign; nothing is an Elden Ring
//!   absolute damage, HP, or FP value.
//! - Skyrim owns projectile flight and impact. A projectile spell is a release
//!   identity the bridge launches; a confirmed native impact is the only thing
//!   that produces a consequence (see `cast::ImpactLedger`).
//! - No skill, level, perk, or attribute ever disables a catalyst or a spell.
//!   Skill moves scaling inside a floor; it never creates a threshold.
//!
//! Numeric bounds below that are not stated by the Charter are marked
//! *tuning input*: they are conservative placeholders for the playtest the
//! design calls for ("play a mage from level 1 to 30"), not extracted facts.

use crate::consequences::{ElementalProfile, HitEvent};
use crate::effects::{ModifiedStat, Modifier, ModifierOperation, StackRule, STATUS_COUNT};

/// Skyrim's five schools. A catalyst's governing skill is its school. A spell's school is
/// descriptive: no rule compares it with a catalyst's, because skill never gates anything.
/// Only the catalyst *family* (staff or ring) decides what a catalyst can cast.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum School {
    Destruction = 0,
    Alteration = 1,
    Illusion = 2,
    Restoration = 3,
    Conjuration = 4,
}

/// Staves carry sorceries (held); rings carry incantations (worn, cast by
/// dedicating a hand). Destruction deliberately straddles both families.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CatalystKind {
    Staff = 0,
    Ring = 1,
}

impl CatalystKind {
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// Bit set of the catalyst families a spell may be cast from.
pub const FROM_STAFF: u8 = CatalystKind::Staff.bit();
pub const FROM_RING: u8 = CatalystKind::Ring.bit();

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hand {
    Left = 0,
    Right = 1,
}

impl Hand {
    pub const fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// Reference catalyst quality: a baseline staff. Elden Ring supplies the
/// *ordering and relative gaps* between catalysts; the absolute scale is
/// rebuilt against Skyrim's health economy, so qualities are ratios to this.
pub const REFERENCE_QUALITY: f32 = 1.0;
/// Tuning input: the range a catalyst's relative quality may take.
pub const MIN_QUALITY: f32 = 0.5;
pub const MAX_QUALITY: f32 = 1.5;
/// Tuning input: skill raises scaling by at most this factor, on a soft-cap
/// curve, and never lowers it below 1.0.
pub const MAX_SKILL_FACTOR: f32 = 1.25;
/// Tuning input: each upgrade level adds this much, up to `MAX_UPGRADE_LEVEL`.
pub const UPGRADE_STEP: f32 = 0.04;
pub const MAX_UPGRADE_LEVEL: u8 = 10;
/// Tuning input: the most a catalyst's scaling can reach, whatever the skill
/// and upgrade, so the relative ordering below the cap is the design's and the
/// absolute magnitude stays Skyrim's.
pub const MAX_SCALING: f32 = 2.0;
/// A passive boosts one spell family by a stated percentage and does nothing
/// else. Tuning input for the largest allowed boost.
pub const MAX_PASSIVE_BONUS: f32 = 0.5;
/// Tuning input: the cost trade a catalyst may make (Lusat's: best scaling at
/// +50% cost). Below 1.0 would be a free discount, so it starts at 1.0.
pub const MIN_COST_MULTIPLIER: f32 = 1.0;
pub const MAX_COST_MULTIPLIER: f32 = 2.0;
/// Tuning input: the final health-damage multiplier a spell hit can carry.
pub const MAX_SPELL_DAMAGE_MULTIPLIER: f32 = 2.5;
/// Tuning input: a charged cast's power at full charge, relative to uncharged.
pub const MAX_CHARGE_POWER: f32 = 2.0;

/// A spell family tag. Passives boost a family, so families give the roster
/// its build structure (gravity, black flame, frost, and so on).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpellFamily(pub u16);

/// At most one passive per catalyst: one family, one percentage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Passive {
    pub family: SpellFamily,
    /// A fraction: `0.3` boosts the family by 30%.
    pub bonus: f32,
}

/// A staff or ring. Exactly two gameplay properties: scaling and at most one
/// passive. Cost trade and school are the record's identity, not extra power.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Catalyst {
    pub id: u32,
    pub kind: CatalystKind,
    /// The governing Skyrim school (and so the governing skill).
    pub school: School,
    /// Relative quality, `MIN_QUALITY..=MAX_QUALITY`; `1.0` is the reference.
    pub quality: f32,
    /// Smithing-stone upgrade level, `0..=MAX_UPGRADE_LEVEL`.
    pub upgrade_level: u8,
    pub passive: Option<Passive>,
    /// Magicka cost multiplier for every spell cast from this catalyst.
    pub cost_multiplier: f32,
}

impl Catalyst {
    pub fn valid(self) -> bool {
        self.quality.is_finite()
            && (MIN_QUALITY..=MAX_QUALITY).contains(&self.quality)
            && self.upgrade_level <= MAX_UPGRADE_LEVEL
            && self.cost_multiplier.is_finite()
            && (MIN_COST_MULTIPLIER..=MAX_COST_MULTIPLIER).contains(&self.cost_multiplier)
            && self
                .passive
                .map_or(true, |p| p.bonus.is_finite() && p.bonus > 0.0 && p.bonus <= MAX_PASSIVE_BONUS)
    }

    /// `Base × f(skill) × (1 + upgrade)`, bounded. `skill` is the governing
    /// school's Skyrim skill, 0 to 100; unusable input reads as 0, which gives
    /// the same scaling a level-one character has. Never below the catalyst's
    /// own quality, so skill cannot suppress it.
    pub fn scaling(self, skill: f32) -> f32 {
        let upgrade = 1.0 + UPGRADE_STEP * f32::from(self.upgrade_level.min(MAX_UPGRADE_LEVEL));
        (self.quality * skill_factor(skill) * upgrade).clamp(MIN_QUALITY, MAX_SCALING)
    }
}

/// Soft-capped skill curve: 1.0 at skill 0, `MAX_SKILL_FACTOR` at 100, with
/// diminishing returns in between (the same soft-cap shape Elden Ring uses
/// rather than a straight line).
pub fn skill_factor(skill: f32) -> f32 {
    let s = if skill.is_finite() { (skill / 100.0).clamp(0.0, 1.0) } else { 0.0 };
    1.0 + (MAX_SKILL_FACTOR - 1.0) * (1.0 - (1.0 - s) * (1.0 - s))
}

/// Magicka regenerates normally out of combat and is frozen in combat
/// (Charter). Skyrim owns the pool and applies this; the core only states the
/// rule so the two cannot drift.
pub const fn magicka_regen_allowed(in_combat: bool) -> bool {
    !in_combat
}

/// The passive bonus that applies to `family` from the two catalysts in hand. The casting
/// hand supplies scaling; either hand supplies a passive, and **passives stack**
/// (`docs/Magic overhaul.md`: "Dual-wield casting: two catalysts, passives stack"). Only
/// passives of the spell's own family count, so a ring boosting another family does nothing
/// for this spell, and two rings with different passives give a different build from one
/// because each boosts its own family. Same-family bonuses add (`1 + a + b`, not `(1 + a)(1 + b)`,
/// the conservative reading; a tuning choice). With two catalysts the sum is at most
/// `2 x MAX_PASSIVE_BONUS`. One physical catalyst in both hands counts once.
pub fn passive_bonus(family: SpellFamily, casting: &Catalyst, offhand: Option<&Catalyst>) -> f32 {
    let off = offhand.filter(|other| other.id != casting.id);
    [Some(casting), off]
        .into_iter()
        .flatten()
        .filter_map(|c| c.passive)
        .filter(|p| p.family == family)
        .map(|p| p.bonus)
        .fold(0.0, |total, bonus| total + bonus)
}

/// How a spell reaches its target. The seven cases of the first magic slice
/// are combinations of delivery, payload and charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellDelivery {
    /// Resolves at release against the target Skyrim selects (a touch, a beam
    /// tick, a hitscan-style effect).
    Instant,
    /// Skyrim launches and simulates a projectile; a confirmed native impact
    /// is the only thing that resolves it.
    Projectile,
    /// Leaves a persistent area that resolves each target once per tick.
    Area,
    /// Applies to the caster at release; no impact is involved.
    SelfCast,
}

/// Seconds, measured from the start of the cast. Spells are actions, so they
/// carry the fields a weapon swing does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CastTiming {
    /// The release frame (for a charged spell, the earliest release).
    pub release_at: f32,
    /// A dodge before this moment cancels the cast; after it, the cast is
    /// committed. Elden Ring's casting is dangerous because of this window.
    pub cancel_until: f32,
    /// Seconds after the release frame during which control is not returned.
    pub recovery: f32,
    /// Hyperarmour: a poise break inside `[from, to)` does not interrupt.
    pub hyperarmour_from: f32,
    pub hyperarmour_to: f32,
}

impl CastTiming {
    pub fn valid(self) -> bool {
        let all = [self.release_at, self.cancel_until, self.recovery, self.hyperarmour_from, self.hyperarmour_to];
        all.iter().all(|v| v.is_finite() && *v >= 0.0)
            && self.release_at > 0.0
            && self.cancel_until <= self.release_at
            && self.hyperarmour_from <= self.hyperarmour_to
            && self.hyperarmour_to <= self.release_at + self.recovery
    }

    pub fn total(self) -> f32 {
        self.release_at + self.recovery
    }
}

/// A held cast: hold past `release_at` to charge toward `max_seconds`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Charge {
    /// Seconds of charging, after `release_at`, to reach full charge.
    pub max_seconds: f32,
    /// Power multiplier at full charge, `1.0..=MAX_CHARGE_POWER`.
    pub power_at_full: f32,
}

impl Charge {
    pub fn valid(self) -> bool {
        self.max_seconds.is_finite()
            && self.max_seconds > 0.0
            && self.power_at_full.is_finite()
            && (1.0..=MAX_CHARGE_POWER).contains(&self.power_at_full)
    }

    /// Power for `held` seconds of charging: linear from 1.0 to
    /// `power_at_full`, never beyond it (holding past full adds nothing).
    pub fn power(self, held: f32) -> f32 {
        1.0 + (self.power_at_full - 1.0) * self.fraction(held)
    }

    pub fn fraction(self, held: f32) -> f32 {
        if held.is_finite() { (held / self.max_seconds).clamp(0.0, 1.0) } else { 0.0 }
    }
}

/// A persistent area: a radius, how long it lasts, and how often it resolves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub radius: f32,
    pub duration: f32,
    pub tick_interval: f32,
}

impl Area {
    pub fn valid(self) -> bool {
        [self.radius, self.duration, self.tick_interval].iter().all(|v| v.is_finite() && *v > 0.0)
            && self.tick_interval <= self.duration
    }

    /// How many times the area resolves over its life.
    pub fn ticks(self) -> u32 {
        (self.duration / self.tick_interval).floor() as u32
    }
}

/// Who a modifier lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifierTarget {
    Caster,
    Target,
}

/// A modifier a spell applies. The stacking rule is data, never an accident
/// of application order. `source` is filled in at release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModifierTemplate {
    pub on: ModifierTarget,
    pub group: u32,
    pub stat: ModifiedStat,
    pub operation: ModifierOperation,
    pub magnitude: f32,
    pub duration: f32,
    pub priority: i16,
    pub stack_rule: StackRule,
}

impl ModifierTemplate {
    fn valid(self) -> bool {
        self.magnitude.is_finite()
            && self.duration.is_finite()
            && self.duration > 0.0
            && (self.operation != ModifierOperation::Multiply || self.magnitude >= 0.0)
    }

    pub fn instantiate(self, source: u64) -> Modifier {
        Modifier {
            source,
            group: self.group,
            stat: self.stat,
            operation: self.operation,
            magnitude: self.magnitude,
            remaining_seconds: self.duration,
            priority: self.priority,
            stack_rule: self.stack_rule,
        }
    }
}

/// Most modifiers one spell may carry: spells are shapes, not buff piles.
pub const MAX_SPELL_MODIFIERS: usize = 4;

/// What a spell does on resolution, in the same terms as every other delivery.
#[derive(Clone, Debug, PartialEq)]
pub struct SpellPayload {
    /// Shares of the Skyrim-owned damage request. All-zero means no damage.
    pub elements: ElementalProfile,
    pub poise_pressure: f32,
    pub stance_pressure: f32,
    /// Buildup per status, in the units the target's thresholds use.
    pub status_buildup: [f32; STATUS_COUNT],
    pub modifiers: Vec<ModifierTemplate>,
}

impl SpellPayload {
    pub fn damages(&self) -> bool {
        self.elements.total() > 0.0
    }

    pub fn builds_status(&self) -> bool {
        self.status_buildup.iter().any(|b| *b > 0.0)
    }

    fn valid(&self) -> bool {
        self.poise_pressure.is_finite()
            && self.poise_pressure >= 0.0
            && self.stance_pressure.is_finite()
            && self.stance_pressure >= 0.0
            && self.status_buildup.iter().all(|b| b.is_finite() && *b >= 0.0)
            && self.modifiers.len() <= MAX_SPELL_MODIFIERS
            && self.modifiers.iter().all(|m| m.valid())
    }

    fn has(&self, on: ModifierTarget) -> bool {
        self.modifiers.iter().any(|m| m.on == on)
    }
}

/// Largest power ratio a spell may carry. Tuning input.
pub const MAX_POWER_RATIO: f32 = 1.5;

#[derive(Clone, Debug, PartialEq)]
pub struct SpellDefinition {
    pub id: u32,
    pub school: School,
    pub family: SpellFamily,
    pub delivery: SpellDelivery,
    /// Which catalyst families may cast it (`FROM_STAFF | FROM_RING`).
    pub castable_from: u8,
    /// A fraction of Scaling, never a damage number.
    pub power_ratio: f32,
    /// Cost as a fraction of the caster's maximum magicka, before the
    /// catalyst's multiplier and any cost modifiers.
    pub cost_fraction: f32,
    pub timing: CastTiming,
    pub charge: Option<Charge>,
    pub area: Option<Area>,
    pub payload: SpellPayload,
}

/// Why a definition is not acceptable. Each names the one rule it broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellError {
    NotFinite,
    PowerRatioOutOfRange,
    CostOutOfRange,
    NoCatalystFamily,
    BadTiming,
    BadCharge,
    BadArea,
    AreaMismatch,
    BadPayload,
    EmptyPayload,
    SelfCastNeedsCasterEffect,
    SelfCastCannotHitTargets,
    TargetedNeedsTargetEffect,
}

impl SpellDefinition {
    pub fn validate(&self) -> Result<(), SpellError> {
        if !self.power_ratio.is_finite() || !self.cost_fraction.is_finite() {
            return Err(SpellError::NotFinite);
        }
        if !(0.0..=MAX_POWER_RATIO).contains(&self.power_ratio) {
            return Err(SpellError::PowerRatioOutOfRange);
        }
        if !(0.0..=1.0).contains(&self.cost_fraction) {
            return Err(SpellError::CostOutOfRange);
        }
        if self.castable_from & (FROM_STAFF | FROM_RING) == 0 || self.castable_from & !(FROM_STAFF | FROM_RING) != 0 {
            return Err(SpellError::NoCatalystFamily);
        }
        if !self.timing.valid() {
            return Err(SpellError::BadTiming);
        }
        if self.charge.map_or(false, |c| !c.valid()) {
            return Err(SpellError::BadCharge);
        }
        match (self.delivery, self.area) {
            (SpellDelivery::Area, Some(area)) if area.valid() => {}
            (SpellDelivery::Area, Some(_)) => return Err(SpellError::BadArea),
            (SpellDelivery::Area, None) => return Err(SpellError::AreaMismatch),
            (_, Some(_)) => return Err(SpellError::AreaMismatch),
            (_, None) => {}
        }
        if !self.payload.valid() {
            return Err(SpellError::BadPayload);
        }
        let p = &self.payload;
        if !p.damages() && !p.builds_status() && p.modifiers.is_empty() {
            return Err(SpellError::EmptyPayload);
        }
        match self.delivery {
            SpellDelivery::SelfCast => {
                if p.damages() || p.builds_status() || p.has(ModifierTarget::Target) {
                    return Err(SpellError::SelfCastCannotHitTargets);
                }
                if !p.has(ModifierTarget::Caster) {
                    return Err(SpellError::SelfCastNeedsCasterEffect);
                }
            }
            _ => {
                if !p.damages() && !p.builds_status() && !p.has(ModifierTarget::Target) {
                    return Err(SpellError::TargetedNeedsTargetEffect);
                }
            }
        }
        Ok(())
    }

    /// Whether `catalyst` may cast this spell. Only the family matters: a
    /// level-one character may cast anything their catalyst's family allows.
    pub fn castable_with(&self, catalyst: &Catalyst) -> bool {
        self.castable_from & catalyst.kind.bit() != 0
    }

    /// Magicka this cast takes, in the host's units. The catalyst's cost trade
    /// and any bounded cost modifier (perks, effects) scale it.
    pub fn magicka_cost(&self, catalyst: &Catalyst, cost_scale: f32, magicka_max: f32) -> f32 {
        let scale = if cost_scale.is_finite() { cost_scale.clamp(0.0, 4.0) } else { 1.0 };
        let max = if magicka_max.is_finite() { magicka_max.max(0.0) } else { 0.0 };
        self.cost_fraction * catalyst.cost_multiplier * scale * max
    }
}

/// Everything about a release that must not change after it, so swapping gear
/// mid-flight cannot mutate a spell already cast.
#[derive(Clone, Debug, PartialEq)]
pub struct SpellSnapshot {
    pub release_id: u64,
    pub caster: u64,
    pub spell: SpellDefinition,
    pub catalyst_id: u32,
    /// Scaling, passive and charge already applied: the final multiplier over
    /// the damage Skyrim assigns to the spell.
    pub damage_multiplier: f32,
    pub charge_fraction: f32,
}

/// The spell's final damage multiplier. `power_ratio × scaling ×
/// (1 + passive) × charge`, bounded. Skyrim's own magnitude, perks, armour and
/// resistance remain in force: this multiplies what it assigns.
pub fn spell_damage_multiplier(
    spell: &SpellDefinition,
    casting: &Catalyst,
    offhand: Option<&Catalyst>,
    skill: f32,
    charge_power: f32,
) -> f32 {
    let passive = 1.0 + passive_bonus(spell.family, casting, offhand);
    let charge = if charge_power.is_finite() { charge_power.clamp(1.0, MAX_CHARGE_POWER) } else { 1.0 };
    (spell.power_ratio * casting.scaling(skill) * passive * charge).clamp(0.0, MAX_SPELL_DAMAGE_MULTIPLIER)
}

impl SpellSnapshot {
    /// The same `HitEvent` a melee swing or an arrow produces. `hit_id` names
    /// this resolution (an area tick has its own); the bridge supplies `target`.
    /// Caster-targeted modifiers are not part of a hit; see `caster_modifiers`.
    pub fn hit_event(&self, target: u64, hit_id: u32) -> HitEvent {
        HitEvent {
            source: self.caster,
            target,
            action_id: self.spell.id,
            hit_id,
            damage_multiplier: if self.spell.payload.damages() { self.damage_multiplier } else { 0.0 },
            armament_multiplier: 1.0,
            elements: self.spell.payload.elements,
            poise_pressure: self.spell.payload.poise_pressure,
            stance_pressure: self.spell.payload.stance_pressure,
            status_buildup: self.spell.payload.status_buildup,
            target_modifiers: self
                .spell
                .payload
                .modifiers
                .iter()
                .filter(|m| m.on == ModifierTarget::Target)
                .map(|m| m.instantiate(self.caster))
                .collect(),
        }
    }

    /// Modifiers the cast applies to its caster (a buff).
    pub fn caster_modifiers(&self) -> Vec<Modifier> {
        self.spell
            .payload
            .modifiers
            .iter()
            .filter(|m| m.on == ModifierTarget::Caster)
            .map(|m| m.instantiate(self.caster))
            .collect()
    }
}
