# SPELL_NOTES

> **Resolved by Claude after review (see the commit that follows the merge).** Note 1 was right:
> `docs/Magic overhaul.md` line 19 says "Two catalysts, passives stack", and `passive_bonus` now sums
> same-family bonuses (different-family passives were already applied per family, so that part of the
> note was not a defect). Note 2 is now stated in the `School` doc comment. Note 3 is fixed by
> `ImpactLedger::try_register`, which returns `Registration` (`MagicWorld` logs `ReleaseUntracked`
> when a release cannot be tracked); `register` stays as a yes/no wrapper. The original notes follow.

Problems observed in the committed magic core while building the first spell
matrix (Slice D). Per the tasking, `core/src/magic.rs` and `core/src/cast.rs`
were **not** modified; the spell matrix in `core/src/spells.rs` and
`core/tests/spell_matrix.rs` works around each item by construction.

## 1. Off-hand passives take the strongest instead of stacking (Charter conflict)

`magic.rs::passive_bonus` folds the casting hand's passive and the off hand's
passive with `f32::max` — "the strongest passive that applies". But
`docs/Charter.md` decides "dual-wield casting: two catalysts, **passives
stack**" (the Elden Ring powerstance), and `docs/Magic overhaul.md` §140 spells
out the intended shape: casting hand supplies Scaling, the other hand supplies
its passive, and two rings with different passives produce a meaningfully
different build (Charter phase 7 acceptance).

Consequence: a player powerstancing two rings from *different* families gets
only whichever bonus is larger rather than both, so "two rings with different
passives" is not yet meaningfully different from one ring. With two passives on
the *same* family, stacking and strongest also differ whenever `1 + a + b`
differs from `1 + max(a, b)` — for the slice's 0.25 that is 1.25 vs 1.5625.

Suggested fix (not applied): sum the passive bonuses for distinct families and
decide the same-family rule explicitly (stack or strongest) in one place, with
a bound (`MAX_PASSIVE_BONUS` plus a documented stacking cap) so dual-casting
cannot exceed the overall multiplier cap silently. Regression test: cast one
spell from two catalysts with different-family passives and assert the
multiplier reflects both.

## 2. Minor: `SpellDefinition::validate` does not re-check `School`

`Catalyst::valid` constrains quality/upgrade/cost/passive but nothing checks
that a spell's `School` is one the *catalyst* could govern — by design skill
never gates anything, so this is arguably correct, but it is worth stating
explicitly in a doc comment that `School` is descriptive only, because a future
reader will look for the constraint that is deliberately not there.

## 3. Minor: `ImpactLedger::register` drops a self-cast silently

`register` returns `false` for a `SelfCast`, indistinguishable from a full
table or a duplicate release id. The bridge cannot tell "nothing to resolve"
from "the ledger refused your release". A three-state return (`Registered`,
`NotApplicable`, `Refused`) or an `Option`/`Result` would make the intended
self-cast path loud instead of silent. The matrix tests the distinction by
asserting on the delivery kind, so the behaviour is exercised, but the API
shape is the defect.

## Adversarial findings

Adversarial property sweep on branch `claude/magic-adversarial`
(`core/tests/magic_adversarial.rs`, added 2025). Nine properties, each driven by
an in-file xorshift64 generator with fixed seeds, 2000–2500 generated cases per
property, public API only (`er_combat_core::...`).

**Every property passed. No library finding.**

- 1. no_panics_on_hostile_input — PASS (2500 cases; NaN/±inf/negative/huge/zero
  catalysts, spells, pools, dt and inputs; spawn/unload/kill/respawn/begin_cast/
  step/native_impact in random order; refusals, accepted casts and resolved
  impacts all exercised).
- 2. magicka_accounting — PASS (2200 cases; MagickaSpent event sum equals
  take_magicka_spent within 1e-3 relative, never negative, never above the sum
  of released cast costs).
- 3. release_ids_unique — PASS (2200 cases; spawn/unload/kill/respawn/cast
  sequences produce no duplicate SpellReleased release_id).
- 4. one_resolution_per_target_and_tick — PASS (2200 cases; no
  (release_id, target, tick-index) resolved more than once).
