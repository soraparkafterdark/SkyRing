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
- [ ] Synchronized first-/third-person animation presentation and mid-action switching
- [ ] First in-game build and test on 1.6.1170
- [ ] Lock-on target from True Directional Movement
- [ ] Gamepad bindings
- [ ] Submit observed core contacts through Skyrim's normal hit path (phase B)
- [ ] Elden Ring animations converted from your own game files (phase B)
