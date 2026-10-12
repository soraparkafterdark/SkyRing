use er_combat_core::ffi::{ErButton, ErCombat, ErHitResult, ErHurt, ErInput, ErVec3, ErWorld};
use er_combat_core::trace::{
    CombatEvent, ConsequenceObservation, IncomingContact, Perspective, TraceError, TraceRecorder,
    TraceReplay, TraceTick,
};

const TICKS: usize = 280;
const DT: f32 = 1.0 / 60.0;

fn input_at(tick: usize) -> ErInput {
    let mut input = ErInput {
        move_y: 0.7,
        ..Default::default()
    };
    match tick {
        5 => {
            input.dodge = ErButton {
                held: 1,
                pressed: 1,
                released: 0,
            }
        }
        6 => input.dodge.held = 1,
        7 => input.dodge.released = 1,
        60 => {
            input.light = ErButton {
                held: 1,
                pressed: 1,
                released: 0,
            }
        }
        61 => input.light.held = 1,
        62 => input.light.released = 1,
        120 => {
            input.heavy = ErButton {
                held: 1,
                pressed: 1,
                released: 0,
            }
        }
        121..=160 => input.heavy.held = 1,
        161 => input.heavy.released = 1,
        200 => {
            input.guard = ErButton {
                held: 1,
                pressed: 1,
                released: 0,
            }
        }
        201..=250 => input.guard.held = 1,
        251 => input.guard.released = 1,
        _ => {}
    }
    input
}

fn recorded_trace(with_perspective: bool) -> er_combat_core::trace::CombatTrace {
    let initial = ErVec3::default();
    let mut recorder = TraceRecorder::new(initial).expect("valid initial position");
    let mut source = ErCombat::new(initial);
    let mut world = ErWorld {
        in_combat: 1,
        ..Default::default()
    };
    let mut confirmed = false;

    for tick in 0..TICKS {
        let input = input_at(tick);
        let mut frame = TraceTick::new(input, world);
        if with_perspective && tick % 17 == 0 {
            frame.perspective = Some(if (tick / 17) % 2 == 0 {
                Perspective::FirstPerson
            } else {
                Perspective::ThirdPerson
            });
        }
        if tick == 12 || tick == 230 {
            frame.incoming.push(IncomingContact {
                source_id: 0xD00D + tick as u64,
                from: ErVec3 {
                    x: 0.0,
                    y: 100.0,
                    z: 0.0,
                },
                low: false,
                hurt: ErHurt::Middle,
            });
        }

        // Capture what the same fixed-step source instance produced so the
        // recorded confirm is guaranteed to name a genuinely live contact.
        let output = source.update(DT, &input, &world);
        for incoming in &frame.incoming {
            source.receive_hit(incoming.from, incoming.low, incoming.hurt);
        }
        if output.hit_live != 0 && !confirmed {
            let delivery_id = (u64::from(output.action_serial) << 32) | 1;
            frame.confirm_live_contact = true;
            frame.consequences.push(ConsequenceObservation {
                delivery_id,
                outcome: 7,
            });
            source.confirm_hit();
            confirmed = true;
        }
        recorder.record(frame).expect("captured facts are valid");
        world.position = output.position;
    }
    assert!(confirmed, "fixture must exercise live contact confirmation");
    recorder.finish()
}

