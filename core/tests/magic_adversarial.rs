//! Adversarial property tests for the magic core.
//!
//! Every test is deterministic: it drives a tiny in-file xorshift64 generator
//! seeded with fixed constants, and each property runs over generated cases.
//! Only the public API is used (`er_combat_core::...`).
//!
//! The properties here are written to be load-bearing, in two ways:
//!
//! 1. Coverage counters. Each property counts what its generator actually
//!    built (accepted casts, released spells, resolved impacts, refused
//!    impacts broken out per refusal reason, kills, respawns, unloads) and
//!    asserts each counter clears a floor. A property whose generator stops
//!    building the situation it is about fails on its coverage asserts
//!    instead of passing vacuously.
//! 2. Deterministic bound-reaching cases. Each property also contains cases
//!    built specifically to reach the bound it guards: a ledger filled past
//!    `MAX_TRACKED_RELEASES`, a maxed build pressed against
//!    `MAX_SPELL_DAMAGE_MULTIPLIER`, a 300-step run that sweeps the tick
//!    counter past any small modulus, kill/respawn/unload inside recorded
//!    fights, and impacts aimed at REAL release ids (`SpellReleased`)
//!    against alive, dead and unknown targets with repeating and
//!    out-of-range tick indexes.
//!
//! Checked against deliberate library mutants; see SPELL_NOTES.md.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use er_combat_core::cast::{CastInput, ImpactError, Registration, MAX_TRACKED_RELEASES};
use er_combat_core::consequences::{DamageElement, ElementalProfile, TargetRules};
use er_combat_core::effects::{StatusDefinition, STATUS_COUNT};
use er_combat_core::magic::*;
use er_combat_core::magic_trace::{canonical_event_bytes, MagicRecorder, MagicScript, ScriptStep};
use er_combat_core::magic_world::*;
use er_combat_core::posture::{PoiseDefinition, StanceDefinition};
use er_combat_core::spells::{all_catalysts, all_spells, apprentice_staff, sages_staff, sun_ring};

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

