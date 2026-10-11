//! Delivery-agnostic status and modifier rules.
//!
//! Melee, spells, throwables, hazards, and creature attacks all feed this
//! layer. It produces semantic procs and evaluated modifiers; Skyrim remains
//! responsible for health damage, actor values, visuals, and persistence.

pub const STATUS_COUNT: usize = 6;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusKind {
    Bleed = 0,
    Frostbite = 1,
    Poison = 2,
    Rot = 3,
    Sleep = 4,
    Madness = 5,
}

impl StatusKind {
    pub const ALL: [Self; STATUS_COUNT] = [
        Self::Bleed,
        Self::Frostbite,
        Self::Poison,
        Self::Rot,
        Self::Sleep,
        Self::Madness,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Actor- and status-specific tuning. Threshold and decay are Skyrim-facing
/// balance values, not copied absolute values from Elden Ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusDefinition {
    pub threshold: f32,
    pub decay_delay_seconds: f32,
    pub decay_per_second: f32,
    pub proc_lockout_seconds: f32,
}

impl StatusDefinition {
    pub fn valid(self) -> bool {
        self.threshold.is_finite()
            && self.threshold > 0.0
            && self.decay_delay_seconds.is_finite()
            && self.decay_delay_seconds >= 0.0
            && self.decay_per_second.is_finite()
            && self.decay_per_second >= 0.0
            && self.proc_lockout_seconds.is_finite()
            && self.proc_lockout_seconds >= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusProc {
    pub kind: StatusKind,
    /// Stable bridge-owned source identifier. The effect kernel does not
    /// interpret it, but preserves it for attribution and kill credit.
    pub source: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BuildupResult {
    Ignored,
    Accumulated { current: f32, threshold: f32 },
    Proc(StatusProc),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatusMeter {
    buildup: f32,
    seconds_since_gain: f32,
    lockout_remaining: f32,
    last_source: u64,
}

impl StatusMeter {
    pub fn buildup(self) -> f32 {
        self.buildup
    }

    pub fn lockout_remaining(self) -> f32 {
        self.lockout_remaining
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusSet {
    meters: [StatusMeter; STATUS_COUNT],
}

impl StatusSet {
    pub fn meter(&self, kind: StatusKind) -> StatusMeter {
        self.meters[kind.index()]
    }

    /// Apply raw buildup after an actor-specific resistance multiplier.
    /// `received_multiplier` is 1 for neutral, 0 for immune, and may exceed 1
    /// for vulnerability. Buildup during a post-proc lockout is ignored.
    pub fn apply(
        &mut self,
        kind: StatusKind,
        raw_buildup: f32,
        received_multiplier: f32,
        definition: StatusDefinition,
        source: u64,
    ) -> BuildupResult {
        if !definition.valid()
            || !raw_buildup.is_finite()
            || raw_buildup <= 0.0
            || !received_multiplier.is_finite()
            || received_multiplier <= 0.0
        {
            return BuildupResult::Ignored;
        }
        let meter = &mut self.meters[kind.index()];
        if meter.lockout_remaining > 0.0 {
            return BuildupResult::Ignored;
        }
        let gained = raw_buildup * received_multiplier;
        if !gained.is_finite() || gained <= 0.0 {
            return BuildupResult::Ignored;
        }
        meter.buildup += gained;
        meter.seconds_since_gain = 0.0;
        meter.last_source = source;
        if meter.buildup >= definition.threshold {
            meter.buildup = 0.0;
            meter.lockout_remaining = definition.proc_lockout_seconds;
            BuildupResult::Proc(StatusProc { kind, source })
        } else {
            BuildupResult::Accumulated {
                current: meter.buildup,
                threshold: definition.threshold,
            }
        }
    }

    /// Advance buildup decay and post-proc lockouts for every status.
    pub fn tick(&mut self, delta_seconds: f32, definitions: &[StatusDefinition; STATUS_COUNT]) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        for kind in StatusKind::ALL {
            let definition = definitions[kind.index()];
            if !definition.valid() {
                continue;
            }
            let meter = &mut self.meters[kind.index()];
            meter.lockout_remaining = (meter.lockout_remaining - delta_seconds).max(0.0);
            let before = meter.seconds_since_gain;
            meter.seconds_since_gain += delta_seconds;
            let decay_before = (before - definition.decay_delay_seconds).max(0.0);
            let decay_after = (meter.seconds_since_gain - definition.decay_delay_seconds).max(0.0);
            let decay_seconds = decay_after - decay_before;
            meter.buildup = (meter.buildup - definition.decay_per_second * decay_seconds).max(0.0);
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModifiedStat {
    DamageDealt,
    DamageTaken,
    StaminaCost,
    MagickaCost,
    CastSpeed,
    MovementSpeed,
    Poise,
    StanceDamage,
    StatusBuildupDealt,
    StatusBuildupTaken,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModifierOperation {
    Add,
    /// A literal factor: `1.10` means ten percent stronger.
    Multiply,
    Override,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StackRule {
    /// New application replaces every modifier in its stacking group.
    Replace,
    /// Keep magnitude and refresh the existing duration.
    Refresh,
    /// Keep the numerically strongest magnitude and refresh duration.
    Strongest,
    /// One instance per source; reapplication from that source replaces it.
    BySource,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modifier {
    pub source: u64,
    pub group: u32,
    pub stat: ModifiedStat,
    pub operation: ModifierOperation,
    pub magnitude: f32,
    pub remaining_seconds: f32,
    pub priority: i16,
    pub stack_rule: StackRule,
}

impl Modifier {
    fn valid(self) -> bool {
        self.magnitude.is_finite()
            && self.remaining_seconds.is_finite()
            && self.remaining_seconds > 0.0
            && (self.operation != ModifierOperation::Multiply || self.magnitude >= 0.0)
    }

    fn strength(self) -> f32 {
        match self.operation {
            ModifierOperation::Multiply => (self.magnitude - 1.0).abs(),
            ModifierOperation::Add | ModifierOperation::Override => self.magnitude.abs(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModifierSet {
    active: Vec<Modifier>,
}

impl ModifierSet {
    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Apply an explicit stacking rule. Group zero is valid; callers choose
    /// stable group identifiers rather than relying on insertion order.
    pub fn apply(&mut self, modifier: Modifier) -> bool {
        if !modifier.valid() {
            return false;
        }
        let same_group = |candidate: &Modifier| {
            candidate.group == modifier.group
                && candidate.stat == modifier.stat
                && candidate.operation == modifier.operation
        };
        match modifier.stack_rule {
            StackRule::Replace => {
                self.active.retain(|candidate| !same_group(candidate));
                self.active.push(modifier);
            }
            StackRule::Refresh => {
                if let Some(existing) = self
                    .active
                    .iter_mut()
                    .find(|candidate| same_group(candidate))
                {
                    existing.remaining_seconds =
                        existing.remaining_seconds.max(modifier.remaining_seconds);
                } else {
                    self.active.push(modifier);
                }
            }
            StackRule::Strongest => {
                if let Some(existing) = self
                    .active
                    .iter_mut()
                    .find(|candidate| same_group(candidate))
                {
                    if modifier.strength() > existing.strength() {
                        *existing = modifier;
                    } else {
                        existing.remaining_seconds =
                            existing.remaining_seconds.max(modifier.remaining_seconds);
                    }
                } else {
                    self.active.push(modifier);
                }
            }
            StackRule::BySource => {
                if let Some(existing) = self
                    .active
                    .iter_mut()
                    .find(|candidate| same_group(candidate) && candidate.source == modifier.source)
                {
                    *existing = modifier;
                } else {
                    self.active.push(modifier);
                }
            }
        }
        true
    }

    pub fn tick(&mut self, delta_seconds: f32) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        for modifier in &mut self.active {
            modifier.remaining_seconds -= delta_seconds;
        }
        self.active
            .retain(|modifier| modifier.remaining_seconds > 0.0);
    }

    /// Additions apply first, then multiplicative factors. The highest-priority
    /// override wins; equal-priority overrides use the lower stable group id.
    pub fn evaluate(&self, stat: ModifiedStat, base: f32) -> f32 {
        if !base.is_finite() {
            return base;
        }
        let mut additive = 0.0;
        let mut multiplicative = 1.0;
        let mut override_value: Option<(i16, u32, f32)> = None;
        for modifier in self.active.iter().filter(|modifier| modifier.stat == stat) {
            match modifier.operation {
                ModifierOperation::Add => additive += modifier.magnitude,
                ModifierOperation::Multiply => multiplicative *= modifier.magnitude,
                ModifierOperation::Override => {
                    let candidate = (modifier.priority, modifier.group, modifier.magnitude);
                    if override_value.is_none_or(|current| {
                        candidate.0 > current.0
                            || (candidate.0 == current.0 && candidate.1 < current.1)
                    }) {
                        override_value = Some(candidate);
                    }
                }
            }
        }
        override_value.map_or((base + additive) * multiplicative, |(_, _, value)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: StatusDefinition = StatusDefinition {
        threshold: 100.0,
        decay_delay_seconds: 2.0,
        decay_per_second: 10.0,
        proc_lockout_seconds: 1.0,
    };
    const DEFINITIONS: [StatusDefinition; STATUS_COUNT] = [STATUS; STATUS_COUNT];

    fn modifier(group: u32, magnitude: f32, stack_rule: StackRule) -> Modifier {
        Modifier {
            source: 1,
            group,
            stat: ModifiedStat::DamageDealt,
            operation: ModifierOperation::Multiply,
            magnitude,
            remaining_seconds: 10.0,
            priority: 0,
            stack_rule,
        }
    }

    #[test]
    fn statuses_accumulate_independently_and_preserve_proc_source() {
        let mut statuses = StatusSet::default();
        assert_eq!(
            statuses.apply(StatusKind::Bleed, 60.0, 1.0, STATUS, 41),
            BuildupResult::Accumulated {
                current: 60.0,
                threshold: 100.0
            }
        );
        statuses.apply(StatusKind::Frostbite, 25.0, 1.0, STATUS, 9);
        assert_eq!(
            statuses.apply(StatusKind::Bleed, 25.0, 2.0, STATUS, 42),
            BuildupResult::Proc(StatusProc {
                kind: StatusKind::Bleed,
                source: 42
            })
        );
        assert_eq!(statuses.meter(StatusKind::Bleed).buildup(), 0.0);
        assert_eq!(statuses.meter(StatusKind::Frostbite).buildup(), 25.0);
    }

    #[test]
    fn immunity_decay_delay_and_proc_lockout_are_explicit() {
        let mut statuses = StatusSet::default();
        assert_eq!(
            statuses.apply(StatusKind::Poison, 80.0, 0.0, STATUS, 1),
            BuildupResult::Ignored
        );
        statuses.apply(StatusKind::Poison, 80.0, 1.0, STATUS, 1);
        statuses.tick(1.0, &DEFINITIONS);
        assert_eq!(statuses.meter(StatusKind::Poison).buildup(), 80.0);
        statuses.tick(2.0, &DEFINITIONS);
        assert_eq!(statuses.meter(StatusKind::Poison).buildup(), 70.0);
        statuses.apply(StatusKind::Poison, 40.0, 1.0, STATUS, 2);
        assert!(statuses.meter(StatusKind::Poison).lockout_remaining() > 0.0);
        assert_eq!(
            statuses.apply(StatusKind::Poison, 100.0, 1.0, STATUS, 3),
            BuildupResult::Ignored
        );
    }

    #[test]
    fn modifiers_have_deterministic_stacking_and_evaluation_order() {
        let mut modifiers = ModifierSet::default();
        assert!(modifiers.apply(modifier(1, 1.10, StackRule::Replace)));
        assert!(modifiers.apply(modifier(2, 1.20, StackRule::Replace)));
        let mut additive = modifier(3, 5.0, StackRule::Replace);
        additive.operation = ModifierOperation::Add;
        modifiers.apply(additive);
        assert!((modifiers.evaluate(ModifiedStat::DamageDealt, 100.0) - 138.6).abs() < 1e-4);

        modifiers.apply(modifier(1, 1.05, StackRule::Refresh));
        assert_eq!(modifiers.len(), 3, "refresh does not create another stack");
        assert!((modifiers.evaluate(ModifiedStat::DamageDealt, 100.0) - 138.6).abs() < 1e-4);
    }

    #[test]
    fn strongest_and_by_source_rules_do_what_their_names_say() {
        let mut modifiers = ModifierSet::default();
        modifiers.apply(modifier(5, 1.10, StackRule::Strongest));
        modifiers.apply(modifier(5, 1.05, StackRule::Strongest));
        assert_eq!(modifiers.len(), 1);
        assert!((modifiers.evaluate(ModifiedStat::DamageDealt, 100.0) - 110.0).abs() < 1e-4);

        let mut other_source = modifier(6, 1.10, StackRule::BySource);
        modifiers.apply(other_source);
        other_source.source = 2;
        modifiers.apply(other_source);
        assert_eq!(modifiers.len(), 3);
    }

    #[test]
    fn modifiers_expire_and_invalid_values_are_rejected() {
        let mut modifiers = ModifierSet::default();
        let mut short = modifier(1, 1.10, StackRule::Replace);
        short.remaining_seconds = 0.5;
        modifiers.apply(short);
        modifiers.tick(0.5);
        assert!(modifiers.is_empty());

        let mut invalid = modifier(2, f32::NAN, StackRule::Replace);
        assert!(!modifiers.apply(invalid));
        invalid.magnitude = -1.0;
        assert!(!modifiers.apply(invalid));
    }
}
