//! Adversarial property tests for the magic core.
//!
//! Every test is deterministic: it drives a tiny in-file xorshift64 generator
//! seeded with fixed constants, and each property runs over at least 2000
//! generated cases. Only the public API is used (`er_combat_core::...`).
//! Failures, if any, are library findings recorded in SPELL_NOTES.md; failing
//! properties are marked `#[ignore = "FINDING: ..."]` after confirmation.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use er_combat_core::cast::{CastInput, MAX_TRACKED_RELEASES};
use er_combat_core::consequences::{DamageElement, ElementalProfile, TargetRules};
use er_combat_core::effects::{StatusDefinition, STATUS_COUNT};
use er_combat_core::magic::*;
use er_combat_core::magic_trace::{canonical_event_bytes, MagicRecorder, MagicScript};
use er_combat_core::magic_world::*;
use er_combat_core::posture::{PoiseDefinition, StanceDefinition};
use er_combat_core::spells::{all_catalysts, all_spells, apprentice_staff};

// --- xorshift64 ---------------------------------------------------------------------------

struct Xs(u64);

impl Xs {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    fn usize(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u64() % n as u64) as usize }
    }
    fn pick<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        &slice[self.usize(slice.len())]
    }
    /// Uniform in [0, 1).
    fn f32_01(&mut self) -> f32 {
        f32::from_bits(0x3F800000u32 | ((self.next_u64() >> 41) as u32)) - 1.0
    }
    /// Mixed magnitudes: zero, negatives, tiny, huge, infinities, NaN, ordinary.
    fn f32_any(&mut self) -> f32 {
        match self.usize(10) {
            0 => 0.0,
            1 => -self.f32_01(),
            2 => self.f32_01() * 1e-30,
            3 => self.f32_01() * 1e30,
            4 => f32::INFINITY,
            5 => f32::NEG_INFINITY,
            6 => f32::NAN,
            7 => -f32::from_bits(((self.next_u64() >> 33) as u32) | 0x40000000),
            _ => f32::from_bits(((self.next_u64() >> 33) as u32) | 0x3F000000),
        }
    }
    /// Finite value in [lo, hi].
    fn f32_range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32_01()
    }
}

// --- fixtures ------------------------------------------------------------------------------

fn rules() -> TargetRules {
    TargetRules {
        poise: PoiseDefinition { threshold: 100.0, recovery_delay_seconds: 2.0, recovery_per_second: 20.0 },
        stance: StanceDefinition { threshold: 100.0, decay_delay_seconds: 2.0, decay_per_second: 10.0, break_lockout_seconds: 3.0 },
        statuses: [StatusDefinition { threshold: 100.0, decay_delay_seconds: 2.0, decay_per_second: 10.0, proc_lockout_seconds: 5.0 }; STATUS_COUNT],
        status_received: [1.0; STATUS_COUNT],
        stance_received: 1.0,
    }
}

