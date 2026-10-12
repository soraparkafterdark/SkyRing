//! Magic composed with the shared effect pipeline, host-side and deterministic.
//!
//! `MagicWorld` is what the bridge's spell path talks to: it owns each actor's
//! caster and consequence state, tracks releases in flight, and turns a native
//! impact into the same `HitOutcome` melee and arrows produce. It is where the
//! "one effect pipeline" rule becomes observable:
//!
//! - a debuff a spell applies sits in the target's modifier set and amplifies
//!   the next hit from any delivery, until it expires;
//! - a buff sits in the caster's modifier set: `MagickaCost` makes the next cast
//!   cheaper, `CastSpeed` makes it faster, `DamageDealt` raises its power;
//! - status buildup accumulates, procs, locks out and decays exactly as it does
//!   for a frost sword;
//! - a dead, unloaded or respawned actor keeps nothing from its earlier life.
//!
//! Skyrim keeps health, magicka, projectile flight and impact. This layer only
//! multiplies, schedules and correlates.

use std::collections::BTreeMap;

use crate::cast::{CastEvent, CastInput, CastRefusal, CastRequest, Caster, ImpactError, ImpactLedger, Registration};
use crate::consequences::{ConsequenceState, HitOutcome, TargetRules};
use crate::effects::{ModifiedStat, STATUS_COUNT};
use crate::magic::{Catalyst, Hand, SpellDefinition, MAX_SPELL_DAMAGE_MULTIPLIER};
use crate::posture::{PoiseResult, StanceResult};

/// Cast speed outside this range is a data error, not a build.
const MIN_CAST_SPEED: f32 = 0.25;
const MAX_CAST_SPEED: f32 = 4.0;
/// A cost modifier below this is clamped: a buff may cheapen casting, not free it.
const MIN_COST_SCALE: f32 = 0.1;

