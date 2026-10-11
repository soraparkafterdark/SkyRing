//! Separate defensive poise and offensive stance meters.
//!
//! Poise answers whether an actor is interrupted by the current hit. Stance
//! accumulates pressure on a target until it is opened for a critical. They
//! deliberately do not share state, recovery, or break rules.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoiseDefinition {
    pub threshold: f32,
    pub recovery_delay_seconds: f32,
    pub recovery_per_second: f32,
}

impl PoiseDefinition {
    pub fn valid(self) -> bool {
        self.threshold.is_finite()
            && self.threshold > 0.0
            && self.recovery_delay_seconds.is_finite()
            && self.recovery_delay_seconds >= 0.0
            && self.recovery_per_second.is_finite()
            && self.recovery_per_second >= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PoiseResult {
    Ignored,
    Withstood { accumulated: f32, threshold: f32 },
    Interrupted { source: u64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoiseState {
    accumulated: f32,
    seconds_since_pressure: f32,
}

impl PoiseState {
    pub fn accumulated(self) -> f32 {
        self.accumulated
    }

    /// Apply interruption pressure. `threshold_multiplier` is where armour,
    /// hyperarmour, and target-side poise modifiers compose; 1 is neutral.
    pub fn apply(
        &mut self,
        pressure: f32,
        threshold_multiplier: f32,
        definition: PoiseDefinition,
        source: u64,
    ) -> PoiseResult {
        if !definition.valid()
            || !pressure.is_finite()
            || pressure <= 0.0
            || !threshold_multiplier.is_finite()
            || threshold_multiplier <= 0.0
        {
            return PoiseResult::Ignored;
        }
        let threshold = definition.threshold * threshold_multiplier;
        if !threshold.is_finite() || threshold <= 0.0 {
            return PoiseResult::Ignored;
        }
        self.accumulated += pressure;
        self.seconds_since_pressure = 0.0;
        if self.accumulated >= threshold {
            self.accumulated = 0.0;
            PoiseResult::Interrupted { source }
        } else {
            PoiseResult::Withstood {
                accumulated: self.accumulated,
                threshold,
            }
        }
    }

    pub fn tick(&mut self, delta_seconds: f32, definition: PoiseDefinition) {
        if !definition.valid() || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        let before = self.seconds_since_pressure;
        self.seconds_since_pressure += delta_seconds;
        let recovery_before = (before - definition.recovery_delay_seconds).max(0.0);
        let recovery_after =
            (self.seconds_since_pressure - definition.recovery_delay_seconds).max(0.0);
        self.accumulated = (self.accumulated
            - definition.recovery_per_second * (recovery_after - recovery_before))
            .max(0.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StanceDefinition {
    pub threshold: f32,
    pub decay_delay_seconds: f32,
    pub decay_per_second: f32,
    pub break_lockout_seconds: f32,
}

impl StanceDefinition {
    pub fn valid(self) -> bool {
        self.threshold.is_finite()
            && self.threshold > 0.0
            && self.decay_delay_seconds.is_finite()
            && self.decay_delay_seconds >= 0.0
            && self.decay_per_second.is_finite()
            && self.decay_per_second >= 0.0
            && self.break_lockout_seconds.is_finite()
            && self.break_lockout_seconds >= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StanceResult {
    Ignored,
    Accumulated { current: f32, threshold: f32 },
    Broken { source: u64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StanceState {
    accumulated: f32,
    seconds_since_pressure: f32,
    lockout_remaining: f32,
}

impl StanceState {
    pub fn accumulated(self) -> f32 {
        self.accumulated
    }

    pub fn lockout_remaining(self) -> f32 {
        self.lockout_remaining
    }

    pub fn apply(
        &mut self,
        pressure: f32,
        received_multiplier: f32,
        definition: StanceDefinition,
        source: u64,
    ) -> StanceResult {
        if !definition.valid()
            || self.lockout_remaining > 0.0
            || !pressure.is_finite()
            || pressure <= 0.0
            || !received_multiplier.is_finite()
            || received_multiplier <= 0.0
        {
            return StanceResult::Ignored;
        }
        let gained = pressure * received_multiplier;
        if !gained.is_finite() || gained <= 0.0 {
            return StanceResult::Ignored;
        }
        self.accumulated += gained;
        self.seconds_since_pressure = 0.0;
        if self.accumulated >= definition.threshold {
            self.accumulated = 0.0;
            self.lockout_remaining = definition.break_lockout_seconds;
            StanceResult::Broken { source }
        } else {
            StanceResult::Accumulated {
                current: self.accumulated,
                threshold: definition.threshold,
            }
        }
    }

    pub fn tick(&mut self, delta_seconds: f32, definition: StanceDefinition) {
        if !definition.valid() || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        self.lockout_remaining = (self.lockout_remaining - delta_seconds).max(0.0);
        let before = self.seconds_since_pressure;
        self.seconds_since_pressure += delta_seconds;
        let decay_before = (before - definition.decay_delay_seconds).max(0.0);
        let decay_after = (self.seconds_since_pressure - definition.decay_delay_seconds).max(0.0);
        self.accumulated = (self.accumulated
            - definition.decay_per_second * (decay_after - decay_before))
            .max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POISE: PoiseDefinition = PoiseDefinition {
        threshold: 50.0,
        recovery_delay_seconds: 1.0,
        recovery_per_second: 10.0,
    };
    const STANCE: StanceDefinition = StanceDefinition {
        threshold: 100.0,
        decay_delay_seconds: 2.0,
        decay_per_second: 20.0,
        break_lockout_seconds: 1.0,
    };

    #[test]
    fn poise_accumulates_recovers_and_interrupts() {
        let mut poise = PoiseState::default();
        assert_eq!(
            poise.apply(30.0, 1.0, POISE, 1),
            PoiseResult::Withstood {
                accumulated: 30.0,
                threshold: 50.0
            }
        );
        poise.tick(1.5, POISE);
        assert_eq!(poise.accumulated(), 25.0);
        assert_eq!(
            poise.apply(25.0, 1.0, POISE, 7),
            PoiseResult::Interrupted { source: 7 }
        );
        assert_eq!(poise.accumulated(), 0.0);
    }

    #[test]
    fn hyperarmour_changes_poise_threshold_not_stance() {
        let mut poise = PoiseState::default();
        assert_eq!(
            poise.apply(60.0, 2.0, POISE, 1),
            PoiseResult::Withstood {
                accumulated: 60.0,
                threshold: 100.0
            }
        );
        let mut stance = StanceState::default();
        assert_eq!(
            stance.apply(60.0, 1.0, STANCE, 1),
            StanceResult::Accumulated {
                current: 60.0,
                threshold: 100.0
            }
        );
    }

    #[test]
    fn stance_decays_breaks_and_honours_post_break_lockout() {
        let mut stance = StanceState::default();
        stance.apply(80.0, 1.0, STANCE, 1);
        stance.tick(3.0, STANCE);
        assert_eq!(stance.accumulated(), 60.0);
        assert_eq!(
            stance.apply(40.0, 1.0, STANCE, 2),
            StanceResult::Broken { source: 2 }
        );
        assert!(stance.lockout_remaining() > 0.0);
        assert_eq!(stance.apply(100.0, 1.0, STANCE, 3), StanceResult::Ignored);
        stance.tick(1.0, STANCE);
        assert!(matches!(
            stance.apply(20.0, 1.0, STANCE, 3),
            StanceResult::Accumulated { .. }
        ));
    }

    #[test]
    fn invalid_pressure_never_poisons_meter_state() {
        let mut poise = PoiseState::default();
        assert_eq!(poise.apply(f32::NAN, 1.0, POISE, 1), PoiseResult::Ignored);
        assert_eq!(poise.accumulated(), 0.0);
        let mut stance = StanceState::default();
        assert_eq!(
            stance.apply(f32::INFINITY, 1.0, STANCE, 1),
            StanceResult::Ignored
        );
        assert_eq!(stance.accumulated(), 0.0);
    }
}
