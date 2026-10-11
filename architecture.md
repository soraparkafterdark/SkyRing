# SkyRing architecture

This document is the implementation contract. `Charter.md` owns product
decisions; this file turns those decisions into code boundaries and an order of
work. If they disagree, the Charter wins and this file must change.

## Product invariant

When the player is fighting, action commitment should read like Elden Ring.
When the player is progressing through the world, the game remains Skyrim.

That means SkyRing imports **timing and geometry**, then translates their
combat consequences into Skyrim's actor, progression, resistance, perk, save,
and damage systems. It does not run a second RPG underneath Skyrim.

## Current priority order

1. Reproducible core and plugin builds, including the CrossOver runtime target.
2. One complete melee vertical slice in Skyrim.
3. Action and armament balance translation.
4. Elemental channels and a shared buff/debuff/status pipeline.
5. Separate poise and stance systems.
6. Magic actions, catalysts, and spell delivery.
7. Breadth: additional weapon classes, spells, and compatibility work.
8. Megafauna, after the combat platform is stable.

Consumables remain animation-less. Throwables are the first item category that
may later introduce a committed use animation.

## Ownership boundary

| Concern | Authority | Notes |
| --- | --- | --- |
| Input buffering, action state, cancel windows | combat core | Deterministic at 60 Hz |
| I-frames, hit windows, root motion, sweep geometry | combat IR/core | Seconds and metres internally |
| World collision, targets, actors, animation graph | SKSE bridge/Skyrim | Never simulated in parallel |
| Base weapon damage, health, armour, difficulty | Skyrim | Use the normal hit path |
| Skills, perks, levels, saves, quests | Skyrim | No parallel character sheet |
| Action damage multiplier | SkyRing balance policy | Bounded translation, not ER attack rating |
| Armament quality bonus | SkyRing configuration | Applied once and capped |
| Elemental resistance and persistent effects | shared effect pipeline + Skyrim | One path for melee, magic, and throwables |
| Poise | SkyRing | Defensive interruption threshold |
| Stance | SkyRing | Offensive accumulated break meter |

The Rust core may say that a hit happened and describe its action multiplier,
geometry, impact, stance pressure, elements, and status buildup. It must not
send an Elden Ring-derived absolute health-damage number to Skyrim.

## Runtime shape

```text
Skyrim input and actor facts
          |
          v
  Rust combat core
  - action state
  - timing/cancels
  - movement intent
  - live hit geometry
          |
          v
  C ABI event contract
  - action multiplier
  - impact/stance pressure
  - elements/status payload
          |
          v
  SKSE bridge
  - character controller
  - actor queries
  - normal Skyrim hit path
  - effects/UI/logging
          |
          v
        Skyrim
```

The core can be tested natively on macOS. Skyrim loads Windows x86-64 DLLs,
so producing the deployable Rust core and SKSE plugin is a separate build
stage. The final acceptance environment is Skyrim under CrossOver on Apple
Silicon, not merely a successful native unit test.

## Combat IR

The intermediate representation (IR) is a versioned, engine-neutral record of
facts extracted or authored about an action. It sits between source extraction
and generated Rust/Skyrim artifacts.

The first schema only needs:

- stable action and hit identifiers;
- source game build and provenance (`extracted`, `estimated`, or `authored`);
- time in seconds;
- distance and geometry in metres;
- input, cancel, hit, invulnerability, and hyperarmour windows;
- root motion and weapon sweeps;
- source stamina cost and source motion/guard/stance values;
- elemental and status tags when present.

Absolute Elden Ring health damage does not belong in a Skyrim gameplay output.
Source values may exist in a private local IR as evidence, but the Skyrim
compiler must translate or refuse them according to the Charter.

Generated proprietary game data stays ignored and local. The repository may
contain schemas, converters, validators, and synthetic fixtures.

## Balance translation

Combat balance has three independent layers:

```text
Skyrim weapon value     action value       armament personality
base damage/perks   x   bounded multiplier x small capped bonus
```

Keeping them separate prevents a heavy attack, legendary weapon, status proc,
and global difficulty adjustment from each unknowingly importing the same
power increase.

### Action value

Actions spend different amounts of time, stamina, reach, safety, and
commitment. Damage alone cannot balance them. Every action profile is evaluated
on at least:

- damage per landed hit;
- time to contact and time to safety;
- sustainable damage per second;
- stamina efficiency;
- reach and tracking;
- poise/hyperarmour exposure;
- guard and stance pressure;
- status buildup per second.

Initial policy:

- Light attacks define the repeatable baseline.
- Heavy attacks win on burst, guard pressure, and stance pressure, not
  automatically on sustained damage per second.
