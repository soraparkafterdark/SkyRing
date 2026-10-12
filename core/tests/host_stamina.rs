//! Skyrim owns the stamina pool: the core takes it from `ErWorld` every update and reports
//! what it spent in `ErOutput::stamina_spent`, so no parallel saved pool exists.

use er_combat_core::ffi::{ErAction, ErCombat, ErHitResult, ErHurt, ErInput, ErOutput, ErVec3, ErWorld};
use er_combat_core::sim::data::{MAX_STAMINA, ROLL_COST, SPRINT_DRAIN};

const DT: f32 = 1.0 / 60.0;

fn world(current: f32, max: f32) -> ErWorld {
    ErWorld { stamina_current: current, stamina_max: max, ..Default::default() }
}

/// Taps dodge (down on tick 2, up on tick 3) with the stick forward, one tick per update.
/// Returns every update's output.
fn tap_dodge(combat: &mut ErCombat, w: &mut ErWorld, ticks: usize) -> Vec<ErOutput> {
    (0..ticks)
        .map(|tick| {
            // A held stick makes the dodge a roll; with none it is a backstep.
            let mut input = ErInput { move_y: 1.0, ..Default::default() };
            match tick {
                2 => {
                    input.dodge.pressed = 1;
                    input.dodge.held = 1;
                }
                3 => input.dodge.released = 1,
                _ => {}
            }
            let out = combat.update(DT, &input, w);
            w.position = out.position;
            out
        })
        .collect()
}

fn started_roll(outputs: &[ErOutput]) -> Option<usize> {
    outputs.iter().position(|o| o.action_kind == ErAction::Roll)
}

#[test]
fn an_empty_host_pool_refuses_a_roll_and_any_stamina_allows_it() {
    // Actions need stamina above zero, as in Elden Ring: a sliver is enough, nothing is not.
    let mut empty = world(0.0, 100.0);
    let none = tap_dodge(&mut ErCombat::new(ErVec3::default()), &mut empty, 60);
    assert_eq!(started_roll(&none), None, "0 of 100 must not roll");

    let mut sliver = world(1.0, 100.0);
    let some = tap_dodge(&mut ErCombat::new(ErVec3::default()), &mut sliver, 60);
    assert!(started_roll(&some).is_some(), "1 of 100 must roll");

    let mut full = world(100.0, 100.0);
    let all = tap_dodge(&mut ErCombat::new(ErVec3::default()), &mut full, 60);
    assert!(started_roll(&all).is_some(), "100 of 100 must roll");
}

#[test]
fn the_update_that_starts_a_roll_reports_its_cost_in_host_units_and_idle_reports_zero() {
    let mut w = world(100.0, 100.0);
    let outputs = tap_dodge(&mut ErCombat::new(ErVec3::default()), &mut w, 60);
    let start = started_roll(&outputs).expect("the roll starts");
    let expected = ROLL_COST * 100.0 / MAX_STAMINA;
    assert!(
        (outputs[start].stamina_spent - expected).abs() < 1e-3,
        "roll start reported {} expected {expected}",
        outputs[start].stamina_spent
    );
    for (tick, out) in outputs.iter().enumerate() {
        if tick != start {
            assert_eq!(out.stamina_spent, 0.0, "tick {tick} spent {} but nothing started", out.stamina_spent);
        }
    }
    let total: f32 = outputs.iter().map(|o| o.stamina_spent).sum();
    assert!((total - expected).abs() < 1e-3, "the whole run spent {total}, one roll costs {expected}");
}

#[test]
fn the_report_scales_with_the_hosts_own_units() {
    // The same roll against a pool of 100, 1000 and 1.0 reports the same share of the pool.
    let share = |max: f32| {
        let mut w = world(max, max);
        let outputs = tap_dodge(&mut ErCombat::new(ErVec3::default()), &mut w, 60);
        outputs.iter().map(|o| o.stamina_spent).sum::<f32>() / max
    };
    let reference = ROLL_COST / MAX_STAMINA;
    for max in [100.0_f32, 1000.0, 1.0] {
        assert!((share(max) - reference).abs() < 1e-4, "pool {max}: share {} vs {reference}", share(max));
    }
}

#[test]
fn without_a_host_pool_the_core_keeps_and_reports_its_own() {
    // stamina_max 0 is what ErWorld::default() has: behaviour is unchanged, units are the core's.
    let mut w = ErWorld::default();
    assert_eq!(w.stamina_max, 0.0);
    let mut combat = ErCombat::new(ErVec3::default());
    let outputs = tap_dodge(&mut combat, &mut w, 60);
    let start = started_roll(&outputs).expect("a roll starts from a fresh core");
    assert!((outputs[start].stamina_spent - ROLL_COST).abs() < 1e-3);
    assert!(outputs[start].stamina < 1.0, "the internal pool dropped");
}

#[test]
fn the_host_pool_overrides_whatever_the_core_thinks_it_has() {
    let mut combat = ErCombat::new(ErVec3::default());
    let mut w = world(50.0, 100.0);
    let out = combat.update(DT, &ErInput::default(), &w);
    assert!((out.stamina - 0.5).abs() < 1e-4, "50 of 100 is half the pool, got {}", out.stamina);
    // Skyrim drinks a potion: the next update acts on it, with no memory of the old value.
    w.stamina_current = 100.0;
    assert!((combat.update(DT, &ErInput::default(), &w).stamina - 1.0).abs() < 1e-4);
}

