//! Spells as committed actions, and exactly-once impact correlation.
//!
//! A cast is an action: a startup you can be punished during, a release
//! frame, a cancel window that closes before it, hyperarmour for part of it,
//! and a recovery you cannot leave by rolling. `Caster` is that state machine
//! for one actor. It is deterministic: the same requests and inputs at the
//! same ticks produce the same events, with no wall clock and no pointers.
//!
//! Skyrim owns the magicka pool. The caster reads the pool each tick, charges
//! the cost at the release frame, and reports a monotonic spent total the
//! bridge turns into Skyrim magicka damage (the same shape as stamina).
//!
//! `ImpactLedger` is the spell half of the ranged contract in
//! `docs/EXECUTION_PLAN.md`: a release is snapshotted, Skyrim simulates any
//! flight, and only a matching native impact resolves it, once per target
//! (once per tick for an area). Unknown, stale, duplicate and out-of-range
//! impacts are refused as diagnostics, never as damage permission.

use crate::consequences::HitEvent;
use crate::magic::{
    spell_damage_multiplier, Catalyst, Hand, SpellDefinition, SpellDelivery, SpellError, SpellSnapshot,
};

/// Everything fixed at the moment a cast begins. Equipment changed afterwards
/// cannot alter a cast already under way.
#[derive(Clone, Copy, Debug)]
pub struct CastRequest<'a> {
    pub hand: Hand,
    pub casting: &'a Catalyst,
    pub offhand: Option<&'a Catalyst>,
    pub spell: &'a SpellDefinition,
    /// The governing school's Skyrim skill, 0 to 100.
    pub skill: f32,
    pub magicka_current: f32,
    pub magicka_max: f32,
    /// Bounded cost modifier from perks and effects; 1.0 is none.
    pub cost_scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CastRefusal {
    /// A cast is already under way. One at a time per actor.
    Busy,
    InvalidCatalyst,
    InvalidSpell(SpellError),
    /// The catalyst's family cannot cast this spell. No skill or level gate
    /// exists: this is the only way a spell is unavailable to a catalyst.
    WrongCatalystFamily,
    /// The host's pool is not usable (non-finite or negative maximum).
    InvalidPool,
    NotEnoughMagicka { needed: f32, available: f32 },
}

/// What the bridge tells the caster each tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct CastInput {
    /// The cast button is held (charged spells charge while it is).
    pub held: bool,
    /// A dodge was requested this tick.
    pub dodge: bool,
    /// The posture system broke poise this tick.
    pub poise_broken: bool,
    /// Skyrim's live magicka, checked again at the release frame.
    pub magicka_current: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CastEvent {
    Started { hand: Hand, spell_id: u32 },
    /// The release frame. Carries the immutable snapshot the bridge launches
    /// (or applies, for a self-cast) and the ledger resolves.
    Released(SpellSnapshot),
    /// The pool was too low at the release frame. No spell, no cost.
    Fizzled { needed: f32, available: f32 },
    /// A dodge inside the cancel window. No cost.
    Cancelled,
    /// Poise broke outside hyperarmour. Before the release it costs nothing.
    Interrupted,
    /// Recovery ended; control returns.
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Idle,
    /// Seconds since the cast began, before the release frame.
    Winding(f32),
    /// Seconds of charge held after the release frame.
    Charging(f32),
    /// Seconds since the release, and the charge the release used.
    Recovering { since_release: f32, charged: f32 },
}

/// One cast's frozen inputs.
#[derive(Clone, Debug)]
struct Active {
    hand: Hand,
    spell: SpellDefinition,
    casting: Catalyst,
    offhand: Option<Catalyst>,
    skill: f32,
    magicka_max: f32,
    cost_scale: f32,
}

#[derive(Clone, Debug)]
pub struct Caster {
    id: u32,
    serial: u32,
    phase: Phase,
    active: Option<Active>,
    spent_total: f64,
    reported_total: f64,
}

impl Caster {
    /// `id` is the actor's stable identity, so release ids differ per actor.
    pub fn new(id: u32) -> Self {
        Self { id, serial: 0, phase: Phase::Idle, active: None, spent_total: 0.0, reported_total: 0.0 }
    }

    pub fn is_idle(&self) -> bool {
        self.phase == Phase::Idle
    }

    /// The hand a cast has dedicated, from startup through recovery. That hand
    /// is not blocking, attacking or two-handing; a ring casts the same way a
    /// staff does.
    pub fn dedicated_hand(&self) -> Option<Hand> {
        self.active.as_ref().map(|a| a.hand)
    }

