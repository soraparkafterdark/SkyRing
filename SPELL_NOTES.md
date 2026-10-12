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