/// Spell definitions: a valid slice spell with a fresh id, a mutation of one,
/// or a fully random one (mostly refused).
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
        // fully random (mostly refused)
        _ => {
            spell.family = SpellFamily(r.u32() as u16);
            spell.delivery = match r.usize(4) {
                0 => SpellDelivery::Instant,
                1 => SpellDelivery::Projectile,
                2 => SpellDelivery::Area,
                _ => SpellDelivery::SelfCast,
            };
            spell.castable_from = match r.usize(4) {
                0 => FROM_STAFF,
                1 => FROM_RING,
                2 => FROM_STAFF | FROM_RING,
                _ => 0,
            };
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
        spell: all_spells()[0].clone(),
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

/// A slice spell re-id'd, so a test can pin a delivery family by construction
/// (index 2 is the persistent area, index 3 the self-cast buff).
fn slice_spell(id: u32, index: usize) -> SpellDefinition {
    let mut spell = all_spells()[index % all_spells().len()].clone();
    spell.id = id;
    spell
}

/// A spec that certainly casts and releases `spell` from the caster's staff.
fn staff_spec(spell: SpellDefinition) -> CastSpec {
    CastSpec { hand: Hand::Left, casting: apprentice_staff(), offhand: None, spell, skill: 50.0, magicka_current: 500.0, magicka_max: 500.0 }
}

/// Begin a cast and then step it at a fixed sane cadence until it has passed
/// its release frame, so releases happen constantly rather than by luck.
fn cast_and_release(world: &mut MagicWorld, actor: u32, spec: &CastSpec) -> CastStart {
    let started = world.begin_cast(actor, spec);
    for _ in 0..18 {
        let mut inputs = BTreeMap::new();
        inputs.insert(actor, CastInput { held: false, dodge: false, poise_broken: false, magicka_current: 500.0 });
        world.step(0.1, &inputs);
    }
    started
}

/// The release ids the world has actually minted, most recent last. Only the
/// tail of the log is scanned: recent releases are the ones still in flight.
fn real_release_ids(events: &[MagicEvent]) -> Vec<u64> {
    let start = events.len().saturating_sub(600);
    events[start..]
        .iter()
        .filter_map(|e| match e {
            MagicEvent::SpellReleased { release_id, .. } => Some(*release_id),
            _ => None,
        })
        .collect()
}

/// The newest `SpellReleased` id minted for a given spell id, if any.
fn newest_release_for(events: &[MagicEvent], spell: u32) -> Option<u64> {
    events
        .iter()
        .rev()
        .find_map(|e| match e {
            MagicEvent::SpellReleased { release_id, spell: s, .. } if *s == spell => Some(*release_id),
            _ => None,
        })
}

/// The counters every property carries. A property whose generator stopped
/// building the situation it is about must fail on its coverage asserts, not
/// pass silently. Counters are folded from the event log, which is the
/// world's own record of what happened.
#[derive(Default)]
struct Coverage {
    accepted: u64,
    refused: u64,
    released: u64,
    resolved: u64,
    impact_refused: u64,
    refuse_unknown_target: u64,
    refuse_target_dead: u64,
    refuse_ledger_unknown: u64,
    refuse_ledger_duplicate: u64,
    refuse_ledger_tick: u64,
    untracked_full: u64,
    kills: u64,
    respawns: u64,
    unloads: u64,
}

impl Coverage {
    /// Fold one event into the counters.
    fn note(&mut self, event: &MagicEvent) {
        match event {
            MagicEvent::CastStarted { .. } => self.accepted += 1,
            MagicEvent::CastRefused { .. } => self.refused += 1,
            MagicEvent::SpellReleased { .. } => self.released += 1,
            MagicEvent::ReleaseUntracked { reason: Registration::Full, .. } => self.untracked_full += 1,
            MagicEvent::ActorDied { .. } => self.kills += 1,
            MagicEvent::ActorRespawned { .. } => self.respawns += 1,
            MagicEvent::ActorUnloaded { .. } => self.unloads += 1,
            MagicEvent::ImpactResolved { .. } => self.resolved += 1,
            MagicEvent::ImpactRefused { reason, .. } => {
                self.impact_refused += 1;
                match reason {
                    ImpactRefusal::UnknownTarget => self.refuse_unknown_target += 1,
                    ImpactRefusal::TargetDead => self.refuse_target_dead += 1,
                    ImpactRefusal::Ledger(ImpactError::Unknown) => self.refuse_ledger_unknown += 1,
                    ImpactRefusal::Ledger(ImpactError::Duplicate) => self.refuse_ledger_duplicate += 1,
                    ImpactRefusal::Ledger(ImpactError::TickOutOfRange) => self.refuse_ledger_tick += 1,
                    ImpactRefusal::Ledger(ImpactError::SelfCast) => {}
                }
            }
            _ => {}
        }
    }

    /// The shared floor every generator must clear: refusals AND resolutions
    /// both happen constantly, and the world life-cycle is really exercised.
    fn assert_common(&self, ctx: &str, seed: u64) {
        assert!(self.accepted >= 200, "{ctx}: only {} accepted casts (seed {seed})", self.accepted);
        assert!(self.refused >= 200, "{ctx}: only {} refused casts (seed {seed})", self.refused);
        assert!(self.released >= 200, "{ctx}: only {} released spells (seed {seed})", self.released);
        assert!(self.resolved >= 200, "{ctx}: only {} resolved impacts (seed {seed})", self.resolved);
        assert!(self.impact_refused >= 200, "{ctx}: only {} refused impacts (seed {seed})", self.impact_refused);
        assert!(self.refuse_unknown_target >= 20, "{ctx}: only {} unknown-target refusals (seed {seed})", self.refuse_unknown_target);
        assert!(self.refuse_ledger_unknown >= 20, "{ctx}: only {} ledger-unknown refusals (seed {seed})", self.refuse_ledger_unknown);
        assert!(self.kills >= 20, "{ctx}: only {} kills (seed {seed})", self.kills);
        assert!(self.respawns >= 20, "{ctx}: only {} respawns (seed {seed})", self.respawns);
        assert!(self.unloads >= 5, "{ctx}: only {} unloads (seed {seed})", self.unloads);
    }
}

/// An actor id that no case ever spawns, so impacts aimed at it exercise the
/// unknown-target refusal.
const GHOST_TARGET: u32 = 0xFFFF_FFF0;

/// Impacts a random release against a random target and tick index: the
/// release is a REAL minted id half the time (biased to the newest few, which
/// are likeliest still in flight, so resolutions happen constantly), the other
/// half a fabricated id (which must be refused as ledger-unknown). Tick
/// indexes repeat and go out of range on purpose.
fn random_impact(r: &mut Xs, world: &mut MagicWorld, ids: &[u32]) {
    let target = ids[r.usize(ids.len())];
    let real = real_release_ids(world.events());
    let release = if !real.is_empty() && r.usize(2) != 0 {
        let back = 1 + r.usize(real.len().min(4));
        real[real.len() - back]
    } else {
        r.next_u64() | 1
    };
    // Tick 0 is the only legal index for a projectile; other values must be
    // refused (or, for an area, resolve only inside the area's life).
    let tick = match r.usize(6) {
        0 | 1 | 2 | 3 => 0,
        4 => (r.next_u64() % 6) as u32,
        _ => r.u32(),
    };
    let _ = world.native_impact(release, target, tick);
}

/// One guaranteed-impact round: aim the newest real release at a live target
/// (resolves) and at a dead target (must be refused `TargetDead` *before* the
/// release is consumed). This is the deterministic detector for a world that
/// lets a dead actor take a hit, and it keeps resolutions flowing.
fn impact_real_release_on_live_and_dead(world: &mut MagicWorld, live_target: u32, corpse: u32) {
    let real = real_release_ids(world.events());
    let Some(release) = real.last().copied() else { return };
    if world.is_alive(live_target) {
        let _ = world.native_impact(release, live_target, 0);
    }
    if world.state(corpse).is_some() {
        if world.is_alive(corpse) {
            let _ = world.kill(corpse);
        }
        // The world must refuse a dead target before the release is consumed:
        // the reason is pinned, so a mutant that skips the dead check fails
        // here whether it resolves the hit or refuses it for another reason.
        match world.native_impact(release, corpse, 0) {
            Err(ImpactRefusal::TargetDead) => {}
            other => panic!("impact on dead actor {corpse}: expected TargetDead refusal, got {other:?}"),
        }
    }
}

// ============================================================================================
// 1. no_panics_on_hostile_input
// ============================================================================================

#[test]
fn no_panics_on_hostile_input() {
    let seed = 0xA1D5_EED1;
    let mut c = Coverage::default();
    for case in 0..800u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x9E37_79B9));
        let mut world = MagicWorld::new();
        for actor in [1u32, 2, 3, 4, 5, 6] {
            let _ = world.spawn(actor, rules());
        }
        let actors = [1u32, 2, 3, 4];
        // 5 is a dedicated live target and 6 a dedicated corpse for the
        // deterministic probe; GHOST is never spawned.
        let targets = [1u32, 2, 3, 4, 5, 6, GHOST_TARGET];
        for _ in 0..64 {
            match r.usize(8) {
                0 => {
                    let _ = world.spawn(r.u32(), rules());
                }
                1 => {
                    let actor = actors[r.usize(actors.len())];
                    if world.is_alive(actor) {
                        let _ = world.kill(actor);
                    }
                }
                2 => {
                    let actor = actors[r.usize(actors.len())];
                    if world.state(actor).is_some() && !world.is_alive(actor) {
                        let _ = world.respawn(actor);
                    }
                }
                3 => {
                    let actor = actors[r.usize(actors.len())];
                    let _ = world.unload(actor);
                    let _ = world.spawn(actor, rules());
                }
                4 | 5 => {
                    let actor = actors[r.usize(actors.len())];
                    let id = 100 + r.u32() % 7;
                    // One in two is a projectile (10 s flight), so a live
                    // release is almost always in flight for the impact half.
                    let spec = if r.usize(2) == 0 {
                        staff_spec(slice_spell(id, 1))
                    } else if r.usize(2) == 0 {
                        sane_spec(id)
                    } else {
                        gen_spec(&mut r, id)
                    };
                    drop(cast_and_release(&mut world, actor, &spec));
                }
                6 => {
                    let mut inputs = BTreeMap::new();
                    for _ in 0..r.usize(3) {
                        inputs.insert(actors[r.usize(actors.len())], gen_input(&mut r));
                    }
                    world.step(gen_dt(&mut r), &inputs);
                }
                _ => {
                    if r.usize(4) == 0 {
                        // Deterministic probe: a guaranteed tracked release,
                        // one resolution on a live target, one TargetDead.
                        if !world.is_alive(5) {
                            let _ = world.respawn(5);
                        }
                        drop(cast_and_release(&mut world, 5, &staff_spec(slice_spell(600, 1))));
                        impact_real_release_on_live_and_dead(&mut world, 5, 6);
                    } else {
                        random_impact(&mut r, &mut world, &targets);
                    }
                }
            }
        }
        // Read-only probes must not panic either.
        let _ = world.take_magicka_spent(r.u32());
        let _ = world.releases_in_flight();
        drop(world.events().len());
        for e in world.events() {
            c.note(e);
        }
    }
    c.assert_common("no_panics_on_hostile_input", seed);
}