    /// Whether a dodge would be honoured now. Spells cannot be cancelled into
    /// a roll once past the cancel window.
    pub fn dodge_allowed(&self) -> bool {
        match (self.phase, &self.active) {
            (Phase::Idle, _) => true,
            (Phase::Winding(elapsed), Some(a)) => elapsed <= a.spell.timing.cancel_until,
            _ => false,
        }
    }

    /// Magicka spent since the last call, in the host's units, for Skyrim to
    /// take from its own pool. Nothing is saved on the core's side.
    pub fn take_spent(&mut self) -> f32 {
        let delta = (self.spent_total - self.reported_total) as f32;
        self.reported_total = self.spent_total;
        delta
    }

    pub fn begin(&mut self, request: CastRequest<'_>) -> Result<CastEvent, CastRefusal> {
        if self.phase != Phase::Idle {
            return Err(CastRefusal::Busy);
        }
        if !request.casting.valid() || request.offhand.map_or(false, |c| !c.valid()) {
            return Err(CastRefusal::InvalidCatalyst);
        }
        request.spell.validate().map_err(CastRefusal::InvalidSpell)?;
        if !request.spell.castable_with(request.casting) {
            return Err(CastRefusal::WrongCatalystFamily);
        }
        if !request.magicka_max.is_finite() || request.magicka_max < 0.0 || !request.magicka_current.is_finite() {
            return Err(CastRefusal::InvalidPool);
        }
        let needed = request.spell.magicka_cost(request.casting, request.cost_scale, request.magicka_max);
        if request.magicka_current < needed {
            return Err(CastRefusal::NotEnoughMagicka { needed, available: request.magicka_current });
        }
        self.active = Some(Active {
            hand: request.hand,
            spell: request.spell.clone(),
            casting: *request.casting,
            offhand: request.offhand.copied(),
            skill: request.skill,
            magicka_max: request.magicka_max,
            cost_scale: request.cost_scale,
        });
        self.phase = Phase::Winding(0.0);
        Ok(CastEvent::Started { hand: request.hand, spell_id: request.spell.id })
    }

    /// Advances by `dt` seconds. Fixed small steps keep it deterministic; a
    /// long step still transitions in order and reports every event.
    pub fn tick(&mut self, dt: f32, input: CastInput) -> Vec<CastEvent> {
        let mut events = Vec::new();
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let Some(active) = self.active.clone() else { return events };
        let timing = active.spell.timing;
        let mut budget = dt;

        loop {
            match self.phase {
                Phase::Idle => break,
                Phase::Winding(elapsed) => {
                    if input.dodge && elapsed <= timing.cancel_until {
                        self.finish_cast();
                        events.push(CastEvent::Cancelled);
                        break;
                    }
                    if input.poise_broken && !hyperarmour(timing, elapsed) {
                        self.finish_cast();
                        events.push(CastEvent::Interrupted);
                        break;
                    }
                    let until_release = timing.release_at - elapsed;
                    if budget < until_release {
                        self.phase = Phase::Winding(elapsed + budget);
                        break;
                    }
                    budget -= until_release;
                    // At the release frame: a charged spell keeps charging while the
                    // button is held; anything else releases now.
                    if active.spell.charge.is_some() && input.held {
                        self.phase = Phase::Charging(0.0);
                    } else {
                        self.release(&active, 0.0, input, &mut events);
                    }
                }
                Phase::Charging(held) => {
                    let charge = active.spell.charge.expect("charging implies a charged spell");
                    let at = timing.release_at + held;
                    if input.poise_broken && !hyperarmour(timing, at) {
                        self.finish_cast();
                        events.push(CastEvent::Interrupted);
                        break;
                    }
                    if !input.held {
                        self.release(&active, held, input, &mut events);
                        continue;
                    }
                    let room = charge.max_seconds - held;
                    if budget < room {
                        self.phase = Phase::Charging(held + budget);
                        break;
                    }
                    // Full charge releases: holding longer adds nothing.
                    budget -= room;
                    self.release(&active, charge.max_seconds, input, &mut events);
                }
                Phase::Recovering { since_release, charged } => {
                    let at = timing.release_at + charged + since_release;
                    if input.poise_broken && !hyperarmour(timing, at) {
                        self.finish_cast();
                        events.push(CastEvent::Interrupted);
                        break;
                    }
                    let left = timing.recovery - since_release;
                    if budget < left {
                        self.phase = Phase::Recovering { since_release: since_release + budget, charged };
                        break;
                    }
                    self.finish_cast();
                    events.push(CastEvent::Finished);
                    break;
                }
            }
        }
        events
    }

