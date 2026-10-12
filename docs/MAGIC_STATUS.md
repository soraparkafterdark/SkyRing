# Magic: implementation status

Claim ceiling: **`local`**. Everything below is proven by host tests on macOS. Nothing here
has run inside Skyrim, and no `windows`, `source` or `release` claim is made.

## Ordering note

`AGENTS.md` puts magic after the shared ranged delivery proof (one bow, three arrows). Magic
was built first on the owner's explicit instruction. It composes only the pieces that already
exist (consequences, effects, posture, the trace format) and does **not** duplicate bow
delivery: when `core/src/delivery.rs` lands, projectile spells should move onto its release
identity and `ImpactLedger` should be unified with the bow's ledger. Until the bow slice is
proven, treat the projectile-spell path as provisional.

## What exists

| Module | What it does |
|---|---|
| `core/src/magic.rs` | `Catalyst` (staff or ring: a relative `quality`, an `upgrade_level`, at most one `Passive`, a `cost_multiplier`), the gear axis `Base x f(skill) x (1 + upgrade)`, `SpellDefinition` with named validation errors, `SpellSnapshot` and its conversion to the shared `HitEvent`. |
| `core/src/cast.rs` | `Caster`: a spell as a committed action (startup, cancel window, release, charge, hyperarmour, recovery, hand dedication, magicka read from the host and charged at the release frame). `ImpactLedger`: exactly-once correlation of a native impact to a release. |
| `core/src/magic_world.rs` | `MagicWorld`: casters, releases in flight and per-actor consequence state in one deterministic world; buffs and debuffs through the ordinary modifier set; actor death, unload and form reuse. |
| `core/src/magic_trace.rs` | `MagicRecorder` records every call that can change a `MagicWorld`; `MagicScript::replay` re-runs them on a fresh world; `canonical_event_bytes` is a versioned, field-exact byte form of the event log, so a runtime log and its headless replay compare as bytes. |
| `tools/magic_ir.py`, `fixtures/ir/v1/magic_matrix.json` | Catalysts and spells as optional IR records, with the Charter's rules enforced at the boundary. |

## Rules the tests pin (each one is mutation-checked)

- Spell power is a **ratio of the casting catalyst's quality**, never a damage number; the
  final multiplier is bounded and multiplies what Skyrim assigns. Skyrim keeps absolute
  magnitude, armour, resistance, perks and difficulty.
- **No gate of any kind.** Skill moves scaling inside a floor (a level-one character gets
  exactly the catalyst's quality) and never disables anything. The only thing that makes a
  spell unavailable to a catalyst is its family (staff or ring), and Destruction deliberately
  has spells on both. The IR rejects requirement keys anywhere, including Intelligence and Faith.
- A catalyst carries a quality and **at most one passive**. Passives apply from either hand,
  boost only their family, and do not stack (the stronger applies).
- Casting is an action: a dodge cancels only before `cancel_until`; after it the cast is
  committed; poise breaks interrupt outside hyperarmour and are carried through inside it;
  the hand is dedicated from startup through recovery.
- **Magicka is Skyrim's.** The core reads the pool, checks it again at the release frame
  (a drained pool fizzles, costing nothing), reports a monotonic spend, and states the
  regeneration rule (frozen in combat, normal outside) for Skyrim to apply.
- One effect pipeline: a debuff amplifies the next hit from any delivery until it expires; a
  buff changes the caster's next cast (`MagickaCost`, `CastSpeed`, `DamageDealt`); a cost buff
  cannot make casting free; statuses build, proc, lock out and decay exactly as for a sword.
- Releases are **snapshots**: changing staves mid-cast or mid-flight changes nothing.
- An impact resolves **once per target** (once per tick for an area). Unknown, stale,
  duplicate and out-of-range impacts are refused as diagnostics; a miss expires with no
  consequence; a dead or unknown target is refused before the release is consumed.
- A death clears every status, modifier and meter; nothing leaks across a respawn or a reused
  form id; release ids stay unique across an actor's unload while projectiles fly.
- The whole scripted fight is bit-for-bit deterministic.

## Tuning inputs (placeholders, not extracted or Charter-fixed)

The Charter fixes the shape of the gear axis, not its numbers. These bounds are conservative
placeholders for the playtest the design calls for ("play a mage from level 1 to 30"), and are
marked as such in `core/src/magic.rs`. They are mirrored in `tools/magic_ir.py`, and a test
compares the two so they cannot drift:

`MIN_QUALITY 0.5`, `MAX_QUALITY 1.5`, `MAX_SKILL_FACTOR 1.25`, `UPGRADE_STEP 0.04`,
`MAX_UPGRADE_LEVEL 10`, `MAX_SCALING 2.0`, `MAX_PASSIVE_BONUS 0.5`, cost multiplier `1.0..2.0`,
`MAX_POWER_RATIO 1.5`, `MAX_CHARGE_POWER 2.0`, `MAX_SPELL_DAMAGE_MULTIPLIER 2.5`,
`PROJECTILE_MAX_FLIGHT_SECONDS 10`, `INSTANT_RESOLVE_SECONDS 0.5`.

Not decided by this work, and still open in `docs/Magic overhaul.md`: what governs Scaling
beyond the school skill, enchanting's relationship to catalyst upgrades, and the second ring
slot that powerstancing depends on.

## Not done, and why

- **C ABI for casting and for consequence payloads.** `consequence-abi-state` depends on the
  owner-data balance task; magic's bridge calls need that payload first. No ABI revision was
  made for magic.
- **Skyrim integration** (reading magicka, launching projectiles, the native hit gateway,
  catalyst forms and staff-record replacement, animations, both perspectives): BLOCKED on a
  Windows build and a Skyrim runtime. `magic-runtime-slice` also depends on
  `catalyst-runtime-slice` and `projectile-adapter`.
- **Real spell and catalyst numbers**: BLOCKED on owner-extracted data. The fixtures are
  synthetic and say so.
- **Cast timings from the source game**: the ones used are estimates. Agreement between this
  code and its own fixtures is not extraction validation.
- **Persisting a magic session**: scripts and the event log replay in memory and the log has a
  canonical byte form, but scripts carry spell definitions, which have no wire format until the
  IR loader exists. `MagicEvent` is not yet part of the combat trace format.
- **The seven-case proof under CrossOver**, the gravity/Alteration build, perspective parity
  for casting.
- **Dragon tree, summoning, gravity sorceries, new spell content** (Charter phase 8).