// ============================================================================================
// 2. magicka_accounting
// ============================================================================================

#[test]
fn magicka_accounting() {
    let seed = 0xBEEF_CAFE;
    let mut released_count = 0u64;
    for case in 0..1200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0xD1B5_4A32));
        let mut world = MagicWorld::new();
        let caster = 7u32;
        world.spawn(caster, rules()).unwrap();
        let mut released_costs: Vec<f32> = Vec::new();
        for _ in 0..24 {
            match r.usize(4) {
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
        released_count += world.events().iter().filter(|e| matches!(e, MagicEvent::SpellReleased { .. })).count() as u64;
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
    // The property must not pass because the generator never released anything.
    assert!(released_count >= 200, "generator degenerate: only {released_count} releases over the run");
}

// ============================================================================================
// 3. release_ids_unique
// ============================================================================================

#[test]
fn release_ids_unique() {
    let seed = 0x1D5E_15E5;
    let mut total_released = 0u64;
    for case in 0..1200u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x2545_F491));
        let mut world = MagicWorld::new();
        let mut ids: BTreeSet<u64> = BTreeSet::new();
        let mut scanned = 0usize;
        for _ in 0..40 {
            let actor = 1 + r.usize(3) as u32;
            match r.usize(6) {
                0 => {
                    let _ = world.spawn(actor, rules());
                }
                1 => {
                    let _ = world.unload(actor);
                }
                2 => {
                    let _ = world.kill(actor);
                }
                3 => {
                    let _ = world.respawn(actor);
                }
                4 => {
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    let _ = world.begin_cast(actor, &spec);
                    let mut inputs = BTreeMap::new();
                    inputs.insert(actor, gen_input(&mut r));
                    world.step(gen_dt(&mut r), &inputs);
                }
                _ => {
                    let id = 100 + r.u32() % 7;
                    drop(cast_and_release(&mut world, actor, &sane_spec(id)));
                }
            }
            // Scan only newly appended events, not the whole cumulative log.
            for e in &world.events()[scanned..] {
                if let MagicEvent::SpellReleased { release_id, .. } = e {
                    total_released += 1;
                    if !ids.insert(*release_id) {
                        panic!("seed {seed} case {case}: duplicate release_id {release_id}");
                    }
                }
            }
            scanned = world.events().len();
        }
    }
    assert!(total_released >= 400, "generator degenerate: only {total_released} releases over the run");
}

// ============================================================================================
// 4. one_resolution_per_target_and_tick
//
// Impacts are aimed at REAL release ids (harvested from SpellReleased) against
// alive, dead and unknown targets, with tick indexes that repeat and go out of
// range. Besides the random sweep, each case runs deterministic probes that
// pin the ledger's contract edge to edge:
//
// - an area tick at `area.ticks()` must be refused TickOutOfRange;
// - the same (release, target, tick) twice must resolve once, then refuse
//   Duplicate;
// - a release that was never tracked (a self-cast) must refuse, never damage;
// - an unknown target must be refused UnknownTarget WITHOUT consuming the
//   release (a second identical attempt is still UnknownTarget);
// - a dead target must be refused TargetDead before the release is consumed.
// ============================================================================================