- Charged heavies earn a large hit through long, interruptible commitment.
- Crouch pokes earn speed and reach; they must not also dominate raw damage,
  stance pressure, and stamina efficiency.
- Jump attacks pay for approach and recovery with burst/stance pressure.
- Guard counters receive high stance pressure because they require a
  successful defensive setup.
- Multi-hit and paired actions are budgeted across the entire action, not per
  individual contact.

Extracted motion values are evidence, not final Skyrim multipliers. Each action
role has an envelope. The translator clamps source values to that envelope,
and telemetry/playtesting adjusts the envelope rather than hand-patching every
weapon.

### Armament value and the fun bias

Imported armaments should feel desirable and slightly overpowered without
invalidating Skyrim's loot and progression.

- Ordinary translated equipment: no global bonus (`1.00x`).
- Exceptional imported armament: up to `1.05x` over a comparable Skyrim item.
- Legendary/signature armament: starts at `1.10x`, hard-capped at `1.15x`.
- The bonus is applied once, before resistance and difficulty.
- Unique skills and status access count against the same power budget. A
  weapon with a strong unique skill does not automatically also receive the
  full numeric bonus.
- No shipped configuration may exceed the cap without an explicit Charter
  change and a regression fixture demonstrating why.

“Comparable” means the same upgrade tier, weapon class, character skill, and
perk state. We do not compare a finished legendary weapon to an untempered
iron sword and call the resulting ratio balanced.

## Shared hit and effect pipeline

Every successful melee, spell, throwable, hazard, or creature contact creates
the same conceptual event:

```text
HitEvent
  source / target
  action / hit identity
  Skyrim base-damage request + action multiplier + armament bonus
  impact class
  poise pressure
  stance pressure
  elemental channels
  zero or more status-buildup contributions
  zero or more immediate modifiers
```

The effect evaluator then applies:

1. Skyrim damage, armour, resistance, perk, and difficulty handling.
2. Poise interruption.
3. Stance accumulation and possible break.
4. Elemental consequences.
5. Status buildup and possible proc.
6. Buff/debuff modifiers with explicit stacking rules.

This pipeline must exist before magic. A frost sword and a frost spell submit
the same frost buildup; only their delivery actions differ.

### Status state

Each actor/status pair tracks current buildup, threshold, resistance, decay
delay, decay rate, immunity, proc lockout, and source attribution. Bleed,
frostbite, poison, rot, sleep, and madness differ by their proc definitions,
not by having separate engines.

### Modifier state

A buff or debuff records source, affected stat, additive/multiplicative/override
operation, magnitude, duration, stacking group, refresh rule, and tags. The
stacking rule is data, never an accident of effect application order.

## Vertical slices and gates

### Slice A: melee contact

One weapon class and one test actor. Done only when input, movement, animation,
outgoing contact, incoming evasion/blocking, Skyrim damage, stamina, hit-stop,
and deterministic logs work in the CrossOver target.

### Slice B: consequences

The same weapon can apply one element, one buildup status, one timed debuff,
poise pressure, and stance pressure. Tests prove that all five are independent.

### Slice C: magic

Three catalysts and a small spell matrix exercise direct damage, a projectile,
a persistent area, a buff, a debuff, a status, and a charged cast. Magic may
only compose the action and effect systems established by A and B.

## Harness evidence

Every iteration should leave evidence that another run can compare:

- core unit and property tests;
- IR schema and golden-fixture validation;
- C/Rust ABI size and offset tests;
- timestamped action/event logs;
- deterministic scripted input traces;
- build manifests and source-data fingerprints;
- in-game capture for cases where animation blending must be judged visually.

The tuning harness should report per action: landed-hit damage, time to first
contact, time to neutral, DPS, stamina efficiency, stance per second, buildup
per second, and the ratio to the light-attack baseline. A value outside its
role envelope fails loudly; it is not silently accepted because it came from
source data.

### Verification profiles

`verification/contract.json` is the machine-readable completion contract and
`tools/verify.py` is its local runner. The profiles are deliberately not
interchangeable:

- `local` permits the synthetic action table and proves only host-testable
  plumbing and invariants;
- `source` requires the owner's generated action table and fingerprints it;
- `release` also requires Windows artifacts and CrossOver evidence tied to the
  exact commit and source fingerprint.

A lower profile must never be summarized as a higher one. Missing proprietary
data or an unavailable runtime is a visible failed gate, not a silent skip.
The complete colossal-sword criteria live in the contract's
`release_acceptance` list so an automated harness cannot redefine "done" as
"the unit tests passed."