- 5. dead_actors_take_nothing — PASS (2200 cases; no ImpactResolved targeting a
  dead actor, no CastStarted by a dead actor anywhere in the log).
- 6. events_are_ordered — PASS (2200 cases; event tick never decreases).
- 7. ledger_is_bounded — PASS (2200 cases; releases_in_flight() never exceeds
  cast::MAX_TRACKED_RELEASES).
- 8. multiplier_bounds — PASS (2200 cases; SpellReleased multiplier finite and
  within [0, magic::MAX_SPELL_DAMAGE_MULTIPLIER]; ImpactResolved multiplier
  finite and >= 0).
- 9. replay_matches — PASS (2100 cases; MagicScript::replay log equals the live
  log and canonical_event_bytes of both are identical).

One earlier suspicion was investigated and dismissed as *not* a library defect:
an Instant release that occurs during a huge-dt `step` (e.g. dt = 1e30) is
registered in the ledger and then legitimately aged out by the same step's
`ledger.tick(dt)`, so a subsequent `native_impact` on that release_id returns
`Ledger(Unknown)`. This is the documented expiry behaviour ("a miss or a world
impact never reaches resolve, so it expires without consequence"), reproducible
by design, and the test was adjusted to impact releases on the tick they are
produced.

## Adversarial findings

Adversarial property sweep on branch `claude/magic-adversarial`
(`core/tests/magic_adversarial.rs`, added 2025). Nine properties, each driven by
an in-file xorshift64 generator with fixed seeds, 2000–2500 generated cases per
property, public API only (`er_combat_core::...`).

**Every property passed. No library finding.**

- 1. no_panics_on_hostile_input — PASS (2500 cases; NaN/±inf/negative/huge/zero
  catalysts, spells, pools, dt and inputs; spawn/unload/kill/respawn/begin_cast/
  step/native_impact in random order; refusals, accepted casts and resolved
  impacts all exercised).
- 2. magicka_accounting — PASS (2200 cases; MagickaSpent event sum equals
  take_magicka_spent within 1e-3 relative; never negative; never more than the
  sum of the released casts' costs).
- 3. release_ids_unique — PASS (2200 cases; no release_id minted twice, across
  unloads and respawns).
- 4. one_resolution_per_target_and_tick — PASS (2200 cases; no
  (release_id, target, tick-index) resolved more than once).
- 5. dead_actors_take_nothing — PASS (2200 cases; no ImpactResolved targeting a
  dead actor, no CastStarted by a dead actor anywhere in the log).
- 6. events_are_ordered — PASS (2200 cases; event tick never decreases).
- 7. ledger_is_bounded — PASS (2200 cases; releases_in_flight() never exceeds
  cast::MAX_TRACKED_RELEASES).
- 8. multiplier_bounds — PASS (2200 cases; SpellReleased multiplier finite and
  within [0, magic::MAX_SPELL_DAMAGE_MULTIPLIER]; ImpactResolved multiplier
  finite and >= 0).
- 9. replay_matches — PASS (2100 cases; MagicScript::replay log equals the live
  log and canonical_event_bytes of both are identical).

One earlier suspicion was investigated and dismissed as *not* a library defect:
an Instant release that occurs during a huge-dt `step` (e.g. dt = 1e30) is
registered in the ledger and then legitimately aged out by the same step's
`ledger.tick(dt)`, so a subsequent `native_impact` on that release_id returns
`Ledger(Unknown)`. This is the documented expiry behaviour ("a miss or a world
impact never reaches resolve, so it expires without consequence"), reproducible
by design, and the test was adjusted to impact releases on the tick they are
produced.

## Mutation audit of the adversarial sweep (follow-up)

The sweep above was re-audited with mutation testing: the library was broken on
purpose, one bug at a time (`tools/mutate_magic.py`, applied to a scratch copy;
this repository is never modified), and the harness checked whether
`cargo test --test magic_adversarial` noticed. The first version of the sweep
**failed the audit**: 8 of 10 mutants SURVIVED. The properties could not fail
because their generators never built the situations the properties were about:
impacts were aimed at fabricated release ids, so no live release ever resolved
against a dead or unknown target; the ledger was never filled to
`MAX_TRACKED_RELEASES`; no case reached the damage-multiplier cap; the tick
counter never passed a small modulus; and recorded fights contained no
kill/respawn steps worth replaying. A property that cannot fail proves nothing.

The sweep was rewritten to be load-bearing, in two ways:

1. **Coverage counters** (`struct Coverage` in the test file). Every property
   counts what its generator actually built — accepted casts, refused casts,
   released spells, resolved impacts, refused impacts broken out per reason
   (`UnknownTarget`, `TargetDead`, ledger `Unknown`/`Duplicate`/
   `TickOutOfRange`), `ReleaseUntracked(Full)` events, kills, respawns and
   unloads — and asserts each counter clears a floor (e.g. >= 200 resolved
   impacts and >= 200 refused impacts per property, >= 400 dead-target
   refusals in `dead_actors_take_nothing`). If the generator stops building
   the situation, the property fails on its coverage asserts instead of
   passing vacuously.

2. **Deterministic bound-reaching cases.** Impacts aim REAL release ids
   (harvested from `SpellReleased`) at alive, dead and unknown targets with
   repeating and out-of-range tick indexes; `ledger_is_bounded` spawns 96–115
   casters that all release in the same few ticks, asserts
   `releases_in_flight() <= MAX_TRACKED_RELEASES` after every step AND that the
   bound was reached (`ReleaseUntracked(Full)` at least `n - 64` times, and
   every caster released exactly once); `multiplier_bounds` includes a maxed
   build (power_ratio 1.5, quality 1.5, upgrade 10, stacked 0.5 passives on
   both hands, charge 2.0, skill 100 — raw 12.0) and asserts the cap is hit
   exactly and never passed; `events_are_ordered` includes a 300-step case so
   the tick counter sweeps past any small modulus, with every event tick also
   checked against `tick_count()`; `replay_matches` asserts the recorded
   scripts really contain kill/respawn/unload/impact steps that took effect in
   the live log; `one_resolution_per_target_and_tick` pins the ledger's edge
   cases (area tick at `area.ticks()` refused `TickOutOfRange`, repeat
   (release, target, tick) refused `Duplicate`, untracked self-cast release
   refused, unknown target refused twice without consuming the release, dead
   target refused `TargetDead` before the release is consumed).

Two library behaviours discovered by the audit (documented, not defects):

- **The cap is enforced twice.** `magic::spell_damage_multiplier` clamps to
  `MAX_SPELL_DAMAGE_MULTIPLIER`, and `magic_world::step` re-clamps every
  released snapshot to the same cap (its DamageDealt modifier path). Removing
  the clamp in the magic module is therefore invisible through the world: the
  world-only property could not fail. The sweep now also exercises the public
  function directly (`multiplier_function_is_the_bound`): with the maxed legal
  inputs the function must return exactly 2.5 (raw 12.0 held by the clamp
  alone), must never exceed the cap, and must return 0.0 for negative input.
  Its documented input contract is a *validated* definition and valid
  catalysts — the world validates both before calling it — so the sweep draws
  valid definitions and hostile skill/charge values, which the function itself
  must sanitize.
- **A self-cast release is never tracked**, so a `native_impact` aimed at its
  release id is refused `Ledger(Unknown)` (not `ImpactError::SelfCast`, which
  the ledger reserves for entries that were tracked before resolving).

Result of the re-audit (harness output):

```
dead-target-allowed                CAUGHT
dead-can-cast                      CAUGHT
duplicate-resolution               CAUGHT
ledger-unbounded                   CAUGHT
no-damage-bound                    CAUGHT
tick-goes-backwards                CAUGHT
replay-ignores-kills               CAUGHT
area-tick-unbounded                CAUGHT
refusal-consumes-release           CAUGHT
CONTROL-spend-doubled              CAUGHT

SURVIVORS: none
```

The `EQUIVALENT-*` controls in the harness are skipped by design: they assert
library-internal panics that the public API cannot observe. `cargo test
--all-targets` passes with the unmodified library.