#[test]
fn one_resolution_per_target_and_tick() {
    let seed = 0xFACE_B00C;
    let mut c = Coverage::default();
    for case in 0..400u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x9E37_79B9));
        let mut world = MagicWorld::new();
        let caster = 1u32;
        world.spawn(caster, rules()).unwrap();
        for t in [2u32, 3, 4, 5] {
            let _ = world.spawn(t, rules());
        }
        let targets = [1u32, 2, 3, 4, 5, 999];

        // --- deterministic probes ---------------------------------------------------------
        // A persistent area (the slice's pyre pillar, re-id'd to 777): four
        // ticks over six seconds, so tick index 4 is out of range.
        let started = cast_and_release(&mut world, caster, &staff_spec(slice_spell(777, 2)));
        assert!(matches!(started, CastStart::Started), "area probe cast refused: {started:?}");
        let area_release = newest_release_for(world.events(), 777).expect("area probe never released");
        match world.native_impact(area_release, 2, 4) {
            Err(ImpactRefusal::Ledger(ImpactError::TickOutOfRange)) => {}
            other => panic!("seed {seed} case {case}: area tick 4 must be TickOutOfRange, got {other:?}"),
        }
        assert!(
            world.native_impact(area_release, 2, 0).is_ok(),
            "seed {seed} case {case}: first resolution of a live release on a live target must succeed"
        );
        match world.native_impact(area_release, 2, 0) {
            Err(ImpactRefusal::Ledger(ImpactError::Duplicate)) => {}
            other => panic!("seed {seed} case {case}: repeat (release,target,tick) must be Duplicate, got {other:?}"),
        }
        // A self-cast (the slice's haste buff, re-id'd to 778) is never
        // tracked: its release id must refuse, never resolve.
        let started = cast_and_release(&mut world, caster, &staff_spec(slice_spell(778, 3)));
        assert!(matches!(started, CastStart::Started), "self-cast probe refused: {started:?}");
        let self_release = newest_release_for(world.events(), 778).expect("self-cast probe never released");
        match world.native_impact(self_release, 2, 0) {
            Err(ImpactRefusal::Ledger(ImpactError::Unknown)) => {}
            other => panic!("seed {seed} case {case}: untracked self-cast release must refuse, got {other:?}"),
        }
        // An unknown target is refused before the release is consumed: doing
        // it twice must give the same refusal, not a duplicate marker.
        for _ in 0..2 {
            match world.native_impact(area_release, 999, 0) {
                Err(ImpactRefusal::UnknownTarget) => {}
                other => panic!("seed {seed} case {case}: unknown target must be UnknownTarget, got {other:?}"),
            }
        }
        // A dead target is refused before the release is consumed.
        let _ = world.kill(3);
        match world.native_impact(area_release, 3, 0) {
            Err(ImpactRefusal::TargetDead) => {}
            other => panic!("seed {seed} case {case}: dead target must be TargetDead, got {other:?}"),
        }

        // --- random sweep -----------------------------------------------------------------
        let mut scanned = 0usize;
        let mut seen: HashMap<(u64, u32, u64), usize> = HashMap::new();
        for _ in 0..40 {
            match r.usize(6) {
                0 => {
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    let _ = world.begin_cast(caster, &spec);
                    let mut inputs = BTreeMap::new();
                    let input = CastInput { held: r.usize(3) == 0, dodge: false, poise_broken: false, magicka_current: 500.0 };
                    inputs.insert(caster, input);
                    world.step(1.0 / 60.0, &inputs);
                }
                1 | 2 => {
                    let target = targets[r.usize(targets.len())];
                    if target < 900 {
                        if world.state(target).is_some() && !world.is_alive(target) {
                            let _ = world.respawn(target);
                        } else if world.is_alive(target) {
                            let _ = world.kill(target);
                        }
                    }
                }
                3 => {
                    let target = targets[r.usize(targets.len())];
                    if target < 900 {
                        let _ = world.unload(target);
                        let _ = world.spawn(target, rules());
                    }
                }
                4 => {
                    let mut inputs = BTreeMap::new();
                    for _ in 0..r.usize(3) {
                        inputs.insert(targets[r.usize(targets.len())], gen_input(&mut r));
                    }
                    world.step(gen_dt(&mut r), &inputs);
                }
                _ => {
                    for _ in 0..2 {
                        random_impact(&mut r, &mut world, &targets);
                    }
                }
            }
            // The ledger alone enforces exactly-once resolution: count every
            // (release, target, tick) among the newly appended events.
            let events = world.events();
            for e in &events[scanned..] {
                if let MagicEvent::ImpactResolved { tick, release_id, target, .. } = e {
                    *seen.entry((*release_id, *target, *tick)).or_default() += 1;
                }
            }
            scanned = events.len();
        }
        for ((release, target, tick), count) in &seen {
            assert!(
                *count <= 1,
                "seed {seed} case {case}: release {release} resolved target {target} tick {tick} {count} times"
            );
        }
        for e in world.events() {
            c.note(e);
        }
    }
    c.assert_common("one_resolution_per_target_and_tick", seed);
    // Per-reason floors: the deterministic probes guarantee these on every
    // case; the floors turn a generator regression into a failure.
    assert!(c.refuse_ledger_duplicate >= 100, "only {} duplicate refusals (seed {seed})", c.refuse_ledger_duplicate);
    assert!(c.refuse_ledger_tick >= 100, "only {} tick-out-of-range refusals (seed {seed})", c.refuse_ledger_tick);
    assert!(c.refuse_target_dead >= 100, "only {} dead-target refusals (seed {seed})", c.refuse_target_dead);
    assert!(c.refuse_unknown_target >= 100, "only {} unknown-target refusals (seed {seed})", c.refuse_unknown_target);
    assert!(c.kills >= 100, "only {} kills (seed {seed})", c.kills);
    assert!(c.unloads >= 100, "only {} unloads (seed {seed})", c.unloads);
}

// ============================================================================================
// 5. dead_actors_take_nothing
//
// The generator aims REAL releases at actors that are dead at the moment of
// impact, so the property observes a live release resolving against a dead
// actor being refused — not only fabricated ids against anything. Each case
// also begins a cast on a possibly-dead actor without checking aliveness
// first: the world (not the test) must refuse it.
// ============================================================================================