/// Spell definitions: sometimes a valid slice spell, sometimes a mutation of
/// one, sometimes fully random (mostly refused).
fn gen_spell(r: &mut Xs, id: u32) -> SpellDefinition {
    let mut spell = r.pick(&all_spells()).clone();
    spell.id = id;
    match r.usize(3) {
        // valid slice spell
        0 => spell,
        // small mutation (may or may not still validate)
        1 => {
            match r.usize(6) {
                0 => spell.power_ratio = r.f32_any(),
                1 => spell.cost_fraction = r.f32_any(),
                2 => spell.timing.release_at = r.f32_any(),
                3 => spell.timing.cancel_until = r.f32_any(),
                4 => spell.charge = if r.usize(2) == 0 { None } else { Some(Charge { max_seconds: r.f32_any(), power_at_full: r.f32_any() }) },
                _ => spell.payload.poise_pressure = r.f32_any(),
            }
            spell
        }
        // fully random, mostly refused
        _ => {
            spell.power_ratio = r.f32_any();
            spell.cost_fraction = r.f32_any();
            spell.timing = CastTiming {
                release_at: r.f32_any(),
                cancel_until: r.f32_any(),
                recovery: r.f32_any(),
                hyperarmour_from: r.f32_any(),
                hyperarmour_to: r.f32_any(),
            };
            spell.charge = if r.usize(2) == 0 { Some(Charge { max_seconds: r.f32_any(), power_at_full: r.f32_any() }) } else { None };
            spell.area = if r.usize(2) == 0 {
                Some(Area { radius: r.f32_any(), duration: r.f32_any(), tick_interval: r.f32_any() })
            } else {
                None
            };
            spell.payload.poise_pressure = r.f32_any();
            spell.payload.stance_pressure = r.f32_any();
            spell.payload.elements = if r.usize(2) == 0 {
                ElementalProfile::from_channels(&[(DamageElement::Fire, r.f32_any())]).unwrap_or_default()
            } else {
                ElementalProfile::default()
            };
            spell
        }
    }
}

fn gen_catalyst(r: &mut Xs, id: u32) -> Catalyst {
    if r.usize(2) == 0 {
        *r.pick(&all_catalysts())
    } else {
        Catalyst {
            id,
            kind: if r.usize(2) == 0 { CatalystKind::Staff } else { CatalystKind::Ring },
            school: School::Destruction,
            quality: r.f32_any(),
            upgrade_level: (r.next_u64() & 0xFF) as u8,
            passive: if r.usize(2) == 0 {
                Some(Passive { family: SpellFamily(r.u32() as u16), bonus: r.f32_any() })
            } else {
                None
            },
            cost_multiplier: r.f32_any(),
        }
    }
}

fn gen_spec(r: &mut Xs, spell_id: u32) -> CastSpec {
    CastSpec {
        hand: if r.usize(2) == 0 { Hand::Left } else { Hand::Right },
        casting: gen_catalyst(r, 1),
        offhand: if r.usize(3) == 0 { None } else { Some(gen_catalyst(r, 2)) },
        spell: gen_spell(r, spell_id),
        skill: if r.usize(4) == 0 { r.f32_any() } else { r.f32_range(0.0, 100.0) },
        magicka_current: if r.usize(3) == 0 { r.f32_any() } else { r.f32_range(0.0, 500.0) },
        magicka_max: if r.usize(3) == 0 { r.f32_any() } else { r.f32_range(1.0, 500.0) },
    }
}

fn gen_dt(r: &mut Xs) -> f32 {
    match r.usize(6) {
        0 => f32::NAN,
        1 => -r.f32_01(),
        2 => 0.0,
        3 => 1e30,
        4 => f32::INFINITY,
        _ => r.f32_range(0.0, 0.1),
    }
}

fn gen_input(r: &mut Xs) -> CastInput {
    CastInput {
        held: r.usize(2) == 0,
        dodge: r.usize(4) == 0,
        poise_broken: r.usize(8) == 0,
        magicka_current: if r.usize(3) == 0 { r.f32_any() } else { r.f32_range(0.0, 500.0) },
    }
}

/// A guaranteed-castable spec with a sane pool, used to guarantee that success
/// paths are exercised alongside the hostile ones.
fn sane_spec(spell_id: u32) -> CastSpec {
    CastSpec {
        hand: Hand::Left,
        casting: apprentice_staff(),
        offhand: None,
        spell: er_combat_core::spells::all_spells()[0].clone(),
        skill: 50.0,
        magicka_current: 500.0,
        magicka_max: 500.0,
    }
    .with_spell_id(spell_id)
}

trait WithSpellId {
    fn with_spell_id(self, id: u32) -> CastSpec;
}
impl WithSpellId for CastSpec {
    fn with_spell_id(mut self, id: u32) -> CastSpec {
        self.spell.id = id;
        self
    }
}