#[test]
fn out_of_range_values_are_clamped_and_unusable_ones_fall_back_to_the_internal_pool() {
    let share = |w: ErWorld| ErCombat::new(ErVec3::default()).update(DT, &ErInput::default(), &w).stamina;
    assert!((share(world(500.0, 100.0)) - 1.0).abs() < 1e-6, "above the maximum is a full pool");
    assert_eq!(share(world(-20.0, 100.0)), 0.0, "below zero is an empty pool");
    for (current, max) in [(f32::NAN, 100.0), (50.0, f32::NAN), (f32::INFINITY, 100.0), (50.0, f32::INFINITY), (50.0, -100.0), (50.0, 0.0)] {
        assert!(
            (share(world(current, max)) - 1.0).abs() < 1e-6,
            "({current}, {max}) is not a usable pool: the internal one (full) stays"
        );
    }
}

#[test]
fn combat_sprinting_reports_the_drain_it_causes() {
    let mut combat = ErCombat::new(ErVec3::default());
    let mut w = ErWorld { in_combat: 1, ..world(100.0, 100.0) };
    let mut sprint_ticks = 0usize;
    let mut spent = 0.0_f32;
    for tick in 0..180 {
        let mut input = ErInput { move_y: 1.0, ..Default::default() };
        input.dodge.held = 1;
        if tick == 0 {
            input.dodge.pressed = 1;
        }
        let out = combat.update(DT, &input, &w);
        w.position = out.position;
        if out.sprinting != 0 {
            sprint_ticks += 1;
            spent += out.stamina_spent;
        }
    }
    assert!(sprint_ticks >= 60, "the player should sprint for at least a second, got {sprint_ticks} ticks");
    let expected = SPRINT_DRAIN * 100.0 / MAX_STAMINA * sprint_ticks as f32 * DT;
    assert!(
        (spent - expected).abs() <= expected * 0.05,
        "sprinting {sprint_ticks} ticks reported {spent}, expected {expected} within 5%"
    );
}

#[test]
fn sprinting_out_of_combat_costs_nothing() {
    let mut combat = ErCombat::new(ErVec3::default());
    let mut w = world(100.0, 100.0);
    let mut total = 0.0;
    for tick in 0..120 {
        let mut input = ErInput { move_y: 1.0, ..Default::default() };
        input.dodge.held = 1;
        input.dodge.pressed = u8::from(tick == 0);
        let out = combat.update(DT, &input, &w);
        w.position = out.position;
        total += out.stamina_spent;
    }
    assert_eq!(total, 0.0);
}

#[test]
fn a_guarded_hit_between_updates_is_reported_by_the_next_update() {
    let mut combat = ErCombat::new(ErVec3::default());
    let mut w = world(100.0, 100.0);
    let mut guard = ErInput::default();
    guard.guard.held = 1;
    guard.guard.pressed = 1;
    let mut out = combat.update(DT, &guard, &w);
    guard.guard.pressed = 0;
    for _ in 0..30 {
        out = combat.update(DT, &guard, &w);
    }
    assert_eq!(out.guarding, 1, "the guard is up");
    assert_eq!(out.stamina_spent, 0.0);

    // The bridge calls er_receive_hit between updates; an attacker straight ahead (+Y) is guarded.
    let result = combat.receive_hit(ErVec3 { x: 0.0, y: 150.0, z: 0.0 }, false, ErHurt::Middle);
    assert_eq!(result, ErHitResult::Blocked);
    let next = combat.update(DT, &guard, &w);
    assert!(next.stamina_spent > 0.0, "blocking costs stamina, and the next update must report it");
    w.position = next.position;
    assert_eq!(combat.update(DT, &guard, &w).stamina_spent, 0.0, "and only once");
}

#[test]
fn a_negative_host_value_never_reports_negative_spending() {
    // Skyrim's stamina can be driven below zero by damage-over-time effects. The core treats
    // that as an empty pool: a guarded hit breaks the guard, and spends nothing.
    let mut combat = ErCombat::new(ErVec3::default());
    let mut w = world(-20.0, 100.0);
    let mut guard = ErInput::default();
    guard.guard.held = 1;
    guard.guard.pressed = 1;
    for _ in 0..30 {
        combat.update(DT, &guard, &w);
        guard.guard.pressed = 0;
    }
    let result = combat.receive_hit(ErVec3 { x: 0.0, y: 150.0, z: 0.0 }, false, ErHurt::Middle);
    let next = combat.update(DT, &guard, &w);
    w.position = next.position;
    assert!(next.stamina_spent >= 0.0, "reported {}", next.stamina_spent);
    assert_ne!(result, ErHitResult::Blocked, "an empty pool cannot hold a guard against a hit");
}

#[test]
fn exhaustion_is_deterministic() {
    // The same host pool and inputs give the same reports, bit for bit.
    let run = || {
        let mut w = world(3.0, 100.0);
        tap_dodge(&mut ErCombat::new(ErVec3::default()), &mut w, 90)
            .iter()
            .map(|o| (o.action_kind as u32, o.stamina_spent.to_bits(), o.stamina.to_bits()))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}