#[test]
fn dead_actors_take_nothing() {
    let seed = 0xDEAD_BEEF;
    let mut c = Coverage::default();
    for case in 0..400u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x6D2B_79F5));
        let mut world = MagicWorld::new();
        let actors = [1u32, 2, 3];
        let targets = [1u32, 2, 3, GHOST_TARGET];
        for a in actors {
            let _ = world.spawn(a, rules());
        }
        // Deterministic probe: a live projectile release, then a live-target
        // resolution, then the same release aimed at a fresh corpse.
        if !world.is_alive(1) {
            let _ = world.respawn(1);
        }
        drop(cast_and_release(&mut world, 1, &staff_spec(slice_spell(100, 1))));
        impact_real_release_on_live_and_dead(&mut world, 1, 2);
        let mut scanned = 0usize;
        let mut dead: BTreeSet<u32> = BTreeSet::new();
        for _ in 0..40 {
            let actor = actors[r.usize(3)];
            match r.usize(7) {
                0 => {
                    if world.is_alive(actor) {
                        let _ = world.kill(actor);
                    }
                }
                1 => {
                    if world.state(actor).is_some() && !world.is_alive(actor) {
                        let _ = world.respawn(actor);
                    }
                }
                2 => {
                    if world.is_alive(actor) {
                        let id = 100 + r.u32() % 7;
                        // Half the casts are projectiles, so a live release is
                        // almost always in flight for the impact half.
                        let spec = if r.usize(2) == 0 {
                            staff_spec(slice_spell(id, 1))
                        } else if r.usize(2) == 0 {
                            sane_spec(id)
                        } else {
                            gen_spec(&mut r, id)
                        };
                        drop(cast_and_release(&mut world, actor, &spec));
                    }
                }
                3 => {
                    // No aliveness check: a dead actor must be refused by the
                    // world, and the log must never show it starting a cast.
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    let _ = world.begin_cast(actor, &spec);
                }
                4 => {
                    let mut inputs = BTreeMap::new();
                    inputs.insert(actor, gen_input(&mut r));
                    world.step(gen_dt(&mut r), &inputs);
                }
                5 => {
                    let target = actors[r.usize(3)];
                    let _ = world.unload(target);
                    let _ = world.spawn(target, rules());
                }
                _ => {
                    for _ in 0..2 {
                        random_impact(&mut r, &mut world, &targets);
                    }
                    impact_real_release_on_live_and_dead(&mut world, actors[r.usize(3)], actors[r.usize(3)]);
                }
            }
            // Walk the newly appended events maintaining a dead-set: while
            // dead, an actor must neither take an ImpactResolved nor start a
            // Cast.
            let events = world.events();
            for e in &events[scanned..] {
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
            scanned = events.len();
        }
        for e in world.events() {
            c.note(e);
        }
    }
    c.assert_common("dead_actors_take_nothing", seed);
    // The point of THIS property: dead targets really got aimed at, and the
    // world refused those impacts before the release was consumed.
    assert!(c.refuse_target_dead >= 400, "only {} dead-target refusals: the property never saw a dead target (seed {seed})", c.refuse_target_dead);
    assert!(c.kills >= 400, "only {} kills (seed {seed})", c.kills);
    assert!(c.respawns >= 100, "only {} respawns (seed {seed})", c.respawns);
}

// ============================================================================================
// 6. events_are_ordered
//
// Case 0 runs 300 consecutive steps at a fixed positive dt, so the tick
// counter sweeps well past any small modulus: a tick that wraps or resets
// makes event ticks decrease (and tick_count collapse).
// ============================================================================================

#[test]
fn events_are_ordered() {
    let seed = 0x05D3_0001;
    for case in 0..1100u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x2722_0A95));
        let mut world = MagicWorld::new();
        let long = case == 0;
        for _ in 0..(if long { 300 } else { 40 }) {
            match r.usize(6) {
                0 => {
                    let _ = world.spawn(r.u32(), rules());
                }
                1 => {
                    let _ = world.unload(r.u32());
                }
                2 => {
                    let _ = world.kill(r.u32());
                }
                3 => {
                    let _ = world.respawn(r.u32());
                }
                4 => {
                    let actor = r.u32();
                    let _ = world.begin_cast(actor, &gen_spec(&mut r, actor));
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
            if long {
                // An unconditional sane step: the tick counter must climb once
                // per step, every step, for the whole run.
                let mut inputs = BTreeMap::new();
                inputs.insert(1u32, CastInput { held: false, dodge: false, poise_broken: false, magicka_current: 500.0 });
                world.step(1.0 / 60.0, &inputs);
            }
        }
        let _ = world.native_impact(r.next_u64(), r.u32(), r.u32());
        if long {
            assert!(world.tick_count() >= 300, "seed {seed} case {case}: tick_count {} after 300 steps", world.tick_count());
        }
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
            assert!(tick <= world.tick_count(), "seed {seed} case {case}: event tick {tick} beyond tick_count {}", world.tick_count());
            last = tick;
        }
    }
}

// ============================================================================================
// 7. ledger_is_bounded
//
// Fills the ledger past the bound on purpose: 96+ casters all release in the
// same few ticks so more than MAX_TRACKED_RELEASES releases are in flight at
// once. Asserts the bound was REACHED (ReleaseUntracked(Full) at least once,
// and at least n - MAX of them) and never passed.
// ============================================================================================

