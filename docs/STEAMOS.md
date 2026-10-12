# Skyrim inspection on SteamOS

The owner's Skyrim SE/AE copy lives on the SteamOS ThinkCentre, where it runs
under Steam + Proton. `tools/skyrim_steamos.py` inspects that install. It is
read-only, standard-library Python, and writes only to the git-ignored
`verification/out/`. The Windows build and extraction path
(`tools/owner_data.py`, the Windows SKSE build) is unchanged and remains the
route for Windows runtime checks.

## What it is, and is not

It reverse-engineers the *installed Skyrim*: build identity, executable
structure, Address Library, SKSE plugins, and plugin (ESM) data. It is **not**
the Elden Ring owner-data dump. That still requires Elden Ring and is
unaffected, and the Skyrim install is still never merged into the owner-data
bundle (see `EXTRACTION.md`).

Claim ceiling: everything here is a SteamOS/Proton preflight. AGENTS.md puts
the acceptance machine at CrossOver on Apple Silicon, so no report from this
tool is `release` evidence or may fill `skyrim_version` in a release manifest.
A missing install, prefix, SKSE build or Address Library file is `BLOCKED`.

## Usage

On the ThinkCentre (Desktop Mode terminal), from a clone of this repository:

```bash
python3 tools/skyrim_steamos.py probe            # identify build, check prerequisites
python3 tools/skyrim_steamos.py exe              # deep PE analysis of SkyrimSE.exe
python3 tools/skyrim_steamos.py address-library --id 11063 --rva 0x1234
python3 tools/skyrim_steamos.py plugins          # audit Data/SKSE/Plugins DLLs
python3 tools/skyrim_steamos.py esm              # Skyrim.esm header and record counts
python3 tools/skyrim_steamos.py weapons --filter TwoHand
python3 tools/skyrim_steamos.py strings --pattern 'Havok|hkb'
python3 tools/skyrim_steamos.py logs             # SKSE logs from the Proton prefix
python3 tools/skyrim_steamos.py launch-help      # SKSE under Proton launch steps
```

Steam is found under the usual SteamOS paths (including the Flatpak Steam and
SD-card libraries listed in `libraryfolders.vdf`). Override with
`--steam-root` or `--game-dir`. Names are matched case-insensitively because
Linux paths are case-sensitive and Windows-era mod instructions are not.

| Command | Reads | Reports |
| --- | --- | --- |
| `probe` | appmanifest, exe, `Data/`, SKSE, prefix | build ID, exe version/hashes, base-master hashes, SKSE and Address Library match, Proton tool, a runtime fingerprint, blockers |
| `exe` | any PE (default `SkyrimSE.exe`) | sections with entropy, imports, exports, PDB identity, ASLR/NX, SteamStub flag |
| `address-library` | `version(lib)-*.bin` | ID to RVA lookups both ways; accepted only if the parse consumes the file exactly |
| `plugins` | `Data/SKSE/Plugins/*.dll` | hash, build time, SKSE exports, whether it loads on 1.6.x (`SKSEPlugin_Version`) |
| `esm` | a plugin (default `Skyrim.esm`) | flags, masters, per-record-type counts |
| `weapons` | `WEAP` records | editor ID, damage, speed, reach, animation type |
| `strings` | any binary | ASCII or UTF-16 strings with file offsets |
| `logs` | Proton prefix | SKSE and plugin log lines, tail |

## Cautions

- **SteamStub.** The Steam `SkyrimSE.exe` carries a `.bind` section and its
  code is encrypted on disk. `exe` flags it. Byte reads at an RVA from the
  file (`address-library --peek`) are not valid code in that case; use a
  running-process dump for disassembly.
- **Layouts.** The Address Library delta encoding and the `WEAP` `DNAM`/`DATA`
  field layouts follow community documentation. The first is self-checked
  (exact consumption); the weapon fields are not, so cross-check a known item
  (Iron Sword: damage 7, speed 1.0, reach 1.0) before relying on them. Weapon
  names live in localized string tables and are not decoded.
- **Plugins are Windows x64 DLLs.** Build or cross-compile them off the
  SteamOS box; nothing native to Linux can load into the Proton process.
- **Third-party data stays private.** Reports contain absolute paths and
  hashes of owned game files, so they live in ignored `verification/out/`.
  Do not commit them.

The pure parsers are in `tools/skyrim_re.py` and are tested natively on macOS
with synthetic PE, Address Library and plugin fixtures
(`tools/tests/test_skyrim_re.py`, `tools/tests/test_skyrim_steamos.py`).
Exercising them against the real install is the one thing the tests cannot do.