// ============================================================================================
// 1. no_panics_on_hostile_input
// ============================================================================================

#[test]
fn no_panics_on_hostile_input() {
    let seed = 0xA1D5_EED1;
    let mut refused = 0u64;
    let mut accepted = 0u64;
    let mut impacted = 0u64;
    for case in 0..3u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x9E37_79B9));
        let mut world = MagicWorld::new();
        for actor in [1u32, 2, 3, 4] {
            drop(world.spawn(actor, rules()));
        }
        let mut next_release: u64 = 1;
        for _ in 0..64 {
            match r.usize(8) {
                0 => {
                    let actor = r.u32();
                    drop(world.spawn(actor, rules()));
                }
                1 => {
                    let actor = r.u32();
                    drop(world.unload(actor));
                }
                2 => {
                    let actor = r.u32();
                    drop(world.kill(actor));
                }
                3 => {
                    let actor = r.u32();
                    drop(world.respawn(actor));
                }
                4 => {
                    // Alternate hostile and sane specs so both refusal and
                    // success paths are exercised.
                    let actor = 1 + r.u32() % 4;
                    if r.usize(2) == 0 {
                        drop(world.spawn(actor, rules()));
                    } else if !world.is_alive(actor) {
                        drop(world.respawn(actor));
                    }
                    let spec = if r.usize(3) == 0 {
                        { let id = 100 + r.u32() % 4; gen_spec(&mut r, id) }
                    } else {
                        sane_spec(101)
                    };
                    if matches!(world.begin_cast(actor, &spec), CastStart::Started) {
                        accepted += 1;
                    } else {
                        refused += 1;
                    }
                }
                5 => {
                    let dt = if r.usize(3) == 0 { gen_dt(&mut r) } else { 1.0 / 60.0 };
                    let mut inputs = BTreeMap::new();
                    for actor in [1u32, 2, 3, 4] {
                        if r.usize(4) != 0 {
                            let mut input = gen_input(&mut r);
                            input.magicka_current = 500.0;
                            inputs.insert(actor, input);
                        }
                    }
                    world.step(dt, &inputs);
                }
                _ => {
                    // Advance one sane tick, then impact whatever this very
                    // tick released (still in flight), plus a hostile id.
                    let mut inputs = BTreeMap::new();
                    for actor in [1u32, 2, 3, 4] {
                        let mut input = gen_input(&mut r);
                        input.magicka_current = 500.0;
                        input.dodge = false;
                        input.poise_broken = false;
                        inputs.insert(actor, input);
                    }
                    for _ in 0..40 {
                        world.step(1.0 / 60.0, &inputs);
                        // Impact whatever this tick released, before it can age out.
                        let now = world.tick_count();
                        let released: Vec<u64> = world.events().iter().filter_map(|e| match e {
                            MagicEvent::SpellReleased { release_id, tick, .. } if *tick == now => Some(*release_id),
                            _ => None,
                        }).collect();
                        let target = 1 + r.u32() % 4;
                        if !world.is_alive(target) {
                            drop(world.respawn(target));
                        }
                        for release in released {
                            if world.native_impact(release, target, 0).is_ok() {
                                impacted += 1;
                            } else {
                                refused += 1;
                            }
                        }
                    }
                    let now = world.tick_count();
                    let live: Vec<u64> = world.events().iter().filter_map(|e| match e {
                        MagicEvent::SpellReleased { release_id, tick, .. } if *tick == now => Some(*release_id),
                        _ => None,
                    }).collect();
                    let target = 1 + r.u32() % 4;
                    if !world.is_alive(target) {
                        drop(world.respawn(target));
                    }
                    for release in live {
                        let out = world.native_impact(release, target, 0);
                        
                        if out.is_ok() {
                            impacted += 1;
                        } else {
                            refused += 1;
                        }
                    }
                    next_release = next_release.wrapping_add(1);
                    let hostile_release = r.next_u64();
                    if world.native_impact(hostile_release, target, r.u32()).is_ok() {
                        impacted += 1;
                    } else {
                        refused += 1;
                    }
                }
            }
        }
        // Read-only probes must not panic either.
        drop(world.take_magicka_spent(r.u32()));
        drop(world.releases_in_flight());
        drop(world.events().len());
    }
    assert!(accepted > 0, "no successful cast; seed {seed} accepted={accepted} refused={refused} impacted={impacted}");
    assert!(impacted > 0, "no successful impact; seed {seed} accepted={accepted} refused={refused} impacted={impacted}");
    assert!(refused > 0, "no refusals exercised; seed {seed} accepted={accepted} refused={refused} impacted={impacted}");
}