#[test]
fn ledger_is_bounded() {
    let seed: u64 = 0xB0D9_00D5;
    for case in 0..20u32 {
        let n: usize = 96 + case as usize;
        let mut world = MagicWorld::new();
        for actor in 1..=n as u32 {
            let _ = world.spawn(actor, rules());
        }
        // Everyone starts a spell in the same few ticks. Every spell is made
        // castable from both families and cheap, so every cast starts, and no
        // spell is a self-cast, so every release occupies a ledger slot.
        for actor in 1..=n as u32 {
            let mut spell = slice_spell(1000 + actor, (actor as usize) % all_spells().len());
            if spell.delivery == SpellDelivery::SelfCast {
                spell = slice_spell(1000 + actor, 0);
            }
            spell.castable_from = FROM_STAFF | FROM_RING;
            spell.cost_fraction = 0.01;
            let spec = CastSpec {
                hand: Hand::Left,
                casting: if actor % 3 == 0 { sages_staff() } else { apprentice_staff() },
                offhand: if actor % 5 == 0 { Some(sun_ring()) } else { None },
                spell,
                skill: 100.0,
                magicka_current: 500.0,
                magicka_max: 500.0,
            };
            match world.begin_cast(actor, &spec) {
                CastStart::Started => {}
                other => panic!("seed {seed} case {case}: actor {actor} cast did not start: {other:?}"),
            }
        }
        // Step far enough for every release frame (0.45s..0.9s) to pass while
        // every release is still tracked (projectiles 10s, areas 6s, instants
        // 0.5s — the instants outlive the last registration).
        for _ in 0..70 {
            let mut inputs = BTreeMap::new();
            for actor in 1..=n as u32 {
                inputs.insert(actor, CastInput { held: false, dodge: false, poise_broken: false, magicka_current: 500.0 });
            }
            world.step(0.05, &inputs);
            assert!(
                world.releases_in_flight() <= MAX_TRACKED_RELEASES,
                "seed {seed} case {case}: {} releases in flight > MAX_TRACKED_RELEASES at tick {}",
                world.releases_in_flight(),
                world.tick_count()
            );
        }
        // Every caster released exactly once; the bounded table must have
        // refused everything past the bound.
        let released: usize = world
            .events()
            .iter()
            .filter(|e| matches!(e, MagicEvent::SpellReleased { .. }))
            .count();
        assert_eq!(released, n as usize, "seed {seed} case {case}: {released} releases from {n} casters");
        let full_events: usize = world
            .events()
            .iter()
            .filter(|e| matches!(e, MagicEvent::ReleaseUntracked { reason: Registration::Full, .. }))
            .count();
        assert!(
            full_events >= n - MAX_TRACKED_RELEASES,
            "seed {seed} case {case}: only {full_events} Full refusals for {n} simultaneous releases; the bound was never reached"
        );
        assert!(
            world.releases_in_flight() <= MAX_TRACKED_RELEASES,
            "seed {seed} case {case}: {} releases in flight > MAX_TRACKED_RELEASES",
            world.releases_in_flight()
        );
        assert!(world.releases_in_flight() >= 1, "seed {seed} case {case}: nothing in flight; the bound was never stressed");
    }
}

// ============================================================================================
// 8. multiplier_bounds
//
// Includes cases built to exceed the cap on every axis at once (max quality,
// max upgrade level, max passive on both hands, max charge, max power_ratio,
// skill 100) and asserts the cap is reached but never passed.
// ============================================================================================

/// The most powerful legal cast the rules allow, on every axis at once:
/// power_ratio 1.5 × scaling 2.0 (capped) × stacked passives 2.0 × charge 2.0
/// = 12.0 raw, which only the damage-multiplier cap can hold at 2.5.
fn maxed_spec() -> CastSpec {
    let mut spell = all_spells()[6].clone(); // solar lance: charged, projectile
    spell.id = 107;
    spell.power_ratio = MAX_POWER_RATIO;
    spell.castable_from = FROM_STAFF | FROM_RING;
    spell.charge = Some(Charge { max_seconds: 2.0, power_at_full: MAX_CHARGE_POWER });
    spell.cost_fraction = 0.05;
    let family = spell.family;
    let casting = Catalyst {
        id: 11,
        kind: CatalystKind::Staff,
        school: School::Destruction,
        quality: MAX_QUALITY,
        upgrade_level: MAX_UPGRADE_LEVEL,
        passive: Some(Passive { family, bonus: MAX_PASSIVE_BONUS }),
        cost_multiplier: 1.0,
    };
    let offhand = Catalyst {
        id: 12,
        kind: CatalystKind::Ring,
        school: School::Restoration,
        quality: MAX_QUALITY,
        upgrade_level: MAX_UPGRADE_LEVEL,
        passive: Some(Passive { family, bonus: MAX_PASSIVE_BONUS }),
        cost_multiplier: 1.0,
    };
    CastSpec { hand: Hand::Left, casting, offhand: Some(offhand), spell, skill: 100.0, magicka_current: 500.0, magicka_max: 500.0 }
}