/// Everything about a cast the bridge knows when the player or an NPC starts it.
#[derive(Clone, Debug)]
pub struct CastSpec {
    pub hand: Hand,
    pub casting: Catalyst,
    pub offhand: Option<Catalyst>,
    pub spell: SpellDefinition,
    /// The governing school's Skyrim skill, 0 to 100.
    pub skill: f32,
    pub magicka_current: f32,
    pub magicka_max: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldError {
    UnknownActor,
    ActorDead,
    ActorExists,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CastStart {
    Started,
    Refused(CastRefusal),
    Failed(WorldError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImpactRefusal {
    Ledger(ImpactError),
    UnknownTarget,
    TargetDead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastEnd {
    Finished,
    Cancelled,
    Interrupted,
    Fizzled,
}

/// The deterministic record of what happened. No wall clock, no pointers.
#[derive(Clone, Debug, PartialEq)]
pub enum MagicEvent {
    ActorSpawned { tick: u64, actor: u32 },
    ActorDied { tick: u64, actor: u32 },
    ActorRespawned { tick: u64, actor: u32 },
    ActorUnloaded { tick: u64, actor: u32 },
    CastStarted { tick: u64, actor: u32, spell: u32 },
    CastRefused { tick: u64, actor: u32, spell: u32, reason: CastRefusal },
    SpellReleased { tick: u64, actor: u32, release_id: u64, spell: u32, damage_multiplier: f32, charge_fraction: f32 },
    MagickaSpent { tick: u64, actor: u32, amount: f32 },
    /// The ledger refused to track a release (a duplicate id or a full table), so no impact
    /// can ever resolve it. Loud on purpose: the bridge must not wait for an impact.
    ReleaseUntracked { tick: u64, actor: u32, release_id: u64, reason: Registration },
    SelfEffectsApplied { tick: u64, actor: u32, applied: u32 },
    CastEnded { tick: u64, actor: u32, how: CastEnd },
    ImpactResolved {
        tick: u64,
        release_id: u64,
        target: u32,
        damage_multiplier: f32,
        status_procs: u8,
        poise_interrupted: bool,
        stance_broken: bool,
        modifiers_applied: u32,
    },
    ImpactRefused { tick: u64, release_id: u64, target: u32, reason: ImpactRefusal },
}

struct Entry {
    caster: Caster,
    state: ConsequenceState,
    rules: TargetRules,
    alive: bool,
    /// Poise broke since the last step; the caster sees it on the next one.
    interrupt: bool,
    /// Magicka not yet reported to Skyrim.
    owed: f32,
    /// The last pool Skyrim reported. An actor with no input this tick keeps it:
    /// the world is never told the pool is empty unless Skyrim says so.
    pool: f32,
}

pub struct MagicWorld {
    tick: u64,
    actors: BTreeMap<u32, Entry>,
    /// Release numbering survives unloads, so ids stay unique while projectiles fly.
    serials: BTreeMap<u32, u32>,
    ledger: ImpactLedger,
    events: Vec<MagicEvent>,
}

impl Default for MagicWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl MagicWorld {
    pub fn new() -> Self {
        Self { tick: 0, actors: BTreeMap::new(), serials: BTreeMap::new(), ledger: ImpactLedger::default(), events: Vec::new() }
    }

    pub fn events(&self) -> &[MagicEvent] {
        &self.events
    }

    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    pub fn releases_in_flight(&self) -> usize {
        self.ledger.len()
    }

    pub fn state(&self, actor: u32) -> Option<&ConsequenceState> {
        self.actors.get(&actor).map(|e| &e.state)
    }

    pub fn is_alive(&self, actor: u32) -> bool {
        self.actors.get(&actor).map_or(false, |e| e.alive)
    }

    pub fn dedicated_hand(&self, actor: u32) -> Option<Hand> {
        self.actors.get(&actor).and_then(|e| e.caster.dedicated_hand())
    }

    pub fn dodge_allowed(&self, actor: u32) -> bool {
        self.actors.get(&actor).map_or(true, |e| e.caster.dodge_allowed())
    }

    /// Magicka spent since the last call, in the host's units, for Skyrim to take.
    pub fn take_magicka_spent(&mut self, actor: u32) -> f32 {
        self.actors.get_mut(&actor).map_or(0.0, |e| std::mem::take(&mut e.owed))
    }

    pub fn spawn(&mut self, actor: u32, rules: TargetRules) -> Result<(), WorldError> {
        if self.actors.contains_key(&actor) {
            return Err(WorldError::ActorExists);
        }
        let serial = self.serials.get(&actor).copied().unwrap_or(0);
        self.actors.insert(
            actor,
            Entry { caster: Caster::resume(actor, serial), state: ConsequenceState::default(), rules, alive: true, interrupt: false, owed: 0.0, pool: 0.0 },
        );
        self.events.push(MagicEvent::ActorSpawned { tick: self.tick, actor });
        Ok(())
    }

    /// The actor is gone from the loaded world. Nothing of its state survives, but
    /// its release numbering does, and releases already in flight still resolve.
    pub fn unload(&mut self, actor: u32) -> Result<(), WorldError> {
        let entry = self.actors.remove(&actor).ok_or(WorldError::UnknownActor)?;
        self.serials.insert(actor, entry.caster.serial());
        self.events.push(MagicEvent::ActorUnloaded { tick: self.tick, actor });
        Ok(())
    }

    /// Death ends the cast and clears every status, modifier, poise and stance
    /// meter. A dead actor is not a target and cannot cast.
    pub fn kill(&mut self, actor: u32) -> Result<(), WorldError> {
        let entry = self.actors.get_mut(&actor).ok_or(WorldError::UnknownActor)?;
        entry.caster.cancel();
        entry.state = ConsequenceState::default();
        entry.alive = false;
        entry.interrupt = false;
        self.events.push(MagicEvent::ActorDied { tick: self.tick, actor });
        Ok(())
    }

    /// A fresh life: nothing carries over from before the death.
    pub fn respawn(&mut self, actor: u32) -> Result<(), WorldError> {
        let entry = self.actors.get_mut(&actor).ok_or(WorldError::UnknownActor)?;
        entry.caster.cancel();
        entry.state = ConsequenceState::default();
        entry.alive = true;
        entry.interrupt = false;
        self.events.push(MagicEvent::ActorRespawned { tick: self.tick, actor });
        Ok(())
    }

    pub fn begin_cast(&mut self, actor: u32, spec: &CastSpec) -> CastStart {
        let Some(entry) = self.actors.get_mut(&actor) else { return CastStart::Failed(WorldError::UnknownActor) };
        if !entry.alive {
            return CastStart::Failed(WorldError::ActorDead);
        }
        let cost_scale = entry.state.modifiers.evaluate(ModifiedStat::MagickaCost, 1.0);
        let cost_scale = if cost_scale.is_finite() { cost_scale.max(MIN_COST_SCALE) } else { 1.0 };
        let request = CastRequest {
            hand: spec.hand,
            casting: &spec.casting,
            offhand: spec.offhand.as_ref(),
            spell: &spec.spell,
            skill: spec.skill,
            magicka_current: spec.magicka_current,
            magicka_max: spec.magicka_max,
            cost_scale,
        };
        match entry.caster.begin(request) {
            Ok(_) => {
                entry.pool = spec.magicka_current;
                self.events.push(MagicEvent::CastStarted { tick: self.tick, actor, spell: spec.spell.id });
                CastStart::Started
            }
            Err(reason) => {
                self.events.push(MagicEvent::CastRefused { tick: self.tick, actor, spell: spec.spell.id, reason });
                CastStart::Refused(reason)
            }
        }
    }

    /// Advances every actor by `dt`. `inputs` carries each caster's controls this
    /// tick; an actor with none is idle-handed with its last reported magicka. Order is by actor id,
    /// so the result never depends on hash or insertion order.
    pub fn step(&mut self, dt: f32, inputs: &BTreeMap<u32, CastInput>) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        self.tick += 1;
        let tick = self.tick;
        let ids: Vec<u32> = self.actors.keys().copied().collect();
        for actor in ids {
            let mut released = Vec::new();
            let mut ended = Vec::new();
            let mut self_effects = 0;
            {
                let entry = self.actors.get_mut(&actor).expect("id came from the map");
                let mut input = match inputs.get(&actor) {
                    Some(given) => {
                        entry.pool = given.magicka_current;
                        *given
                    }
                    None => CastInput { magicka_current: entry.pool, ..Default::default() },
                };
                input.poise_broken |= std::mem::take(&mut entry.interrupt);
                // Cast speed is a modifier like any other: faster actors simply
                // advance their cast faster.
                let speed = entry.state.modifiers.evaluate(ModifiedStat::CastSpeed, 1.0);
                let speed = if speed.is_finite() { speed.clamp(MIN_CAST_SPEED, MAX_CAST_SPEED) } else { 1.0 };
                for event in entry.caster.tick(dt * speed, input) {
                    match event {
                        CastEvent::Released(mut snapshot) => {
                            // A buff on the caster raises the power of what it casts; the
                            // snapshot then stays fixed, like every other release.
                            let dealt = entry.state.modifiers.evaluate(ModifiedStat::DamageDealt, 1.0);
                            if dealt.is_finite() {
                                snapshot.damage_multiplier =
                                    (snapshot.damage_multiplier * dealt.max(0.0)).clamp(0.0, MAX_SPELL_DAMAGE_MULTIPLIER);
                            }
                            for modifier in snapshot.caster_modifiers() {
                                if entry.state.modifiers.apply(modifier) {
                                    self_effects += 1;
                                }
                            }
                            released.push(snapshot);
                        }
                        CastEvent::Fizzled { .. } => ended.push(CastEnd::Fizzled),
                        CastEvent::Cancelled => ended.push(CastEnd::Cancelled),
                        CastEvent::Interrupted => ended.push(CastEnd::Interrupted),
                        CastEvent::Finished => ended.push(CastEnd::Finished),
                        CastEvent::Started { .. } => {}
                    }
                }
                let spent = entry.caster.take_spent();
                entry.owed += spent;
                if spent > 0.0 {
                    self.events.push(MagicEvent::MagickaSpent { tick, actor, amount: spent });
                }
            }
            for snapshot in released {
                self.events.push(MagicEvent::SpellReleased {
                    tick,
                    actor,
                    release_id: snapshot.release_id,
                    spell: snapshot.spell.id,
                    damage_multiplier: snapshot.damage_multiplier,
                    charge_fraction: snapshot.charge_fraction,
                });
                let release_id = snapshot.release_id;
                match self.ledger.try_register(snapshot) {
                    Registration::Tracked | Registration::SelfCast => {}
                    reason => self.events.push(MagicEvent::ReleaseUntracked { tick, actor, release_id, reason }),
                }
            }
            if self_effects > 0 {
                self.events.push(MagicEvent::SelfEffectsApplied { tick, actor, applied: self_effects });
            }
            for how in ended {
                self.events.push(MagicEvent::CastEnded { tick, actor, how });
            }
        }
        // Meters, modifiers and releases age with the world, never with a caster.
        for entry in self.actors.values_mut() {
            let rules = entry.rules;
            entry.state.tick(dt, rules);
        }
        self.ledger.tick(dt);
    }

    /// Skyrim reports that release `release_id` natively struck `target` (for an area,
    /// on tick `tick`). The result is the same `HitOutcome` any delivery produces.
    /// A dead or unknown target is refused *before* the release is consumed.
    pub fn native_impact(&mut self, release_id: u64, target: u32, tick: u32) -> Result<HitOutcome, ImpactRefusal> {
        let now = self.tick;
        let refuse = |world: &mut Self, reason: ImpactRefusal| {
            world.events.push(MagicEvent::ImpactRefused { tick: now, release_id, target, reason });
            Err(reason)
        };
        match self.actors.get(&target) {
            None => return refuse(self, ImpactRefusal::UnknownTarget),
            Some(entry) if !entry.alive => return refuse(self, ImpactRefusal::TargetDead),
            Some(_) => {}
        }
        let event = match self.ledger.resolve(release_id, u64::from(target), tick) {
            Ok(event) => event,
            Err(error) => return refuse(self, ImpactRefusal::Ledger(error)),
        };
        let entry = self.actors.get_mut(&target).expect("checked above");
        let rules = entry.rules;
        let outcome = entry.state.resolve(&event, rules);
        if matches!(outcome.poise, PoiseResult::Interrupted { .. }) {
            entry.interrupt = true;
        }
        let procs = outcome
            .status_procs
            .iter()
            .enumerate()
            .take(STATUS_COUNT)
            .fold(0u8, |mask, (i, proc)| if proc.is_some() { mask | (1 << i) } else { mask });
        self.events.push(MagicEvent::ImpactResolved {
            tick: now,
            release_id,
            target,
            damage_multiplier: outcome.skyrim_damage_multiplier,
            status_procs: procs,
            poise_interrupted: matches!(outcome.poise, PoiseResult::Interrupted { .. }),
            stance_broken: matches!(outcome.stance, StanceResult::Broken { .. }),
            modifiers_applied: outcome.modifiers_applied as u32,
        });
        Ok(outcome)
    }
}