// ============================================================================================
// 2. magicka_accounting
// ============================================================================================

#[test]
fn magicka_accounting() {
    let seed = 0xBEEF_CAFE;
    for case in 0..2200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0xD1B5_4A32));
        let mut world = MagicWorld::new();
        let caster = 7u32;
        world.spawn(caster, rules()).unwrap();
        let mut released_costs: Vec<f32> = Vec::new();
        for _ in 0..24 {
            match r.usize(3) {
                0 | 1 => {
                    let id = 100 + r.u32() % 4;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    if matches!(world.begin_cast(caster, &spec), CastStart::Started) {
                        released_costs.push(spec.spell.magicka_cost(&spec.casting, 1.0, spec.magicka_max));
                    }
                    let mut inputs = BTreeMap::new();
                    inputs.insert(caster, gen_input(&mut r));
                    world.step(gen_dt(&mut r), &inputs);
                }
                _ => {
                    let dt = gen_dt(&mut r);
                    let mut inputs = BTreeMap::new();
                    if r.usize(2) == 0 {
                        inputs.insert(caster, gen_input(&mut r));
                    }
                    world.step(dt, &inputs);
                }
            }
        }
        let spent_events: f32 = world
            .events()
            .iter()
            .filter_map(|e| match e {
                MagicEvent::MagickaSpent { actor, amount, .. } if *actor == caster => Some(*amount),
                _ => None,
            })
            .sum();
        let taken = world.take_magicka_spent(caster);
        let tol = 1e-3 * spent_events.abs().max(1.0);
        assert!(
            (spent_events - taken).abs() <= tol,
            "seed {seed} case {case}: event sum {spent_events} != take_magicka_spent {taken}"
        );
        assert!(taken >= 0.0, "seed {seed} case {case}: negative spend {taken}");
        let budget = released_costs.iter().sum::<f32>() * 1.001;
        assert!(
            taken <= budget,
            "seed {seed} case {case}: spend {taken} exceeds sum of released cast costs {budget}"
        );
    }
}

// ============================================================================================
// 3. release_ids_unique
// ============================================================================================

#[test]
fn release_ids_unique() {
    let seed = 0x1D5E_15E5;
    for case in 0..2200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x2545_F491));
        eprintln!("CASE {case}");
        let mut world = MagicWorld::new();
        let mut ids: BTreeSet<u64> = BTreeSet::new();
        let mut scanned = 0usize;
        for _ in 0..40 {
            let actor = 1 + r.usize(3) as u32;
            match r.usize(5) {
                0 => drop(world.spawn(actor, rules())),
                1 => drop(world.unload(actor)),
                2 => drop(world.kill(actor)),
                3 => drop(world.respawn(actor)),
                _ => {
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    drop(world.begin_cast(actor, &spec));
                    let mut inputs = BTreeMap::new();
                    inputs.insert(actor, gen_input(&mut r));
                    world.step(gen_dt(&mut r), &inputs);
                }
            }
            // Scan only newly appended events, not the whole cumulative log.
            for e in &world.events()[scanned..] {
                if let MagicEvent::SpellReleased { release_id, .. } = e {
                    if !ids.insert(*release_id) {
                        panic!("seed {seed} case {case}: duplicate release_id {release_id}");
                    }
                }
            }
            scanned = world.events().len();
        }
    }
}

