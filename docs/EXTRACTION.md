# Owner-data extraction workstation

## Supported game boundary

SkyRing has exactly one owner-data source and one runtime target:

- **Elden Ring PC (Steam app 1245620)** supplies the private action,
  animation, regulation, and optional sound inputs.
- **Skyrim Special Edition / Anniversary Edition** is the runtime target. Its
  installation is not imported into the Elden Ring owner-data bundle.
- **Cyberpunk 2077 is not a SkyRing input.** It cannot substitute for Elden
  Ring, and the importer rejects a bundle labelled as Cyberpunk data.

Therefore the ThinkCentre needs an installed, legally owned Elden Ring copy
for `dump`. Install Skyrim there as well only if it will be used for native
Windows build/runtime checks. A Cyberpunk installation is unrelated to this
pipeline. Confirm the boundary at any time without installing a game:

```powershell
py -3 tools\owner_data.py requirements
```

SkyRing does not require a gaming GPU to unpack and generate its owner-local
combat data. The extraction path is file I/O, decompression, binary parsing,
and Rust/Python generation. A 64-bit Ryzen 5 ThinkCentre with integrated
graphics is a suitable machine for this work.

## One-command owner-data tool

Clone this repository on the Windows machine, open PowerShell in it, and run:

```powershell
py -3 tools\owner_data.py plan
py -3 tools\owner_data.py dump
```

`plan` and `dump` fail closed unless the directory contains `eldenring.exe`,
`regulation.bin`, and Elden Ring's Oodle DLL. They also record Steam app
1245620's installed `buildid` from `appmanifest_1245620.acf`. For a legitimate
nonstandard layout where that manifest is unavailable, pass the numeric value
explicitly with `--steam-build-id`; do not guess it.

`dump` downloads hash-pinned official Nuxe and WitchyBND releases, downloads
the exact pinned combat-rewrite generator source, and includes action data,
animations, regulation inputs, and player sounds. Nuxe is GUI-only, so the
tool displays the exact Advanced-tab values and filter, launches it, and then
refuses to continue until every required file exists. It automates WitchyBND,
the generators, hashing, and creation of the private transfer ZIP.

Use `--no-sounds` only if you intentionally want the smaller combat-only
bundle. Use `--offline` after the verified archives have already been cached
in the work directory.

Copy the resulting `SkyRing-owner-data-*.zip` to the Mac and run:

```bash
python3 tools/owner_data.py validate-bundle /path/to/SkyRing-owner-data-....zip
python3 tools/owner_data.py import-bundle /path/to/SkyRing-owner-data-....zip
python3 tools/verify.py --profile source
```

Import installs only `extracted.rs` at the location the core consumes. Raw
owner inputs and generated animation/audio remain under ignored
`owner_data/<bundle fingerprint>/` storage. The ZIP and every member are
verified before anything is imported.

The importer validates the exact pinned generator and tool identities, all
required Elden Ring binders, unpacked regulation parameters, the source-game
identity/build, and every member size/hash. It computes a canonical source
fingerprint from that provenance and inventory. Unlike the transfer ZIP hash,
the source fingerprint is stable if the same bytes are bundled again. Import
is staged before the active table changes and writes an ignored
`owner_data/current.json` receipt binding the fingerprint, private manifest,
transfer hash, and installed `extracted.rs` hash.

No upstream tool publishes a separate CPU, GPU, or RAM minimum for extraction.
The practical requirements are:

- 64-bit Windows 10 or newer;
- a legitimate, current Elden Ring installation;
- Python 3.10 or newer (Rust is only needed if you also run the upstream
  sandbox itself);
- Nuxe and WitchyBND (the owner-data tool downloads pinned, hash-verified
  official releases);
- enough disk for the game installation, selected unpacked files, generated
  outputs, and temporary files.

Integrated graphics are irrelevant unless the machine will also run Elden
Ring for visual capture. CPU generation and disk speed affect elapsed time,
not correctness. An SSD is strongly preferred. If the machine has only 8 GB
of RAM, try it before upgrading; memory is not identified upstream as a dump
requirement. For a fresh install, budget at least the game's listed 60 GB plus
working room. Selective extraction avoids making a second complete copy, but
sound extraction and careless full unpacking can consume much more space.

## What the tool selects

The tool encodes the upstream combat-rewrite list and validates it before
generation. At the time this checklist was written, that means:

1. In Nuxe's Advanced > Unpack tab, select only these `chr/` animation binders:
   `c0000.anibnd.dcx`, `c0000_a00_hi`, `c0000_a00_lo`, `c0000_a00_md`,
   `c0000_a0x`, `c0000_a1x`, `c0000_a2x`, `c0000_a3x`, `c0000_a4x`, and
   `c0000_a5x` (each with the `.anibnd.dcx` suffix).
2. Use **Unpack only**. Never use Nuxe's Patch operation for this task.
3. Unpack `regulation.bin` with WitchyBND into `regulation-bin/`.
4. Sound inputs are `sd/cs_smain.bnk` and `sd/enus/cs_main.bnk`. The tool
   includes them by default; `--no-sounds` produces a combat-only bundle.
5. Arrange the owner-local input root as `chr/...`, `regulation-bin/...`, and
   optionally `sd/...`.
6. Set `ER_FILES` to that input root and `ER_GAME_DIR` to Elden Ring's `Game`
   directory, then run the upstream `python tools/setup.py`.

The upstream project says generation takes about a minute after the archives
are unpacked. Archive selection and disk copying are the variable part.

## Bringing the bundle to the Mac

Validate and import the private ZIP:

```bash
python3 tools/owner_data.py validate-bundle /path/to/SkyRing-owner-data-....zip
python3 tools/owner_data.py import-bundle /path/to/SkyRing-owner-data-....zip
python3 tools/verify.py --profile source
```

The importer installs the generated action table at
`core/src/sim/extracted.rs`, where the build consumes it, and keeps raw input,
animation, and sound data below ignored `owner_data/` storage. The verifier
rehashes the private inventory and rejects an action table not bound to the
validated import receipt. The source profile reports the stable whole-source
fingerprint without placing it in Git.

The receipt records the Elden Ring Steam build ID automatically. A later
release gate rejects CrossOver evidence that does not match the source
fingerprint and exact SkyRing commit.

## Tonight's provisioning handoff

1. Install current Python 3 and Git on the ThinkCentre.
2. Install the owner's Elden Ring PC copy through Steam. Skyrim SE/AE is also
   needed for later Windows runtime work; Cyberpunk is not used by SkyRing.
3. Clone this repository onto an NTFS volume with ample free space.
4. Run `py -3 tools\owner_data.py requirements`, then
   `py -3 tools\owner_data.py plan` to prove discovery and record the exact GUI
   selection before starting the longer dump.
5. Run `py -3 tools\owner_data.py dump`. Keep the resulting ZIP private and
   transfer it to the Mac through local/removable storage.
6. On the Mac, run `validate-bundle`, then `import-bundle`, then the `source`
   profile exactly as shown above. A failure is `BLOCKED`; never hand-edit
   `extracted.rs` or relabel a synthetic table.
