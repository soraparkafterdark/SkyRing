//! Deterministic recording and replay for host-side combat evidence.
//!
//! A trace is a sequence of fixed 60 Hz input and Skyrim-world observations.
//! It contains no wall-clock timestamps, pointers, or presentation state that
//! can feed back into combat. Replaying a trace produces a canonical event
//! stream with stable little-endian encoding.

use std::fmt;

use crate::ffi::{ErAction, ErCombat, ErHitResult, ErHurt, ErInput, ErOutput, ErVec3, ErWorld};

const TRACE_MAGIC: &[u8; 8] = b"SKYTRC01";
const EVENT_MAGIC: &[u8; 8] = b"SKYEVT01";
const TRACE_VERSION: u16 = 2;
const EVENT_VERSION: u16 = 2;
const FIXED_DELTA_SECONDS: f32 = 1.0 / 60.0;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perspective {
    FirstPerson = 1,
    ThirdPerson = 3,
}

#[derive(Clone, Copy, Debug)]
pub struct IncomingContact {
    /// Stable bridge-owned identity; never a pointer or runtime address.
    pub source_id: u64,
    pub from: ErVec3,
    pub low: bool,
    pub hurt: ErHurt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsequenceObservation {
    pub delivery_id: u64,
    /// Stable bridge-defined outcome code. The trace does not interpret it.
    pub outcome: u32,
}

#[derive(Clone, Debug)]
pub struct TraceTick {
    pub input: ErInput,
    pub world: ErWorld,
    pub incoming: Vec<IncomingContact>,
    pub confirm_live_contact: bool,
    pub consequences: Vec<ConsequenceObservation>,
    /// Presentation-only evidence. Replay logs it but never passes it to the
    /// combat simulation.
    pub perspective: Option<Perspective>,
}

impl TraceTick {
    pub fn new(input: ErInput, world: ErWorld) -> Self {
        Self {
            input,
            world,
            incoming: Vec::new(),
            confirm_live_contact: false,
            consequences: Vec::new(),
            perspective: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CombatTrace {
    initial_position: ErVec3,
    ticks: Vec<TraceTick>,
}

impl CombatTrace {
    pub fn initial_position(&self) -> ErVec3 {
        self.initial_position
    }

    pub fn ticks(&self) -> &[TraceTick] {
        &self.ticks
    }

    /// Canonical bytes for the recorded input/world facts themselves.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(TRACE_MAGIC);
        put_u16(&mut bytes, TRACE_VERSION);
        put_vec3(&mut bytes, self.initial_position);
        put_u64(&mut bytes, self.ticks.len() as u64);
        for (tick, frame) in self.ticks.iter().enumerate() {
            put_u64(&mut bytes, tick as u64);
            put_input(&mut bytes, &frame.input);
            put_world(&mut bytes, &frame.world);
            put_u32(&mut bytes, frame.incoming.len() as u32);
            for incoming in &frame.incoming {
                put_u64(&mut bytes, incoming.source_id);
                put_vec3(&mut bytes, incoming.from);
                put_u8(&mut bytes, incoming.low as u8);
                put_u32(&mut bytes, incoming.hurt as u32);
            }
            put_u8(&mut bytes, frame.confirm_live_contact as u8);
            put_u32(&mut bytes, frame.consequences.len() as u32);
            for consequence in &frame.consequences {
                put_u64(&mut bytes, consequence.delivery_id);
                put_u32(&mut bytes, consequence.outcome);
            }
            put_u8(
                &mut bytes,
                frame.perspective.map_or(0, |perspective| perspective as u8),
            );
        }
        bytes
    }

    /// Decode a canonical trace and re-run every structural validation used
    /// by live recording. Unknown versions and trailing bytes fail closed.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, TraceError> {
        let mut reader = ByteReader::new(bytes);
        if reader.take(TRACE_MAGIC.len())? != TRACE_MAGIC {
            return Err(reader.invalid("invalid trace magic"));
        }
        let version = reader.u16()?;
        if version != TRACE_VERSION {
            return Err(TraceError::UnsupportedVersion { version });
        }
        let initial_position = reader.vec3()?;
        let tick_count = reader.usize_from_u64("trace tick count does not fit this platform")?;
        // Every tick has at least 80 encoded bytes before variable-length
        // incoming/consequence observations. Reject impossible allocation
        // sizes before reserving memory.
        if tick_count > reader.remaining() / 80 {
            return Err(reader.invalid("trace tick count exceeds remaining bytes"));
        }

        let mut recorder = TraceRecorder::new(initial_position)?;
        for expected_tick in 0..tick_count {
            let encoded_tick = reader.u64()?;
            if encoded_tick != expected_tick as u64 {
                return Err(reader.invalid("trace ticks must be contiguous and start at zero"));
            }
            let input = reader.input()?;
            let world = reader.world()?;
            let mut frame = TraceTick::new(input, world);

            let incoming_count = reader.usize_from_u32()?;
            if incoming_count > reader.remaining() / 25 {
                return Err(reader.invalid("incoming count exceeds remaining bytes"));
            }
            frame.incoming.reserve(incoming_count);
            for _ in 0..incoming_count {
                frame.incoming.push(IncomingContact {
                    source_id: reader.u64()?,
                    from: reader.vec3()?,
                    low: reader.boolean("incoming.low")?,
                    hurt: reader.hurt()?,
                });
            }
            frame.confirm_live_contact = reader.boolean("confirm_live_contact")?;

            let consequence_count = reader.usize_from_u32()?;
            if consequence_count > reader.remaining() / 12 {
                return Err(reader.invalid("consequence count exceeds remaining bytes"));
            }
            frame.consequences.reserve(consequence_count);
            for _ in 0..consequence_count {
                frame.consequences.push(ConsequenceObservation {
                    delivery_id: reader.u64()?,
                    outcome: reader.u32()?,
                });
            }
            frame.perspective = match reader.u8()? {
                0 => None,
                1 => Some(Perspective::FirstPerson),
                3 => Some(Perspective::ThirdPerson),
                _ => return Err(reader.invalid("invalid perspective value")),
            };
            recorder.record(frame)?;
        }
        if reader.remaining() != 0 {
            return Err(reader.invalid("trailing bytes after trace"));
        }
        Ok(recorder.finish())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceError {
    NonFinite {
        tick: u64,
        field: &'static str,
    },
    InvalidFlag {
        tick: u64,
        field: &'static str,
        value: u8,
    },
    ContactConfirmedOutsideLiveWindow {
        tick: u64,
    },
    InvalidEncoding {
        offset: usize,
        reason: &'static str,
    },
    UnsupportedVersion {
        version: u16,
    },
}

impl fmt::Display for TraceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { tick, field } => {
                write!(formatter, "trace tick {tick} has non-finite {field}")
            }
            Self::InvalidFlag { tick, field, value } => {
                write!(
                    formatter,
                    "trace tick {tick} has invalid {field} flag {value}"
                )
            }
            Self::ContactConfirmedOutsideLiveWindow { tick } => {
                write!(
                    formatter,
                    "trace tick {tick} confirms a contact outside a live hit window"
                )
            }
            Self::InvalidEncoding { offset, reason } => {
                write!(
                    formatter,
                    "invalid trace encoding at byte {offset}: {reason}"
                )
            }
            Self::UnsupportedVersion { version } => {
                write!(formatter, "unsupported trace version {version}")
            }
        }
    }
}

impl std::error::Error for TraceError {}

/// Records validated input/world facts without running combat logic.
///
/// Keeping recording separate from replay makes the boundary usable by the
/// SKSE adapter as well as by host tests: the bridge records what Skyrim said,
/// while the replay service owns deterministic simulation and event emission.
#[derive(Debug)]
pub struct TraceRecorder {
    initial_position: ErVec3,
    ticks: Vec<TraceTick>,
}

impl TraceRecorder {
    pub fn new(initial_position: ErVec3) -> Result<Self, TraceError> {
        validate_vec3(0, "initial_position", initial_position)?;
        Ok(Self {
            initial_position,
            ticks: Vec::new(),
        })
    }