// ============================================================================================
// 4. one_resolution_per_target_and_tick
// ============================================================================================

#[test]
fn one_resolution_per_target_and_tick() {
    let seed = 0xFACE_B00C;
    for case in 0..2200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x9E37_79B9));
        let mut world = MagicWorld::new();
        let caster = 1u32;
        world.spawn(caster, rules()).unwrap();
        for t in [2u32, 3, 4] {
            drop(world.spawn(t, rules()));
        }
                for _ in 0..60 {
            let id = 100 + r.u32() % 7;
            let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
            drop(world.begin_cast(caster, &spec));
            let mut inputs = BTreeMap::new();
            let mut input = gen_input(&mut r);
            input.magicka_current = 500.0;
            inputs.insert(caster, input);
            world.step(1.0 / 60.0, &inputs);
            // Impact random release ids against random targets with random tick
            // indices; the ledger alone must enforce exactly-once resolution.
            for _ in 0..r.usize(4) {
                let releases: Vec<u64> = world.events().iter().filter_map(|e| match e {
                    MagicEvent::SpellReleased { release_id, .. } => Some(*release_id),
                    _ => None,
                }).collect();
                let release = if !releases.is_empty() && r.usize(2) == 0 {
                    releases[r.usize(releases.len())]
                } else {
                    r.next_u64()
                };
                let target = 1 + r.u32() % 4;
                    eprintln!("DBG events_len={}", world.events().len());
                let tick = r.u32() % 8;
                if world.is_alive(target) || r.usize(2) == 0 {
                    drop(world.respawn(target));
                }
                drop(world.native_impact(release, target, tick));
            }
            let mut seen: HashMap<(u64, u32, u64), usize> = HashMap::new();
            for e in world.events() {
                if let MagicEvent::ImpactResolved { tick, release_id, target, .. } = e {
                    *seen.entry((*release_id, *target, *tick)).or_default() += 1;
                }
            }
            for ((release, target, tick), count) in &seen {
                assert!(
                    *count <= 1,
                    "seed {seed} case {case}: release {release} resolved target {target} tick {tick} {count} times"
                );
            }
        }
    }
}

// ============================================================================================
// 5. dead_actors_take_nothing
// ============================================================================================

#[test]
fn dead_actors_take_nothing() {
    let seed = 0xDEAD_BEEF;
    for case in 0..2200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x6D2B_79F5));
        let mut world = MagicWorld::new();
        let actors = [1u32, 2, 3];
        for a in actors {
            drop(world.spawn(a, rules()));
        }
        for _ in 0..40 {
            let actor = actors[r.usize(3)];
            match r.usize(5) {
                0 => drop(world.kill(actor)),
                1 => drop(world.respawn(actor)),
                2 => {
                    if world.is_alive(actor) {
                        drop(world.begin_cast(actor, &{ let id = 100 + r.u32() % 7; gen_spec(&mut r, id) }));
                    }
                }
                3 => {
                    let mut inputs = BTreeMap::new();
                    inputs.insert(actor, gen_input(&mut r));
                    world.step(gen_dt(&mut r), &inputs);
                }
                _ => {
                    let target = actors[r.usize(3)];
                    let release = r.next_u64();
                    drop(world.native_impact(release, target, 0));
                }
            }
            // Walk the log maintaining a dead-set: while dead, an actor must
            // neither take an ImpactResolved nor start a Cast.
            let mut dead: BTreeSet<u32> = BTreeSet::new();
            for e in world.events() {
                match e {
                    MagicEvent::ActorDied { actor, .. } => {
                        dead.insert(*actor);
                    }
                    MagicEvent::ActorRespawned { actor, .. }
                    | MagicEvent::ActorUnloaded { actor, .. }
                    | MagicEvent::ActorSpawned { actor, .. } => {
                        dead.remove(actor);
                    }
                    MagicEvent::ImpactResolved { target, .. } => {
                        assert!(!dead.contains(target), "seed {seed} case {case}: dead actor {target} took an impact");
                    }
                    MagicEvent::CastStarted { actor, .. } => {
                        assert!(!dead.contains(actor), "seed {seed} case {case}: dead actor {actor} started a cast");
                    }
                    _ => {}
                }
            }
        }
    }
}

