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

## Traceability

| Rule ID      | Document                  | Schema field       |
|--------------|---------------------------|--------------------|
| `ir-v1-locked` | `schemas/ir/migrations.md` | `schemas/ir/v1.json` (`migration.rule`) |
