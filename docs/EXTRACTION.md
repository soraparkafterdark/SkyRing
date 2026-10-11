# Owner-data extraction workstation

SkyRing does not require a gaming GPU to unpack and generate its owner-local
combat data. The extraction path is file I/O, decompression, binary parsing,
and Rust/Python generation. A 64-bit Ryzen 5 ThinkCentre with integrated
graphics is a suitable machine for this work.

No upstream tool publishes a separate CPU, GPU, or RAM minimum for extraction.
The practical requirements are:

- 64-bit Windows 10 or newer;
- a legitimate, current Elden Ring installation;
- Python 3.10 or newer and Rust for the upstream generators;
- UXM Selective Unpack and WitchyBND (or the currently supported archive
  unpacker required by the selected WitchyBND release);
- enough disk for the game installation, selected unpacked files, generated
  outputs, and temporary files.

Integrated graphics are irrelevant unless the machine will also run Elden
Ring for visual capture. CPU generation and disk speed affect elapsed time,
not correctness. An SSD is strongly preferred. If the machine has only 8 GB
of RAM, try it before upgrading; memory is not identified upstream as a dump
requirement. For a fresh install, budget at least the game's listed 60 GB plus
working room. Selective extraction avoids making a second complete copy, but
sound extraction and careless full unpacking can consume much more space.

## Minimal SkyRing source set

Follow the upstream combat-rewrite instructions for the authoritative list.
At the time this checklist was written, that means:

1. With UXM Selective Unpack, select only these `chr/` animation binders:
   `c0000.anibnd.dcx`, `c0000_a00_hi`, `c0000_a00_lo`, `c0000_a00_md`,
   `c0000_a0x`, `c0000_a1x`, `c0000_a2x`, `c0000_a3x`, `c0000_a4x`, and
   `c0000_a5x` (each with the `.anibnd.dcx` suffix).
2. Use **Unpack only**. Never use UXM's Patch operation for this task.
3. Unpack `regulation.bin` with WitchyBND into `regulation-bin/`.
4. Optional sound inputs are `sd/cs_smain.bnk` and
   `sd/enus/cs_main.bnk`. Sound is not required for the combat-data gate.
5. Arrange the owner-local input root as `chr/...`, `regulation-bin/...`, and
   optionally `sd/...`.
6. Set `ER_FILES` to that input root and `ER_GAME_DIR` to Elden Ring's `Game`
   directory, then run the upstream `python tools/setup.py`.

The upstream project says generation takes about a minute after the archives
are unpacked. Archive selection and disk copying are the variable part.

## Bringing back only what SkyRing needs

For the current core, copy the generated action table to:

```text
core/src/sim/extracted.rs
```

Do not commit it. Run:

```bash
python3 tools/verify.py --profile source
```

The verifier records its SHA-256 fingerprint without placing the generated
file in Git. Animation and sound outputs also remain owner-local; add explicit
ignored destinations before copying them into this repository.

Record the Elden Ring build/version used for extraction. A later release gate
will reject CrossOver evidence that does not match the action-table fingerprint
and exact SkyRing commit.