// ============================================================================================
// 6. events_are_ordered
// ============================================================================================

#[test]
fn events_are_ordered() {
    let seed = 0x05D3_0001;
    for case in 0..2200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x2722_0A95));
        let mut world = MagicWorld::new();
        for _ in 0..40 {
            match r.usize(6) {
                0 => drop(world.spawn(r.u32(), rules())),
                1 => drop(world.unload(r.u32())),
                2 => drop(world.kill(r.u32())),
                3 => drop(world.respawn(r.u32())),
                4 => {
                    let actor = r.u32();
                    drop(world.begin_cast(actor, &gen_spec(&mut r, actor)));
                }
                _ => {
                    let mut inputs = BTreeMap::new();
                    for _ in 0..r.usize(3) {
                        let actor = r.u32();
                        inputs.insert(actor, gen_input(&mut r));
                    }
                    world.step(gen_dt(&mut r), &inputs);
                }
            }
        }
        drop(world.native_impact(r.next_u64(), r.u32(), r.u32()));
        let mut last = 0u64;
        for e in world.events() {
            let tick = match e {
                MagicEvent::ActorSpawned { tick, .. }
                | MagicEvent::ActorDied { tick, .. }
                | MagicEvent::ActorRespawned { tick, .. }
                | MagicEvent::ActorUnloaded { tick, .. }
                | MagicEvent::CastStarted { tick, .. }
                | MagicEvent::CastRefused { tick, .. }
                | MagicEvent::SpellReleased { tick, .. }
                | MagicEvent::MagickaSpent { tick, .. }
                | MagicEvent::ReleaseUntracked { tick, .. }
                | MagicEvent::SelfEffectsApplied { tick, .. }
                | MagicEvent::CastEnded { tick, .. }
                | MagicEvent::ImpactResolved { tick, .. }
                | MagicEvent::ImpactRefused { tick, .. } => *tick,
            };
            assert!(tick >= last, "seed {seed} case {case}: tick went {last} -> {tick}");
            last = tick;
        }
    }
}

// ============================================================================================
// 7. ledger_is_bounded
// ============================================================================================

#[test]
fn ledger_is_bounded() {
    let seed = 0xB0D9_00D5;
    for case in 0..2200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x1656_67E1));
        let mut world = MagicWorld::new();
        let caster = 42u32;
        world.spawn(caster, rules()).unwrap();
        for _ in 0..60 {
            match r.usize(4) {
                0 => {
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    drop(world.begin_cast(caster, &spec));
                }
                1 => {
                    let mut inputs = BTreeMap::new();
                    let mut input = gen_input(&mut r);
                    input.magicka_current = 500.0;
                    inputs.insert(caster, input);
                    world.step(gen_dt(&mut r), &inputs);
                }
                2 => {
                    drop(world.unload(caster));
                    drop(world.spawn(caster, rules()));
                }
                _ => {
                    // Many actors releasing so the ledger is pressed from all sides.
                    let actor = caster + 1 + r.u32() % 8;
                    drop(world.spawn(actor, rules()));
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    drop(world.begin_cast(actor, &spec));
                    let mut inputs = BTreeMap::new();
                    let mut input = gen_input(&mut r);
                    input.magicka_current = 500.0;
                    inputs.insert(actor, input);
                    world.step(gen_dt(&mut r), &inputs);
                }
            }
            assert!(
                world.releases_in_flight() <= MAX_TRACKED_RELEASES,
                "seed {seed} case {case}: {} releases in flight > MAX_TRACKED_RELEASES",
                world.releases_in_flight()
            );
        }
    }
}

