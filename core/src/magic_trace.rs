//! Record and replay for `MagicWorld`, and a canonical encoding of its event log.
//!
//! "Runtime logs compare to headless replay without pointers or wall clock" is a
//! property, not a hope: `MagicRecorder` wraps a world and records every call that
//! can change it, `MagicScript::replay` re-runs those calls on a fresh world, and
//! `canonical_event_bytes` gives both logs a stable byte form to compare.
//!
//! Scripts are in-memory. They carry spell definitions, which have no stable wire
//! form yet (that arrives with the IR loader), so this proves determinism and gives
//! the bridge a log to compare, but does not yet persist a session to disk.

use std::collections::BTreeMap;

use crate::cast::{CastInput, CastRefusal, ImpactError};
use crate::consequences::{HitOutcome, TargetRules};
use crate::magic_world::{CastEnd, CastSpec, CastStart, ImpactRefusal, MagicEvent, MagicWorld, WorldError};

/// One call that can change a world. Replaying them in order reproduces it exactly.
#[derive(Clone, Debug)]
pub enum ScriptStep {
    Spawn { actor: u32, rules: TargetRules },
    Unload { actor: u32 },
    Kill { actor: u32 },
    Respawn { actor: u32 },
    BeginCast { actor: u32, spec: Box<CastSpec> },
    Step { dt: f32, inputs: BTreeMap<u32, CastInput> },
    Impact { release_id: u64, target: u32, tick: u32 },
}

#[derive(Clone, Debug, Default)]
pub struct MagicScript {
    steps: Vec<ScriptStep>,
}

impl MagicScript {
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn steps(&self) -> &[ScriptStep] {
        &self.steps
    }

    /// Re-runs every step on a fresh world and returns its event log. Refused calls
    /// are replayed too, so a refusal in the live run is a refusal in the replay.
    pub fn replay(&self) -> Vec<MagicEvent> {
        let mut world = MagicWorld::new();
        for step in &self.steps {
            apply(&mut world, step);
        }
        world.events().to_vec()
    }
}

fn apply(world: &mut MagicWorld, step: &ScriptStep) {
    match step {
        ScriptStep::Spawn { actor, rules } => drop(world.spawn(*actor, *rules)),
        ScriptStep::Unload { actor } => drop(world.unload(*actor)),
        ScriptStep::Kill { actor } => drop(world.kill(*actor)),
        ScriptStep::Respawn { actor } => drop(world.respawn(*actor)),
        ScriptStep::BeginCast { actor, spec } => drop(world.begin_cast(*actor, spec)),
        ScriptStep::Step { dt, inputs } => world.step(*dt, inputs),
        ScriptStep::Impact { release_id, target, tick } => drop(world.native_impact(*release_id, *target, *tick)),
    }
}

/// A `MagicWorld` that records what it is asked to do.
pub struct MagicRecorder {
    world: MagicWorld,
    script: MagicScript,
}

impl Default for MagicRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl MagicRecorder {
    pub fn new() -> Self {
        Self { world: MagicWorld::new(), script: MagicScript::default() }
    }

    /// Read access to the live world. Mutation goes through the recorder.
    pub fn world(&self) -> &MagicWorld {
        &self.world
    }

    pub fn take_magicka_spent(&mut self, actor: u32) -> f32 {
        // Reporting spend does not change what happens next, so it is not a step.
        self.world.take_magicka_spent(actor)
    }

    pub fn spawn(&mut self, actor: u32, rules: TargetRules) -> Result<(), WorldError> {
        self.script.steps.push(ScriptStep::Spawn { actor, rules });
        self.world.spawn(actor, rules)
    }

    pub fn unload(&mut self, actor: u32) -> Result<(), WorldError> {
        self.script.steps.push(ScriptStep::Unload { actor });
        self.world.unload(actor)
    }

    pub fn kill(&mut self, actor: u32) -> Result<(), WorldError> {
        self.script.steps.push(ScriptStep::Kill { actor });
        self.world.kill(actor)
    }

    pub fn respawn(&mut self, actor: u32) -> Result<(), WorldError> {
        self.script.steps.push(ScriptStep::Respawn { actor });
        self.world.respawn(actor)
    }

    pub fn begin_cast(&mut self, actor: u32, spec: &CastSpec) -> CastStart {
        self.script.steps.push(ScriptStep::BeginCast { actor, spec: Box::new(spec.clone()) });
        self.world.begin_cast(actor, spec)
    }

    pub fn step(&mut self, dt: f32, inputs: &BTreeMap<u32, CastInput>) {
        self.script.steps.push(ScriptStep::Step { dt, inputs: inputs.clone() });
        self.world.step(dt, inputs);
    }

    pub fn native_impact(&mut self, release_id: u64, target: u32, tick: u32) -> Result<HitOutcome, ImpactRefusal> {
        self.script.steps.push(ScriptStep::Impact { release_id, target, tick });
        self.world.native_impact(release_id, target, tick)
    }

    pub fn finish(self) -> (MagicScript, Vec<MagicEvent>) {
        let events = self.world.events().to_vec();
        (self.script, events)
    }
}

// --- canonical encoding ---------------------------------------------------------------------