    pub fn record(&mut self, frame: TraceTick) -> Result<(), TraceError> {
        validate_frame(self.ticks.len() as u64, &frame)?;
        self.ticks.push(frame);
        Ok(())
    }

    pub fn finish(self) -> CombatTrace {
        CombatTrace {
            initial_position: self.initial_position,
            ticks: self.ticks,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CombatEvent {
    InputAccepted {
        tick: u64,
        input_id: u64,
    },
    WorldObserved {
        tick: u64,
        actual_position: ErVec3,
        ground_z: f32,
        has_target: bool,
        target: ErVec3,
        in_combat: bool,
        stamina_current: f32,
        stamina_max: f32,
    },
    ActionStarted {
        tick: u64,
        action_serial: u32,
        action_kind: ErAction,
        action_id: [u8; 32],
    },
    MotionRequested {
        tick: u64,
        action_serial: u32,
        desired_position: ErVec3,
        desired_heading: f32,
    },
    MotionResolved {
        tick: u64,
        action_serial: u32,
        actual_position: ErVec3,
    },
    ContactObserved {
        tick: u64,
        delivery_id: u64,
        action_serial: u32,
        hit_id: u32,
    },
    ContactConfirmed {
        tick: u64,
        delivery_id: u64,
    },
    IncomingResolved {
        tick: u64,
        source_id: u64,
        result: ErHitResult,
    },
    StaminaChanged {
        tick: u64,
        before: f32,
        after: f32,
    },
    /// Stamina the core spent during this tick's update, in the host's units, for Skyrim to
    /// take from its own pool.
    StaminaSpent {
        tick: u64,
        amount: f32,
    },
    ConsequenceResolved {
        tick: u64,
        delivery_id: u64,
        outcome: u32,
    },
    HitStopChanged {
        tick: u64,
        action_serial: u32,
        remaining_seconds: f32,
    },
    PerspectiveObserved {
        tick: u64,
        perspective: Perspective,
    },
}

impl CombatEvent {
    fn is_presentation_only(&self) -> bool {
        matches!(self, Self::PerspectiveObserved { .. })
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            Self::InputAccepted { tick, input_id } => {
                put_u8(bytes, 1);
                put_u64(bytes, *tick);
                put_u64(bytes, *input_id);
            }
            Self::WorldObserved {
                tick,
                actual_position,
                ground_z,
                has_target,
                target,
                in_combat,
                stamina_current,
                stamina_max,
            } => {
                put_u8(bytes, 2);
                put_u64(bytes, *tick);
                put_vec3(bytes, *actual_position);
                put_f32(bytes, *ground_z);
                put_u8(bytes, *has_target as u8);
                put_vec3(bytes, *target);
                put_u8(bytes, *in_combat as u8);
                put_f32(bytes, *stamina_current);
                put_f32(bytes, *stamina_max);
            }
            Self::ActionStarted {
                tick,
                action_serial,
                action_kind,
                action_id,
            } => {
                put_u8(bytes, 3);
                put_u64(bytes, *tick);
                put_u32(bytes, *action_serial);
                put_u32(bytes, *action_kind as u32);
                bytes.extend_from_slice(action_id);
            }
            Self::MotionRequested {
                tick,
                action_serial,
                desired_position,
                desired_heading,
            } => {
                put_u8(bytes, 4);
                put_u64(bytes, *tick);
                put_u32(bytes, *action_serial);
                put_vec3(bytes, *desired_position);
                put_f32(bytes, *desired_heading);
            }
            Self::MotionResolved {
                tick,
                action_serial,
                actual_position,
            } => {
                put_u8(bytes, 5);
                put_u64(bytes, *tick);
                put_u32(bytes, *action_serial);
                put_vec3(bytes, *actual_position);
            }
            Self::ContactObserved {
                tick,
                delivery_id,
                action_serial,
                hit_id,
            } => {
                put_u8(bytes, 6);
                put_u64(bytes, *tick);
                put_u64(bytes, *delivery_id);
                put_u32(bytes, *action_serial);
                put_u32(bytes, *hit_id);
            }
            Self::ContactConfirmed { tick, delivery_id } => {
                put_u8(bytes, 7);
                put_u64(bytes, *tick);
                put_u64(bytes, *delivery_id);
            }
            Self::IncomingResolved {
                tick,
                source_id,
                result,
            } => {
                put_u8(bytes, 8);
                put_u64(bytes, *tick);
                put_u64(bytes, *source_id);
                put_u32(bytes, *result as u32);
            }
            Self::StaminaChanged {
                tick,
                before,
                after,
            } => {
                put_u8(bytes, 9);
                put_u64(bytes, *tick);
                put_f32(bytes, *before);
                put_f32(bytes, *after);
            }
            Self::ConsequenceResolved {
                tick,
                delivery_id,
                outcome,
            } => {
                put_u8(bytes, 10);
                put_u64(bytes, *tick);
                put_u64(bytes, *delivery_id);
                put_u32(bytes, *outcome);
            }
            Self::HitStopChanged {
                tick,
                action_serial,
                remaining_seconds,
            } => {
                put_u8(bytes, 11);
                put_u64(bytes, *tick);
                put_u32(bytes, *action_serial);
                put_f32(bytes, *remaining_seconds);
            }
            Self::StaminaSpent { tick, amount } => {
                put_u8(bytes, 13);
                put_u64(bytes, *tick);
                put_f32(bytes, *amount);
            }
            Self::PerspectiveObserved { tick, perspective } => {
                put_u8(bytes, 12);
                put_u64(bytes, *tick);
                put_u8(bytes, *perspective as u8);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventLog {
    events: Vec<CombatEvent>,
}

impl EventLog {
    pub fn events(&self) -> &[CombatEvent] {
        &self.events
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        encode_events(self.events.iter())
    }

    /// Canonical combat bytes with presentation observations removed.
    /// Perspective markers can be added, removed, or changed without altering
    /// this byte stream.
    pub fn mechanical_bytes(&self) -> Vec<u8> {
        encode_events(
            self.events
                .iter()
                .filter(|event| !event.is_presentation_only()),
        )
    }
}

pub struct TraceReplay;

impl TraceReplay {
    pub fn replay(trace: &CombatTrace) -> Result<EventLog, TraceError> {
        let mut combat = ErCombat::new(trace.initial_position);
        let mut events = Vec::new();
        let mut last_action_serial = 0;
        let mut previous_stamina = combat.output().stamina;

        for (tick_index, frame) in trace.ticks.iter().enumerate() {
            let tick = tick_index as u64;
            validate_frame(tick, frame)?;
            events.push(CombatEvent::InputAccepted {
                tick,
                input_id: tick + 1,
            });
            events.push(CombatEvent::WorldObserved {
                tick,
                actual_position: frame.world.position,
                ground_z: frame.world.ground_z,
                has_target: frame.world.has_target != 0,
                target: frame.world.target,
                in_combat: frame.world.in_combat != 0,
                stamina_current: frame.world.stamina_current,
                stamina_max: frame.world.stamina_max,
            });

            let output = combat.update(FIXED_DELTA_SECONDS, &frame.input, &frame.world);
            emit_output_events(
                tick,
                &output,
                frame.world.position,
                &mut last_action_serial,
                &mut previous_stamina,
                &mut events,
            );

            for incoming in &frame.incoming {
                let result = combat.receive_hit(incoming.from, incoming.low, incoming.hurt);
                events.push(CombatEvent::IncomingResolved {
                    tick,
                    source_id: incoming.source_id,
                    result,
                });
            }

            if frame.confirm_live_contact {
                if output.hit_live == 0 {
                    return Err(TraceError::ContactConfirmedOutsideLiveWindow { tick });
                }
                let delivery_id = melee_delivery_id(output.action_serial);
                combat.confirm_hit();
                events.push(CombatEvent::ContactConfirmed { tick, delivery_id });
                events.push(CombatEvent::HitStopChanged {
                    tick,
                    action_serial: output.action_serial,
                    remaining_seconds: output.hit_stop,
                });
            }

            for observation in &frame.consequences {
                events.push(CombatEvent::ConsequenceResolved {
                    tick,
                    delivery_id: observation.delivery_id,
                    outcome: observation.outcome,
                });
            }
            if let Some(perspective) = frame.perspective {
                events.push(CombatEvent::PerspectiveObserved { tick, perspective });
            }
        }

        Ok(EventLog { events })
    }
}

fn emit_output_events(
    tick: u64,
    output: &ErOutput,
    actual_position: ErVec3,
    last_action_serial: &mut u32,
    previous_stamina: &mut f32,
    events: &mut Vec<CombatEvent>,
) {
    if output.action_serial != *last_action_serial {
        events.push(CombatEvent::ActionStarted {
            tick,
            action_serial: output.action_serial,
            action_kind: output.action_kind,
            action_id: output.action,
        });
        *last_action_serial = output.action_serial;
    }
    events.push(CombatEvent::MotionRequested {
        tick,
        action_serial: output.action_serial,
        desired_position: output.position,
        desired_heading: output.heading,
    });
    events.push(CombatEvent::MotionResolved {
        tick,
        action_serial: output.action_serial,
        actual_position,
    });
    if output.stamina.to_bits() != previous_stamina.to_bits() {
        events.push(CombatEvent::StaminaChanged {
            tick,
            before: *previous_stamina,
            after: output.stamina,
        });
        *previous_stamina = output.stamina;
    }
    if output.stamina_spent != 0.0 {
        events.push(CombatEvent::StaminaSpent {
            tick,
            amount: output.stamina_spent,
        });
    }
    if output.hit_live != 0 {
        events.push(CombatEvent::ContactObserved {
            tick,
            delivery_id: melee_delivery_id(output.action_serial),
            action_serial: output.action_serial,
            hit_id: 1,
        });
    }
}

fn melee_delivery_id(action_serial: u32) -> u64 {
    (u64::from(action_serial) << 32) | 1
}

fn encode_events<'a>(events: impl Iterator<Item = &'a CombatEvent>) -> Vec<u8> {
    let selected = events.collect::<Vec<_>>();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(EVENT_MAGIC);
    put_u16(&mut bytes, EVENT_VERSION);
    put_u64(&mut bytes, selected.len() as u64);
    for event in selected {
        event.encode(&mut bytes);
    }
    bytes
}

fn validate_frame(tick: u64, frame: &TraceTick) -> Result<(), TraceError> {
    for (field, value) in [
        ("input.move_x", frame.input.move_x),
        ("input.move_y", frame.input.move_y),
        ("input.camera_heading", frame.input.camera_heading),
        ("world.ground_z", frame.world.ground_z),
        ("world.stamina_current", frame.world.stamina_current),
        ("world.stamina_max", frame.world.stamina_max),
    ] {
        validate_f32(tick, field, value)?;
    }
    validate_vec3(tick, "world.position", frame.world.position)?;
    validate_vec3(tick, "world.target", frame.world.target)?;
    for (field, value) in [
        ("world.has_target", frame.world.has_target),
        ("world.in_combat", frame.world.in_combat),
        ("input.crouch", frame.input.crouch),
        ("input.walk", frame.input.walk),
        ("input.lock", frame.input.lock),
        ("input.two_hand_right", frame.input.two_hand_right),
        ("input.two_hand_left", frame.input.two_hand_left),
    ] {
        validate_flag(tick, field, value)?;
    }
    for (name, button) in [
        ("input.dodge", frame.input.dodge),
        ("input.jump", frame.input.jump),
        ("input.light", frame.input.light),
        ("input.heavy", frame.input.heavy),
        ("input.guard", frame.input.guard),
    ] {
        validate_flag(tick, name, button.held)?;
        validate_flag(tick, name, button.pressed)?;
        validate_flag(tick, name, button.released)?;
    }
    for incoming in &frame.incoming {
        validate_vec3(tick, "incoming.from", incoming.from)?;
    }
    Ok(())
}

fn validate_vec3(tick: u64, field: &'static str, value: ErVec3) -> Result<(), TraceError> {
    if value.x.is_finite() && value.y.is_finite() && value.z.is_finite() {
        Ok(())
    } else {
        Err(TraceError::NonFinite { tick, field })
    }
}

fn validate_f32(tick: u64, field: &'static str, value: f32) -> Result<(), TraceError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(TraceError::NonFinite { tick, field })
    }
}

fn validate_flag(tick: u64, field: &'static str, value: u8) -> Result<(), TraceError> {
    if value <= 1 {
        Ok(())
    } else {
        Err(TraceError::InvalidFlag { tick, field, value })
    }
}

fn put_input(bytes: &mut Vec<u8>, input: &ErInput) {
    put_f32(bytes, input.move_x);
    put_f32(bytes, input.move_y);
    put_f32(bytes, input.camera_heading);
    for button in [
        input.dodge,
        input.jump,
        input.light,
        input.heavy,
        input.guard,
    ] {
        put_u8(bytes, button.held);
        put_u8(bytes, button.pressed);
        put_u8(bytes, button.released);
    }
    for flag in [
        input.crouch,
        input.walk,
        input.lock,
        input.two_hand_right,
        input.two_hand_left,
    ] {
        put_u8(bytes, flag);
    }
}

fn put_world(bytes: &mut Vec<u8>, world: &ErWorld) {
    put_vec3(bytes, world.position);
    put_f32(bytes, world.ground_z);
    put_u8(bytes, world.has_target);
    put_vec3(bytes, world.target);
    put_u8(bytes, world.in_combat);
    put_f32(bytes, world.stamina_current);
    put_f32(bytes, world.stamina_max);
}

fn put_vec3(bytes: &mut Vec<u8>, value: ErVec3) {
    put_f32(bytes, value.x);
    put_f32(bytes, value.y);
    put_f32(bytes, value.z);
}

fn put_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_bits().to_le_bytes());
}

fn put_u8(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

fn put_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

struct ByteReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ByteReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn invalid(&self, reason: &'static str) -> TraceError {
        TraceError::InvalidEncoding {
            offset: self.offset,
            reason,
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], TraceError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| self.invalid("field length overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| self.invalid("unexpected end of trace"))?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, TraceError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, TraceError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, TraceError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("four-byte slice"),
        ))
    }

    fn u64(&mut self) -> Result<u64, TraceError> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes(
            bytes.try_into().expect("eight-byte slice"),
        ))
    }