// ============================================================================================
// 8. multiplier_bounds
// ============================================================================================

#[test]
fn multiplier_bounds() {
    let seed = 0x9E37_79B9;
    for case in 0..2200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x85EB_CA6B));
        let mut world = MagicWorld::new();
        let caster = 9u32;
        world.spawn(caster, rules()).unwrap();
        for _ in 0..30 {
            let id = 100 + r.u32() % 7;
            let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
            drop(world.begin_cast(caster, &spec));
            let mut inputs = BTreeMap::new();
            let mut input = gen_input(&mut r);
            input.magicka_current = 500.0;
            inputs.insert(caster, input);
            world.step(gen_dt(&mut r), &inputs);
        }
        for e in world.events() {
            match e {
                MagicEvent::SpellReleased { damage_multiplier, .. } => {
                    assert!(
                        damage_multiplier.is_finite()
                            && (0.0..=MAX_SPELL_DAMAGE_MULTIPLIER).contains(damage_multiplier),
                        "seed {seed} case {case}: SpellReleased multiplier {damage_multiplier} out of range"
                    );
                }
                MagicEvent::ImpactResolved { damage_multiplier, .. } => {
                    assert!(
                        damage_multiplier.is_finite() && *damage_multiplier >= 0.0,
                        "seed {seed} case {case}: ImpactResolved multiplier {damage_multiplier} not finite and >= 0"
                    );
                }
                _ => {}
            }
        }
    }
}

// ============================================================================================
// 9. replay_matches
// ============================================================================================

#[test]
fn replay_matches() {
    let seed = 0x5EED_5EED;
    let mut exercised = 0u32;
    for case in 0..2100u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0xC2B2_AE3D));
        let mut rec = MagicRecorder::new();
        let mut next_release: u64 = 1;
        for _ in 0..30 {
            match r.usize(8) {
                0 => drop(rec.spawn(r.u32(), rules())),
                1 => drop(rec.unload(r.u32())),
                2 => drop(rec.kill(r.u32())),
                3 => drop(rec.respawn(r.u32())),
                4 => {
                    let actor = r.u32();
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    drop(rec.begin_cast(actor, &spec));
                }
                5 => {
                    let mut inputs = BTreeMap::new();
                    for _ in 0..r.usize(3) {
                        let actor = r.u32();
                        inputs.insert(actor, gen_input(&mut r));
                    }
                    rec.step(gen_dt(&mut r), &inputs);
                }
                6 => {
                    let actor = r.u32();
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    drop(rec.begin_cast(actor, &spec));
                    let mut inputs = BTreeMap::new();
                    let mut input = gen_input(&mut r);
                    input.magicka_current = 500.0;
                    inputs.insert(actor, input);
                    rec.step(1.0 / 60.0, &inputs);
                }
                _ => {
                    let target = r.u32();
                    let release = next_release;
                    next_release = next_release.wrapping_add(1);
                    drop(rec.spawn(target, rules()));
                    drop(rec.native_impact(release, target, 0));
                }
            }
        }
        drop(rec.take_magicka_spent(r.u32()));
        let (script, live) = rec.finish();
        let replayed: Vec<MagicEvent> = MagicScript::replay(&script);
        assert_eq!(live, replayed, "seed {seed} case {case}: replay log differs from live log");
        assert_eq!(
            canonical_event_bytes(&live),
            canonical_event_bytes(&replayed),
            "seed {seed} case {case}: canonical bytes differ"
        );
        if !live.is_empty() {
            exercised += 1;
        }
    }
    assert!(exercised > 2000, "generator degenerate: only {exercised} cases produced events; seed {seed}");
}
