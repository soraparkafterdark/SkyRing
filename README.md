# Elden Ring combat for Skyrim

An in-progress SKSE integration for running Elden Ring-shaped player combat
rules inside Skyrim SE/AE. The host-testable core covers rolls on release,
sprint on hold, i-frames, attack chains, charged heavies, guard counters, and
stamina scheduling. Real source timings and Skyrim/CrossOver behavior remain
separate verification gates.

The combat logic is the engine-free simulation from
[Funny-Bones/ELDEN-RING-Combat-Rewrite](https://github.com/Funny-Bones/ELDEN-RING-Combat-Rewrite)
(MIT, see `third_party/`), taken from commit `177a3b5`.

## Current bridge behavior

- **Combat mode is on only while your weapon is drawn** and no menu is open.
  When you sheathe it, Skyrim and your other mods behave exactly as before.
- **While it's on, the plugin handles dodging, sprinting, jumping and attacks.**
  Skyrim's own sprint, jump and attack/block handlers are paused for that time.
  Each action the core starts plays through the animation event set in
  `ERCombat.ini`. By default those events drive TK Dodge and MCO/ADXP.
- **Activate, movement, shouts, sneaking and menus always stay with Skyrim**, so
  doors, loot and dialogue keep working mid-fight.
- **I-frames come from the active core action table.** They're set through the
  `bIframeActive` / `bInIframe` graph variables. Without an owner import, that
  table is explicitly synthetic.
- **Perspective no longer owns combat-mode lifetime.** Synchronized first- and
  third-person animation graphs, camera behavior, and mid-action runtime proof
  are still open work; the status list below is authoritative.

## Layout

| Path | What it is |
|---|---|
| `core/` | Rust library `er_combat_core.dll`. `src/sim/` owns action timing, while `balance`, `consequences`, `effects`, and `posture` translate contacts into Skyrim-facing results. `src/ffi.rs` is the C interface, in Skyrim units and axes. |
| `core/include/er_combat.h` | The C header for that interface. Tests check that it matches the Rust structs byte for byte. |
| `plugin/` | The SKSE plugin (C++, CommonLibSSE-NG). It hooks the player's update, reads input and plays the core's actions. |
| `plugin/dist/` | What goes in the mod archive: `SKSE/Plugins/ERCombat.dll`, `er_combat_core.dll` and `ERCombat.ini`. |
| `docs/architecture.md` | The implementation contract, ownership boundaries, and build order. |
| `docs/EXECUTION_PLAN.md` | The zero-to-hero phase graph, target code contracts, dump handoff, and swarm merge protocol. |

## Building (Windows)

You need Rust, Visual Studio 2022 with C++, CMake, Ninja and vcpkg (with `VCPKG_ROOT` set).

```powershell
cd core
cargo build --release          # er_combat_core.dll + .dll.lib
cd ..\plugin
cmake --preset release
cmake --build --preset release # ERCombat.dll, both DLLs copied to dist\
```

Then install `plugin/dist` as a mod in Vortex.

Every push also runs the local contract on a macOS worker and compiles the
actual CommonLib/SKSE DLL on Windows. That CI artifact uses the synthetic
action table and is a compile check, not source-data or CrossOver proof.

The engine-independent bridge geometry can also be checked natively on macOS:

```bash
clang++ -std=c++2b -Wall -Wextra -Werror \
  -Icore/include -Iplugin/src \
  plugin/src/BridgePlan.cpp plugin/src/CombatMath.cpp \
  plugin/tests/CombatMathTests.cpp \
  -o /tmp/skyring-combat-math-tests
/tmp/skyring-combat-math-tests
```

## Elden Ring data

The action table (timings, i-frames, cancel windows, root motion) is generated
from **your own copy of Elden Ring** and is never committed. Without it, the
core builds against a stand-in (`core/src/sim/extracted_stub.rs`) so everything
compiles and the tests run. To use the real data:

1. On Windows, run `py -3 tools\owner_data.py dump`; it drives the current
   Nuxe/WitchyBND workflow and the pinned upstream generators.
2. Move the resulting private ZIP to the Mac and run
   `python3 tools/owner_data.py import-bundle <zip>`. The importer installs
   `extracted.rs`; `build.rs` switches over automatically.

The source boundary is deliberate: Elden Ring PC supplies owner data, while
Skyrim SE/AE is the runtime target. Cyberpunk 2077 supplies neither and cannot
substitute for Elden Ring. Run `py -3 tools\owner_data.py requirements` on the
Windows machine for a no-installation-access summary before provisioning.

Import creates an ignored provenance receipt under `owner_data/` and binds the
installed table to a stable whole-source fingerprint. `--profile source`
rehashes that private inventory; merely placing an untracked `extracted.rs` in
the source tree cannot satisfy the source gate.

Please keep generated data out of forks and uploads, as upstream asks.

The Windows machine doing the extraction does not need a discrete GPU. See
[`docs/EXTRACTION.md`](docs/EXTRACTION.md) for the selective file list,
workstation guidance, the `tools/owner_data.py` Windows dumper, and the
hash-verified M2 Mac import handoff.

## Verification profiles

The repository distinguishes host-testable plumbing from source-data and
actual runtime proof:

```bash
python3 tools/verify.py --profile local    # synthetic plumbing is allowed
python3 tools/verify.py --profile source   # requires owner-generated data
python3 tools/verify.py --profile release  # also requires matching CrossOver evidence
```

Each run writes an ignored JSON report to `verification/out/last-run.json`.
The release profile cannot pass from unit tests alone; it fingerprints the
exact commit, extracted data, Windows DLLs, runtime log, and every melee-slice
acceptance assertion. The operational rules for coding agents are in
[`AGENTS.md`](AGENTS.md).

Release evidence is captured from a clean exact commit, after the Windows DLLs
exist and a CrossOver run has written `verification/out/runtime.log`:

```bash
python3 tools/evidence.py capture \
  --elden-ring-build <steam build id> --skyrim-version <version> \
  --crossover-version <version> \
  --accept input=log_event:<event id in the log> \
  --accept stamina=ticks:<start>-<end> \
  --accept hit_stop=capture:verification/out/<capture file>
```

`capture` writes two ignored files: `verification/out/evidence.json` (hashes of
both DLLs, the log, and any cited capture, taken with the commit and clean-tree
state) and `verification/runtime-evidence.json`. It records only what it can
measure, takes the source fingerprint from the validated owner-data receipt,
and promotes an assertion only when a log event id, tick range, or capture
locator is cited; a bare boolean is not evidence. It prints every remaining
problem and exits non-zero until none is left.

`python3 tools/verify.py --profile release` is the authority. It rejects
placeholders, missing fields, a dirty tree, and evidence for another commit,
source fingerprint, action table, DLL, log, capture, or acceptance key, and
requires each cited log event to occur in the hashed log. `tools/evidence.py
fingerprint --require-clean <files>` remains available for ad-hoc hashing.

## Reproducible builds and packaging

Every build input is pinned and checked by the local profile:

- `abi_layout` (`tools/abi_gate.py`) compares the Rust layout, the C header, and
  the pinned `verification/abi-layout.json` (sizes, offsets, enum values,
  `extern "C"` signatures). An intentional ABI change is recorded with
  `python3 tools/abi_gate.py --update`, which bumps the snapshot revision.
- `build_pins` (`tools/build_pins.py`) requires an exact Rust version
  (`core/rust-toolchain.toml`), actions pinned to commit SHAs, `cargo --locked`,
  pinned CMake/Ninja, vcpkg baselines, and runner images, all matching
  `verification/build-pins.json`.
- `package_inventory` (`tools/package_mod.py`) stages the mod archive from an
  explicit allowlist and refuses owner data, generated animation/audio, logs,
  and secrets. The zip is byte-reproducible. Without the Windows DLLs it reports
  `BLOCKED`, never a pass.

None of this proves a Windows runner builds yet: that stays BLOCKED until a
clean runner has produced both DLLs (`notes.windows_runner_verified`).

## Status

- [x] Core as a library with an explicitly checked C interface
- [x] Bounded action/armament balance policy and an action-economy report
- [x] Delivery-agnostic status buildup and buff/debuff stacking kernel
- [x] Separate defensive poise and offensive stance meters
- [x] Shared hit consequence contract for elements, statuses, modifiers, poise, and stance
- [x] Deterministic weapon-sweep versus actor-capsule contact geometry
- [x] Deterministic hostile-target selection and bounded movement requests
- [x] Skyrim high-process actor adapter with contact-only runtime diagnostics
- [x] Core sweep/native Skyrim melee-hit reconciliation (diagnostic opt-in)
- [x] macOS contract verification and Windows SKSE compile CI
- [x] SKSE bridge scaffold: combat mode, input, action → animation event, i-frames
- [x] Skyrim-owned stamina pool and spend report through the ABI (revision 3)
- [x] Magic core, host-tested (`local` only): catalyst gear axis, spells as committed actions, exactly-once impact correlation, buffs/debuffs/statuses through the shared pipeline, deterministic replay. Status, tuning inputs and blockers: [`docs/MAGIC_STATUS.md`](docs/MAGIC_STATUS.md)
- [ ] Synchronized first-/third-person animation presentation and mid-action switching
- [ ] First in-game build and test on 1.6.1170
- [ ] Lock-on target from True Directional Movement
- [ ] Gamepad bindings verified in game (the input rules and defaults exist and are host-tested; the Windows build and the button masks are unverified)
- [ ] Magic through the C ABI and the Skyrim bridge (blocked on the consequence ABI and a Skyrim runtime)
- [ ] Submit observed core contacts through Skyrim's normal hit path (phase B)
- [ ] Elden Ring animations converted from your own game files (phase B)

---

## Update, 12 October 2026: from a clone to a verified core, and the road still ahead

**In short.** This repository began as a clone of an engine-free Elden Ring combat simulation with a
thin Skyrim bridge. Over three days it has become a core with a defined authority model (Skyrim owns
health, stamina and magicka; the core only schedules and multiplies), a verification spine that cannot
be argued with, a data pipeline ready for the game dump, and the first complete magic system, all
proven on a Mac and none of it yet run inside Skyrim. Nothing here claims more than `local`.

### Where it started

The first commit (`ec55e55`, 9 October) was the combat logic of
[Funny-Bones/ELDEN-RING-Combat-Rewrite](https://github.com/Funny-Bones/ELDEN-RING-Combat-Rewrite) as a
library: about 4,000 lines of Rust (the player state machine, a stand-in action table, a C ABI) and a
600-line SKSE bridge scaffold. It had one test file, no CI, no verification of any kind, no way to
bring in game data, and one page of plan.

### What has been added

| Area | Then | Now |
|---|---|---|
| Size | 31 files | 119 files, +23,800 lines, 71 commits (19 landed by the Flint swarm) |
| Rust core | `sim`, `ffi` | adds `balance`, `consequences`, `effects`, `posture`, `trace`, `magic`, `cast`, `magic_world`, `magic_trace`, `spells`, `delivery`, `hazards` |
| Tests | 1 file | 165 Rust tests (unit, integration, property tests) and 191 Python tool tests |
| Verification | none | `tools/verify.py` with three profiles (`local`, `source`, `release`) and 13 required gates |
| Docs | `PLAN.md` | the Charter, architecture, execution plan, dump-independent work list, magic design and status, extraction guide, delivery and hazard audits |

**A verification spine.** A run can only claim what it proved. The `local` profile checks the C/Rust ABI
three ways (Rust layout, the header, a pinned snapshot, including exported signatures), pins every
build input (Rust 1.99.0, actions by commit SHA, `cargo --locked`, CMake, Ninja, vcpkg baselines),
stages the mod archive from an allowlist that refuses owner data, logs and secrets, and runs the
native bridge-geometry tests with warnings as errors. A missing platform is `BLOCKED`, never a pass.
Release evidence is captured from a clean exact commit and rejects anything stale: another commit,
source fingerprint, action table, DLL, log, or acceptance key.

**Authority, made concrete.** Skyrim now owns the stamina pool through the ABI (revision 3): the core reads
it each update, never regenerates it, and reports what it spent, including guarded hits that arrive
between updates. Magic follows the same rule for magicka.

**Core systems.** A delivery-neutral hit consequence pipeline (elements, status buildup, buff and debuff
stacking rules, separate poise and stance meters), bounded balance envelopes (the armament cap is
1.15x), deterministic sweep-versus-capsule contact geometry, target selection and movement
reconciliation, input rules that behave identically on keyboard, mouse and gamepad, and a
perspective-free camera heading. Combat can be recorded and replayed to the byte (trace format v2).

**A data pipeline for the dump.** `tools/owner_data.py` drives the Windows extraction (Nuxe, WitchyBND),
builds a verified bundle with a source fingerprint, imports it on the Mac with a receipt, and refuses a
tampered bundle, a re-import, or another game's data. A versioned combat IR validates actions, catalysts
and spells and forbids absolute damage and requirements at the boundary.

**Magic (this week).** Catalysts (staves and rings, one passive each) with the gear axis from the design
doc; spells as committed actions with a cancel window, hyperarmour, charge, hand dedication and
recovery; magicka read from Skyrim and re-checked at the release frame; exactly-once impact correlation;
buffs, debuffs and statuses through the same pipeline a sword uses; death, unload and form reuse that
leak nothing; deterministic record and replay of a whole session. Three catalysts and the seven cases of
the first slice exist as data. Details, tuning inputs and blockers are in
[`docs/MAGIC_STATUS.md`](docs/MAGIC_STATUS.md).

**Bows, throwables, consumables and hazards.** `delivery.rs` and `hazards.rs`, written by a second agent,
model committed bow draw and release, arrow payloads, throwables and environmental hazards as
calibrated estimates on the shared pipeline. They compile and pass their tests but have **not been
reviewed**.

### Defects found and fixed

- **The core was mirrored.** The Skyrim-to-simulation axis mapping was a reflection, not a rotation.
  Forward and heading survived, but the right stick strafed left and left/right dodge labels were
  swapped. Regression tests fail on the old mapping.
- **Fragile baselines.** A tool test that needed `pytest` on a bare Python, and a fixture that wrote a
  file named `.env`, broke the swarm's baseline; both are fixed and guarded by a test.

### How it was built, and what that taught

Work ran through three kinds of agent: a lead session, the **Flint swarm** (free OpenRouter models, 19
commits landed, mostly test-pinning and tooling), and a **GLM-based helper** that wrote the spell matrix
and reviewed the code. Three lessons are worth keeping:

1. **Small models write tests that cannot fail.** The helper's first property tests all passed while
   6 of 8 deliberate bugs went undetected. `tools/mutate_magic.py` breaks the library on purpose and
   checks a test notices; after being sent back with it, every mutant was caught. Treat any
   untested-by-mutation test as unproven.
2. **Free models fail in predictable ways** (`use core::` instead of the crate name, private items,
   "cargo is unavailable", a decomposer that splits a one-file task into scope-less children). The
   swarm's packets, scope gates and config were hardened, and the findings are written down in Flint's
   `bench/skyring/GUIDANCE.md`.
3. **A reviewer is not optional.** The helper's design review was right about a real defect (passives
   stack, per the design doc); its test claims needed independent checking.

### What "done" means here, and what is not

Everything above is proven by host tests and the `local` profile. **Nothing has run inside Skyrim.**
Every timing, magnitude and cost is a synthetic stand-in or an estimate until the owner's Elden Ring
extraction exists; numbers the Charter does not fix (catalyst scaling bounds, cast timings) are marked
as tuning inputs, not facts. Agreement between two parts of this code is not extraction validation. The
Windows build has been written but never confirmed on a runner, and the C++ added this week (input
routing, settings, camera heading) has not been compiled.

### The road to the rest

The manifest (`swarm/tasks.json`) lists 33 tasks in 17 ordered phases; 18 need no owner data. The honest
position per phase:

| Phase | Host-side work | Still needed |
|---|---|---|
| Foundation | traces, IR, builds, evidence capture: **done (`local`)** | a Windows runner confirming both DLLs; a real CrossOver evidence run |
| Owner data | dump tool and rehearsal tests: **done** | the real dump; source action table; animation conversion (all need the downloaded games) |
| Colossal-sword slice | input rules, heading, hit classification, stamina authority, trace: **done** | ground ray cast and lock-on read; character-controller motion; submitting hits through Skyrim's native path; both-perspective presentation; the CrossOver proof |
| Balance | bounded policy: **done** | real action and armament numbers from the dump |
| Consequences, posture | core meters and pipeline: **done** | the consequence ABI and save envelope; Skyrim adapters; the proofs |
| Bows | contracts (unreviewed) | review; the projectile adapter; one bow and three arrows proven |
| Gear axis, catalysts, magic | core and data: **done (`local`)** | magic through the C ABI; staff-record replacement; catalyst forms; the seven-case CrossOver proof |
| Enemy symmetry, breadth, Torrent, encounter, release, megafauna | not started | everything above first (the Charter order puts megafauna last) |

**What it takes, in order.**

1. **Review and decide (days, no new tooling).** Review `delivery.rs` and `hazards.rs`. Decide the open
   design questions in `docs/Magic overhaul.md`: what governs Scaling beyond the school skill, enchanting
   versus catalyst upgrades, and whether a second ring slot works with your framework.
2. **The dump (one evening on the ThinkCentre).** Run the extraction with `tools/owner_data.py`, import the
   bundle on the Mac, and the `source` profile becomes possible: real timings, root motion, sweeps and
   costs replace the estimates. Rebalance the tuning inputs against them.
3. **Prove the Windows build.** Push, let CI build both DLLs on a clean runner, and fix what the unbuilt
   C++ turns up. This also settles the MSVC toolset pin.
4. **First in-game session under CrossOver.** Confirm the gamepad masks, camera heading in both
   perspectives, and the native hit path; replace the flat-ground and no-target placeholders.
5. **The vertical slice.** One colossal sword against one test actor, satisfying all 14 release assertions
   with an evidence bundle tied to the exact commit and source fingerprint.
6. **Then the Charter order:** consequences ABI, posture, bows, then magic and catalysts through the ABI
   and bridge, then breadth.

**The risks.** CrossOver and the Skyrim bridge are the unknowns: none of the runtime work has been
exercised, and the plan's 14 assertions include one-frame timing agreement and perspective parity.
The extraction depends on third-party tools (Nuxe, WitchyBND) and the owner's copy of the game.
Several magic bounds are guesses until a mage has been played from level 1 to 30. And the free-model
swarm is limited by its request budget, so it is best at narrow, specified, test-first packets, not at
design.

**To check it yourself:** `python3 tools/verify.py --profile local` (about a minute), and
`python3 tools/mutate_magic.py . magic_adversarial` to see the tests being tested.