#[test]
fn multiplier_bounds() {
    let seed = 0x9E37_79B9;
    let mut cap_hits = 0u64;
    let mut released_count = 0u64;
    // Part 1: the maxed build, uncharged and fully charged, must hit the cap
    // exactly and never past it.
    {
        let mut world = MagicWorld::new();
        world.spawn(42, rules()).unwrap();
        let spec = maxed_spec();
        drop(cast_and_release(&mut world, 42, &spec)); // uncharged release
        drop(cast_and_release(&mut world, 42, &spec)); // charged: hold to full
        let multipliers: Vec<f32> = world
            .events()
            .iter()
            .filter_map(|e| match e {
                MagicEvent::SpellReleased { damage_multiplier, .. } => Some(*damage_multiplier),
                _ => None,
            })
            .collect();
        assert!(multipliers.len() >= 2, "the maxed case must release at least twice: {multipliers:?}");
        assert!(
            multipliers.iter().any(|m| (*m - MAX_SPELL_DAMAGE_MULTIPLIER).abs() < 1e-4),
            "the maxed build must reach the cap {MAX_SPELL_DAMAGE_MULTIPLIER} exactly; got {multipliers:?}"
        );
        for m in &multipliers {
            assert!(
                m.is_finite() && (0.0..=MAX_SPELL_DAMAGE_MULTIPLIER).contains(m),
                "maxed build produced multiplier {m} outside [0, {MAX_SPELL_DAMAGE_MULTIPLIER}]"
            );
        }
        cap_hits += 1;
    }
    // Part 2: the adversarial sweep, one cast in four built to exceed the cap,
    // each cast stepped through to its release frame.
    for case in 0..500u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x85EB_CA6B));
        let mut world = MagicWorld::new();
        let caster = 9u32;
        world.spawn(caster, rules()).unwrap();
        for _ in 0..30 {
            let id = 100 + r.u32() % 7;
            let spec = if r.usize(4) == 0 {
                maxed_spec()
            } else if r.usize(3) == 0 {
                sane_spec(id)
            } else {
                gen_spec(&mut r, id)
            };
            let _ = world.begin_cast(caster, &spec);
            for _ in 0..12 {
                let mut inputs = BTreeMap::new();
                let mut input = gen_input(&mut r);
                input.magicka_current = 500.0;
                inputs.insert(caster, input);
                world.step(0.1, &inputs);
            }
        }
        for e in world.events() {
            match e {
                MagicEvent::SpellReleased { damage_multiplier, .. } => {
                    released_count += 1;
                    assert!(
                        damage_multiplier.is_finite()
                            && (0.0..=MAX_SPELL_DAMAGE_MULTIPLIER).contains(damage_multiplier),
                        "seed {seed} case {case}: SpellReleased multiplier {damage_multiplier} out of range"
                    );
                    if (*damage_multiplier - MAX_SPELL_DAMAGE_MULTIPLIER).abs() < 1e-4 {
                        cap_hits += 1;
                    }
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
    assert!(released_count >= 400, "generator degenerate: only {released_count} releases over the run");
    assert!(cap_hits >= 3, "the cap was reached only {cap_hits} times: the bound is not exercised (seed {seed})");
}

// ============================================================================================
// 8b. multiplier_function_is_the_bound
//
// `MagicWorld` re-clamps every release at SpellReleased time (magic_world::step),
// so the cap would hold even if `spell_damage_multiplier` itself stopped
// clamping - and a world-only test could never see it. This property exercises
// the public function directly: over a randomized sweep of hostile inputs AND
// the maxed build, the result must hit exactly `MAX_SPELL_DAMAGE_MULTIPLIER`
// for the maxed inputs and never exceed it, and every negative/hostile input
// must collapse to 0 (no NaN, no negative).
// ============================================================================================

#[test]
fn multiplier_function_is_the_bound() {
    let seed = 0xCAFE_B0BA;
    let spell_family = all_spells()[6].family;
    let maxed_spell = {
        let mut s = all_spells()[6].clone();
        s.power_ratio = MAX_POWER_RATIO;
        s.charge = Some(Charge { max_seconds: 2.0, power_at_full: MAX_CHARGE_POWER });
        s
    };
    let maxed_casting = Catalyst {
        id: 21,
        kind: CatalystKind::Staff,
        school: School::Destruction,
        quality: MAX_QUALITY,
        upgrade_level: MAX_UPGRADE_LEVEL,
        passive: Some(Passive { family: spell_family, bonus: MAX_PASSIVE_BONUS }),
        cost_multiplier: 1.0,
    };
    let maxed_offhand = Catalyst {
        id: 22,
        kind: CatalystKind::Ring,
        school: School::Restoration,
        quality: MAX_QUALITY,
        upgrade_level: MAX_UPGRADE_LEVEL,
        passive: Some(Passive { family: spell_family, bonus: MAX_PASSIVE_BONUS }),
        cost_multiplier: 1.0,
    };
    // The maxed inputs MUST land exactly on the cap: raw
    // 1.5 x 2.0 x 2.0 x 2.0 = 12.0, held to 2.5 only by the clamp.
    let at_cap = spell_damage_multiplier(&maxed_spell, &maxed_casting, Some(&maxed_offhand), 100.0, MAX_CHARGE_POWER);
    assert_eq!(
        at_cap, MAX_SPELL_DAMAGE_MULTIPLIER,
        "the maxed build must sit exactly on the cap; got {at_cap}",
    );
    // Now the hostile sweep. The function's documented input is a validated
    // definition and valid catalysts - the world validates both before ever
    // calling it (begin_cast refuses NotFinite/InvalidCatalyst) - so the sweep
    // draws valid definitions/catalysts and hostile skill/charge numbers,
    // which the function itself must sanitize. No input may escape [0, cap].
    let mut cap_hits = 0u64;
    let mut zero_hits = 0u64;
    let mut valid_cases = 0u64;
    for case in 0..2000u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0x1B873_A59));
        let spell = match r.usize(4) {
            0 | 1 => maxed_spell.clone(),
            2 => {
                // A clean slice spell: two of the seven carry power_ratio 0.0,
                // so the zero floor is exercised too.
                let mut s = all_spells()[r.usize(all_spells().len())].clone();
                s.id = 300;
                s
            }
            _ => gen_spell(&mut r, 300),
        };
        let casting = if r.usize(2) == 0 { maxed_casting } else { gen_catalyst(&mut r, 23) };
        let offhand = match r.usize(3) {
            0 => None,
            1 => Some(maxed_offhand),
            _ => Some(gen_catalyst(&mut r, 24)),
        };
        let skill = match r.usize(3) {
            0 => 100.0,
            1 => r.f32_any(),
            _ => r.f32_range(0.0, 100.0),
        };
        let charge = match r.usize(3) {
            0 => MAX_CHARGE_POWER,
            1 => r.f32_any(),
            _ => r.f32_range(1.0, MAX_CHARGE_POWER),
        };
        // Only validated inputs are in the function's contract.
        if !spell.validate().is_ok() || !casting.valid() || !offhand.map_or(true, |c| c.valid()) {
            continue;
        }
        valid_cases += 1;
        let value = spell_damage_multiplier(&spell, &casting, offhand.as_ref(), skill, charge);
        assert!(
            value.is_finite() && (0.0..=MAX_SPELL_DAMAGE_MULTIPLIER).contains(&value),
            "seed {seed} case {case}: multiplier {value} escaped [0, {MAX_SPELL_DAMAGE_MULTIPLIER}]"
        );
        if value == MAX_SPELL_DAMAGE_MULTIPLIER {
            cap_hits += 1;
        }
        if value == 0.0 {
            zero_hits += 1;
        }
    }
    assert!(valid_cases >= 800, "only {valid_cases} validated cases in the sweep (seed {seed})");
    assert!(cap_hits >= 300, "the cap was reached only {cap_hits} times over the sweep (seed {seed})");
    assert!(zero_hits >= 100, "the zero floor was reached only {zero_hits} times (seed {seed})");
}