    fn release(&mut self, active: &Active, held: f32, input: CastInput, events: &mut Vec<CastEvent>) {
        let needed = active.spell.magicka_cost(&active.casting, active.cost_scale, active.magicka_max);
        let available = if input.magicka_current.is_finite() { input.magicka_current } else { 0.0 };
        if available < needed {
            self.finish_cast();
            events.push(CastEvent::Fizzled { needed, available });
            return;
        }
        self.spent_total += f64::from(needed);
        self.serial = self.serial.wrapping_add(1);
        let charge_power = active.spell.charge.map_or(1.0, |c| c.power(held));
        let charge_fraction = active.spell.charge.map_or(0.0, |c| c.fraction(held));
        events.push(CastEvent::Released(SpellSnapshot {
            release_id: (u64::from(self.id) << 32) | u64::from(self.serial),
            caster: u64::from(self.id),
            spell: active.spell.clone(),
            catalyst_id: active.casting.id,
            damage_multiplier: spell_damage_multiplier(
                &active.spell,
                &active.casting,
                active.offhand.as_ref(),
                active.skill,
                charge_power,
            ),
            charge_fraction,
        }));
        self.phase = Phase::Recovering { since_release: 0.0, charged: held };
    }

    fn finish_cast(&mut self) {
        self.phase = Phase::Idle;
        self.active = None;
    }
}

fn hyperarmour(timing: crate::magic::CastTiming, at: f32) -> bool {
    at >= timing.hyperarmour_from && at < timing.hyperarmour_to
}

// ---------------------------------------------------------------------------

/// Why an impact was refused. Each is a diagnostic, never damage permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImpactError {
    /// No such release: never registered, or already expired (stale).
    Unknown,
    /// This release already resolved against this target (this tick, for an area).
    Duplicate,
    /// An area tick outside the area's life.
    TickOutOfRange,
    /// A self-cast has no impact; its modifiers come from the snapshot.
    SelfCast,
}

/// Tuning input: how long a projectile may fly before its release is stale.
pub const PROJECTILE_MAX_FLIGHT_SECONDS: f32 = 10.0;
/// Tuning input: how long an instant spell's release may wait for its impact.
pub const INSTANT_RESOLVE_SECONDS: f32 = 0.5;
/// Releases tracked at once. A bounded table cannot be grown by a flood of releases.
pub const MAX_TRACKED_RELEASES: usize = 64;

#[derive(Clone, Debug)]
struct Entry {
    snapshot: SpellSnapshot,
    remaining: f32,
    resolved: Vec<(u64, u32)>,
}

#[derive(Clone, Debug, Default)]
pub struct ImpactLedger {
    entries: Vec<Entry>,
}

impl ImpactLedger {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Starts tracking a release. Returns false for a self-cast (nothing to
    /// resolve), a duplicate release id, or a full table.
    pub fn register(&mut self, snapshot: SpellSnapshot) -> bool {
        let remaining = match (snapshot.spell.delivery, snapshot.spell.area) {
            (SpellDelivery::SelfCast, _) => return false,
            (SpellDelivery::Instant, _) => INSTANT_RESOLVE_SECONDS,
            (SpellDelivery::Projectile, _) => PROJECTILE_MAX_FLIGHT_SECONDS,
            (SpellDelivery::Area, Some(area)) => area.duration,
            (SpellDelivery::Area, None) => return false,
        };
        if self.entries.len() >= MAX_TRACKED_RELEASES
            || self.entries.iter().any(|e| e.snapshot.release_id == snapshot.release_id)
        {
            return false;
        }
        self.entries.push(Entry { snapshot, remaining, resolved: Vec::new() });
        true
    }

    /// Resolves a native impact of `release_id` on `target` into the shared
    /// consequence event. `tick` is 0 for single-hit deliveries and the area
    /// tick index for an area. Each (target, tick) resolves once.
    pub fn resolve(&mut self, release_id: u64, target: u64, tick: u32) -> Result<HitEvent, ImpactError> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.snapshot.release_id == release_id)
            .ok_or(ImpactError::Unknown)?;
        let spell = &entry.snapshot.spell;
        let tick = match (spell.delivery, spell.area) {
            (SpellDelivery::Area, Some(area)) => {
                if tick >= area.ticks() {
                    return Err(ImpactError::TickOutOfRange);
                }
                tick
            }
            // A projectile resolves once: any other tick index is not an impact.
            _ => {
                if tick != 0 {
                    return Err(ImpactError::TickOutOfRange);
                }
                0
            }
        };
        if entry.resolved.contains(&(target, tick)) {
            return Err(ImpactError::Duplicate);
        }
        entry.resolved.push((target, tick));
        Ok(entry.snapshot.hit_event(target, tick + 1))
    }

    /// Ages every release and drops those that ran out: a miss or a world
    /// impact never reaches `resolve`, so it expires without consequence.
    pub fn tick(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        for entry in &mut self.entries {
            entry.remaining -= dt;
        }
        self.entries.retain(|e| e.remaining > 0.0);
    }
}
