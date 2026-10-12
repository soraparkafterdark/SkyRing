# IR migrations

## Rules

### ir-v1-locked

**Rule identifier:** `ir-v1-locked`

**Scope:** v1 is the only accepted schema version.

**Policy:**

- The IR schema (`schemas/ir/v1.json`) is locked at `ir_version: 1`.
- A future v2 would declare a concrete, ordered set of field renames/removals
  and a migration tool before acceptance.
- No absolute source-game values (damage, HP, defence, scaling modifiers) may
  be committed anywhere in the document. The schema enforces this with
  `additionalProperties: false` at every level.
- Migration rule identifiers are concrete strings referenced by both the
  schema (`migration.rule`) and this document to ensure traceability.

### Additive records within v1: `catalysts` and `spells`

A v1 document may carry optional top-level `catalysts` and `spells` arrays. Adding optional
records does not rename or remove anything, so it stays inside `ir-v1-locked`; documents
without them are unchanged. They are validated by `tools/magic_ir.py`, called from
`tools/combat_ir.py`:

- spell power is `power_ratio`, a fraction of the casting catalyst's `quality`; the IR still
  forbids the keys `damage` and `scaling`, so no source-game absolute value can enter by name;
- a catalyst has a `quality`, an `upgrade_level`, a `cost_multiplier` and at most one
  `passive` (one family, one percentage); no other property adds power;
- no attribute, skill, level, perk or stat requirement exists in any record (including
  `intelligence` and `faith`), so a level-one character can use every catalyst and spell
  its family allows;
- the numeric bounds mirror `core/src/magic.rs`; `tools/tests/test_magic_ir.py` parses the
  Rust constants and fails if the two differ. Bounds the Charter does not fix are *tuning
  inputs* pending the playtest the magic design calls for.

Golden fixture: `fixtures/ir/v1/magic_matrix.json` (synthetic; three catalysts and the seven
cases of the first magic slice).

## Traceability

| Rule ID      | Document                  | Schema field       |
|--------------|---------------------------|--------------------|
| `ir-v1-locked` | `schemas/ir/migrations.md` | `schemas/ir/v1.json` (`migration.rule`) |
