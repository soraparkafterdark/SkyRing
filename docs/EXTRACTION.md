# Owner-data extraction workstation

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
records the action table's SHA-256 fingerprint without placing it in Git.

Record the Elden Ring build/version used for extraction. A later release gate
will reject CrossOver evidence that does not match the action-table fingerprint
and exact SkyRing commit.