const MAGIC: &[u8; 8] = b"SRMAGEV\0";
const EVENT_LOG_VERSION: u16 = 1;

/// A stable byte form of an event log: little-endian, f32 as raw bits, one tag byte per
/// variant. Two logs are the same iff their bytes are the same. Refusal reasons are encoded
/// by name so a refusal that changes its reason changes the bytes.
pub fn canonical_event_bytes(events: &[MagicEvent]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + events.len() * 24);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&EVENT_LOG_VERSION.to_le_bytes());
    out.extend_from_slice(&(events.len() as u64).to_le_bytes());
    for event in events {
        encode(event, &mut out);
    }
    out
}

fn u8_(out: &mut Vec<u8>, v: u8) {
    out.push(v);
}
fn u32_(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn u64_(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn f32_(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_bits().to_le_bytes());
}

fn refusal_tag(reason: CastRefusal, out: &mut Vec<u8>) {
    match reason {
        CastRefusal::Busy => u8_(out, 0),
        CastRefusal::InvalidCatalyst => u8_(out, 1),
        CastRefusal::InvalidSpell(error) => {
            u8_(out, 2);
            u8_(out, error as u8);
        }
        CastRefusal::WrongCatalystFamily => u8_(out, 3),
        CastRefusal::InvalidPool => u8_(out, 4),
        CastRefusal::NotEnoughMagicka { needed, available } => {
            u8_(out, 5);
            f32_(out, needed);
            f32_(out, available);
        }
    }
}

fn impact_refusal_tag(reason: ImpactRefusal, out: &mut Vec<u8>) {
    match reason {
        ImpactRefusal::UnknownTarget => u8_(out, 0),
        ImpactRefusal::TargetDead => u8_(out, 1),
        ImpactRefusal::Ledger(error) => {
            u8_(out, 2);
            u8_(
                out,
                match error {
                    ImpactError::Unknown => 0,
                    ImpactError::Duplicate => 1,
                    ImpactError::TickOutOfRange => 2,
                    ImpactError::SelfCast => 3,
                },
            );
        }
    }
}

fn encode(event: &MagicEvent, out: &mut Vec<u8>) {
    match event {
        MagicEvent::ActorSpawned { tick, actor } => {
            u8_(out, 1);
            u64_(out, *tick);
            u32_(out, *actor);
        }
        MagicEvent::ActorDied { tick, actor } => {
            u8_(out, 2);
            u64_(out, *tick);
            u32_(out, *actor);
        }
        MagicEvent::ActorRespawned { tick, actor } => {
            u8_(out, 3);
            u64_(out, *tick);
            u32_(out, *actor);
        }
        MagicEvent::ActorUnloaded { tick, actor } => {
            u8_(out, 4);
            u64_(out, *tick);
            u32_(out, *actor);
        }
        MagicEvent::CastStarted { tick, actor, spell } => {
            u8_(out, 5);
            u64_(out, *tick);
            u32_(out, *actor);
            u32_(out, *spell);
        }
        MagicEvent::CastRefused { tick, actor, spell, reason } => {
            u8_(out, 6);
            u64_(out, *tick);
            u32_(out, *actor);
            u32_(out, *spell);
            refusal_tag(*reason, out);
        }
        MagicEvent::SpellReleased { tick, actor, release_id, spell, damage_multiplier, charge_fraction } => {
            u8_(out, 7);
            u64_(out, *tick);
            u32_(out, *actor);
            u64_(out, *release_id);
            u32_(out, *spell);
            f32_(out, *damage_multiplier);
            f32_(out, *charge_fraction);
        }
        MagicEvent::MagickaSpent { tick, actor, amount } => {
            u8_(out, 8);
            u64_(out, *tick);
            u32_(out, *actor);
            f32_(out, *amount);
        }
        MagicEvent::SelfEffectsApplied { tick, actor, applied } => {
            u8_(out, 9);
            u64_(out, *tick);
            u32_(out, *actor);
            u32_(out, *applied);
        }
        MagicEvent::CastEnded { tick, actor, how } => {
            u8_(out, 10);
            u64_(out, *tick);
            u32_(out, *actor);
            u8_(
                out,
                match how {
                    CastEnd::Finished => 0,
                    CastEnd::Cancelled => 1,
                    CastEnd::Interrupted => 2,
                    CastEnd::Fizzled => 3,
                },
            );
        }
        MagicEvent::ImpactResolved { tick, release_id, target, damage_multiplier, status_procs, poise_interrupted, stance_broken, modifiers_applied } => {
            u8_(out, 11);
            u64_(out, *tick);
            u64_(out, *release_id);
            u32_(out, *target);
            f32_(out, *damage_multiplier);
            u8_(out, *status_procs);
            u8_(out, u8::from(*poise_interrupted));
            u8_(out, u8::from(*stance_broken));
            u32_(out, *modifiers_applied);
        }
        MagicEvent::ImpactRefused { tick, release_id, target, reason } => {
            u8_(out, 12);
            u64_(out, *tick);
            u64_(out, *release_id);
            u32_(out, *target);
            impact_refusal_tag(*reason, out);
        }
    }
}