    fn f32(&mut self) -> Result<f32, TraceError> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn vec3(&mut self) -> Result<ErVec3, TraceError> {
        Ok(ErVec3 {
            x: self.f32()?,
            y: self.f32()?,
            z: self.f32()?,
        })
    }

    fn boolean(&mut self, _field: &'static str) -> Result<bool, TraceError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.invalid("boolean field must be zero or one")),
        }
    }

    fn button(&mut self) -> Result<crate::ffi::ErButton, TraceError> {
        Ok(crate::ffi::ErButton {
            held: self.u8()?,
            pressed: self.u8()?,
            released: self.u8()?,
        })
    }

    fn input(&mut self) -> Result<ErInput, TraceError> {
        Ok(ErInput {
            move_x: self.f32()?,
            move_y: self.f32()?,
            camera_heading: self.f32()?,
            dodge: self.button()?,
            jump: self.button()?,
            light: self.button()?,
            heavy: self.button()?,
            guard: self.button()?,
            crouch: self.u8()?,
            walk: self.u8()?,
            lock: self.u8()?,
            two_hand_right: self.u8()?,
            two_hand_left: self.u8()?,
        })
    }

    fn world(&mut self) -> Result<ErWorld, TraceError> {
        Ok(ErWorld {
            position: self.vec3()?,
            ground_z: self.f32()?,
            has_target: self.u8()?,
            target: self.vec3()?,
            in_combat: self.u8()?,
            stamina_current: self.f32()?,
            stamina_max: self.f32()?,
        })
    }

    fn hurt(&mut self) -> Result<ErHurt, TraceError> {
        match self.u32()? {
            0 => Ok(ErHurt::Small),
            1 => Ok(ErHurt::Middle),
            2 => Ok(ErHurt::Large),
            3 => Ok(ErHurt::Knockdown),
            _ => Err(self.invalid("invalid incoming hurt value")),
        }
    }

    fn usize_from_u32(&mut self) -> Result<usize, TraceError> {
        // Every platform supported by Rust has usize at least 32 bits.
        Ok(self.u32()? as usize)
    }

    fn usize_from_u64(&mut self, reason: &'static str) -> Result<usize, TraceError> {
        usize::try_from(self.u64()?).map_err(|_| self.invalid(reason))
    }
}
