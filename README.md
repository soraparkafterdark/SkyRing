# Elden Ring combat for Skyrim

An SKSE plugin that runs Elden Ring's player combat rules inside Skyrim SE/AE.
That covers rolls on release, sprint on hold, i-frames, attack chains with
their real cancel windows, charged heavies, guard counters and stamina costs.
It's built to sit on top of a big modlist (Gate to Sovngarde) without breaking it.

The combat logic is the engine-free simulation from
[Funny-Bones/ELDEN-RING-Combat-Rewrite](https://github.com/Funny-Bones/ELDEN-RING-Combat-Rewrite)
(MIT, see `third_party/`), taken from commit `177a3b5`.

## How it plays

- **Combat mode is on only while your weapon is drawn** and no menu is open.
  When you sheathe it, Skyrim and your other mods behave exactly as before.
- **While it's on, the plugin handles dodging, sprinting, jumping and attacks.**
  Skyrim's own sprint, jump and attack/block handlers are paused for that time.
  Each action the core starts plays through the animation event set in
  `ERCombat.ini`. By default those events drive TK Dodge and MCO/ADXP.
- **Activate, movement, shouts, sneaking and menus always stay with Skyrim**, so
  doors, loot and dialogue keep working mid-fight.
- **I-frames come from the core's real roll timings.** They're set through the
  `bIframeActive` / `bInIframe` graph variables.
- **First- and third-person are interchangeable, including during actions.**
  Both views present the same live action clock, root motion, i-frames, stamina,
  and hit geometry. The first-person camera keeps a stable horizon through
  rolls by default; changing view never restarts or cancels combat state.

## Layout

| Path | What it is |
|---|---|
| `core/` | Rust library `er_combat_core.dll`. `src/sim/` owns action timing, while `balance`, `consequences`, `effects`, and `posture` translate contacts into Skyrim-facing results. `src/ffi.rs` is the C interface, in Skyrim units and axes. |
| `core/include/er_combat.h` | The C header for that interface. Tests check that it matches the Rust structs byte for byte. |
| `plugin/` | The SKSE plugin (C++, CommonLibSSE-NG). It hooks the player's update, reads input and plays the core's actions. |
| `plugin/dist/` | What goes in the mod archive: `SKSE/Plugins/ERCombat.dll`, `er_combat_core.dll` and `ERCombat.ini`. |
| `architecture.md` | The implementation contract, ownership boundaries, and build order. |
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

## Status

- [x] Core as a library with a C interface; 29 tests (`cd core && cargo test --all-targets`)
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
- [ ] Synchronized first-/third-person animation presentation and mid-action switching
- [ ] First in-game build and test on 1.6.1170
- [ ] Lock-on target from True Directional Movement
- [ ] Gamepad bindings
- [ ] Submit observed core contacts through Skyrim's normal hit path (phase B)
- [ ] Elden Ring animations converted from your own game files (phase B)