// ============================================================================================
// 9. replay_matches

//
// Recorded fights include kill/respawn/unload and REAL-release impacts aimed
// at targets whose life state changes mid-flight; the test asserts those steps
// actually appear in the script and the live log, so the replay property
// cannot pass on a script that never kills anyone.
// ============================================================================================

#[test]
fn replay_matches() {
    let seed = 0x5EED_5EED;
    let mut exercised = 0u32;
    let mut scripts_with_kill = 0u32;
    let mut scripts_with_respawn = 0u32;
    let mut scripts_with_unload = 0u32;
    let mut scripts_with_impact = 0u32;
    for case in 0..400u32 {
        let mut r = Xs::new(seed ^ u64::from(case).wrapping_mul(0xC2B2_AE3D));
        let mut rec = MagicRecorder::new();
        for a in [1u32, 2, 3] {
            let _ = rec.spawn(a, rules());
        }
        let targets = [1u32, 2, 3, GHOST_TARGET];
        for _ in 0..30 {
            let actor = 1 + r.usize(3) as u32;
            match r.usize(10) {
                0 => {
                    let _ = rec.unload(actor);
                    let _ = rec.spawn(actor, rules());
                }
                1 => {
                    if rec.world().is_alive(actor) {
                        let _ = rec.kill(actor);
                    }
                }
                2 => {
                    if rec.world().state(actor).is_some() && !rec.world().is_alive(actor) {
                        let _ = rec.respawn(actor);
                    }
                }
                3 | 4 => {
                    let id = 100 + r.u32() % 7;
                    let spec = if r.usize(2) == 0 { sane_spec(id) } else { gen_spec(&mut r, id) };
                    let _ = rec.begin_cast(actor, &spec);
                    let mut inputs = BTreeMap::new();
                    let mut input = gen_input(&mut r);
                    input.magicka_current = 500.0;
                    inputs.insert(actor, input);
                    rec.step(1.0 / 60.0, &inputs);
                }
                5 => {
                    let mut inputs = BTreeMap::new();
                    for _ in 0..r.usize(3) {
                        inputs.insert(1 + r.usize(3) as u32, gen_input(&mut r));
                    }
                    rec.step(gen_dt(&mut r), &inputs);
                }
                6 => {
                    let mut inputs = BTreeMap::new();
                    inputs.insert(actor, CastInput { held: false, dodge: false, poise_broken: false, magicka_current: 500.0 });
                    rec.step(0.2, &inputs);
                }
                _ => {
                    // Aim at REAL release ids (biased to the newest) against
                    // targets whose life state changes mid-flight, with tick
                    // indexes that repeat and go out of range.
                    let target = targets[r.usize(targets.len())];
                    let real = real_release_ids(rec.world().events());
                    let release = if !real.is_empty() && r.usize(2) != 0 {
                        let back = 1 + r.usize(real.len().min(4));
                        real[real.len() - back]
                    } else {
                        r.next_u64() | 1
                    };
                    let tick = match r.usize(4) {
                        0 | 1 => 0,
                        2 => r.u32(),
                        _ => u32::MAX,
                    };
                    if target < 900 && r.usize(3) == 0 && rec.world().is_alive(target) {
                        let _ = rec.kill(target);
                    }
                    let _ = rec.native_impact(release, target, tick);
                }
            }
        }
        let _ = rec.take_magicka_spent(r.u32());
        let (script, live) = rec.finish();
        // Non-vacuousness: the script must actually contain the life-cycle
        // steps the property is about, and the live log must show them took
        // effect (a Kill step on an unknown actor proves nothing).
        let has_kill = script.steps().iter().any(|s| matches!(s, ScriptStep::Kill { .. }))
            && live.iter().any(|e| matches!(e, MagicEvent::ActorDied { .. }));
        let has_respawn = script.steps().iter().any(|s| matches!(s, ScriptStep::Respawn { .. }))
            && live.iter().any(|e| matches!(e, MagicEvent::ActorRespawned { .. }));
        let has_unload = script.steps().iter().any(|s| matches!(s, ScriptStep::Unload { .. }))
            && live.iter().any(|e| matches!(e, MagicEvent::ActorUnloaded { .. }));
        let has_impact = script.steps().iter().any(|s| matches!(s, ScriptStep::Impact { .. }))
            && live.iter().any(|e| matches!(e, MagicEvent::ImpactResolved { .. } | MagicEvent::ImpactRefused { .. }));
        if has_kill {
            scripts_with_kill += 1;
        }
        if has_respawn {
            scripts_with_respawn += 1;
        }
        if has_unload {
            scripts_with_unload += 1;
        }
        if has_impact {
            scripts_with_impact += 1;
        }
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
    assert!(exercised > 300, "generator degenerate: only {exercised} cases produced events; seed {seed}");
    assert!(scripts_with_kill >= 300, "only {scripts_with_kill} recorded fights contained a real kill (seed {seed})");
    assert!(scripts_with_respawn >= 100, "only {scripts_with_respawn} recorded fights contained a respawn (seed {seed})");
    assert!(scripts_with_unload >= 100, "only {scripts_with_unload} recorded fights contained an unload (seed {seed})");
    assert!(scripts_with_impact >= 300, "only {scripts_with_impact} recorded fights contained an impact (seed {seed})");
}