#[test]
fn recorded_input_world_trace_and_replay_are_byte_stable() {
    let trace = recorded_trace(true);
    let trace_bytes = trace.canonical_bytes();
    assert_eq!(trace_bytes, trace.clone().canonical_bytes());
    let decoded = er_combat_core::trace::CombatTrace::from_canonical_bytes(&trace_bytes)
        .expect("canonical trace decodes");
    assert_eq!(trace_bytes, decoded.canonical_bytes());

    let first = TraceReplay::replay(&trace).expect("first replay");
    let second = TraceReplay::replay(&decoded).expect("decoded replay");
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(first.mechanical_bytes(), second.mechanical_bytes());

    let events = first.events();
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::InputAccepted { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::WorldObserved { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::ActionStarted { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::MotionRequested { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::MotionResolved { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::ContactObserved { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::ContactConfirmed { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::IncomingResolved { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::StaminaChanged { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::ConsequenceResolved { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::HitStopChanged { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, CombatEvent::PerspectiveObserved { .. })));

    let incoming = events
        .iter()
        .filter_map(|event| match event {
            CombatEvent::IncomingResolved { result, .. } => Some(*result),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(incoming.len(), 2);
    assert!(incoming.iter().all(|result| matches!(
        result,
        ErHitResult::Dodged
            | ErHitResult::Jumped
            | ErHitResult::Blocked
            | ErHitResult::GuardBroken
            | ErHitResult::Hit
            | ErHitResult::Ignored
    )));
}

#[test]
fn decoder_fails_closed_on_unknown_versions_and_trailing_bytes() {
    let trace = recorded_trace(false);
    // Version 1 had no stamina fields in the world record; reading it as version 2 would
    // misalign every later byte, so it must be refused, as must a version from the future.
    for version in [1u16, 3u16] {
        let mut unknown_version = trace.canonical_bytes();
        unknown_version[8..10].copy_from_slice(&version.to_le_bytes());
        assert!(
            matches!(
                er_combat_core::trace::CombatTrace::from_canonical_bytes(&unknown_version),
                Err(TraceError::UnsupportedVersion { version: v }) if v == version
            ),
            "version {version} must fail closed"
        );
    }

    let mut trailing = trace.canonical_bytes();
    trailing.push(0xAA);
    assert!(matches!(
        er_combat_core::trace::CombatTrace::from_canonical_bytes(&trailing),
        Err(TraceError::InvalidEncoding {
            reason: "trailing bytes after trace",
            ..
        })
    ));
}

#[test]
fn perspective_observations_cannot_change_mechanical_replay_bytes() {
    let with_perspective = recorded_trace(true);
    let mut recorder =
        TraceRecorder::new(with_perspective.initial_position()).expect("valid recorder");
    for source in with_perspective.ticks() {
        let mut frame = source.clone();
        frame.perspective = None;
        recorder.record(frame).expect("copied facts remain valid");
    }
    let without_perspective = recorder.finish();

    let observed = TraceReplay::replay(&with_perspective).expect("observed replay");
    let hidden = TraceReplay::replay(&without_perspective).expect("hidden replay");
    assert_ne!(observed.canonical_bytes(), hidden.canonical_bytes());
    assert_eq!(observed.mechanical_bytes(), hidden.mechanical_bytes());
}

#[test]
fn recorder_rejects_non_finite_facts_and_non_boolean_flags() {
    let mut recorder = TraceRecorder::new(ErVec3::default()).expect("valid recorder");
    let mut invalid_float = TraceTick::new(ErInput::default(), ErWorld::default());
    invalid_float.input.camera_heading = f32::NAN;
    assert_eq!(
        recorder.record(invalid_float),
        Err(TraceError::NonFinite {
            tick: 0,
            field: "input.camera_heading"
        })
    );

    let mut invalid_flag = TraceTick::new(ErInput::default(), ErWorld::default());
    invalid_flag.world.has_target = 2;
    assert_eq!(
        recorder.record(invalid_flag),
        Err(TraceError::InvalidFlag {
            tick: 0,
            field: "world.has_target",
            value: 2
        })
    );
}

#[test]
fn replay_rejects_contact_confirmation_outside_a_live_window() {
    let mut recorder = TraceRecorder::new(ErVec3::default()).expect("valid recorder");
    let mut frame = TraceTick::new(ErInput::default(), ErWorld::default());
    frame.confirm_live_contact = true;
    recorder
        .record(frame)
        .expect("recording is structurally valid");
    assert_eq!(
        TraceReplay::replay(&recorder.finish()),
        Err(TraceError::ContactConfirmedOutsideLiveWindow { tick: 0 })
    );
}
