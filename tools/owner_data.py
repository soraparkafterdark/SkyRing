#!/usr/bin/env python3
"""Build and transfer SkyRing's private owner-data bundle.

The Windows dump path uses Nuxe for selective archive extraction, WitchyBND
for regulation.bin, and the pinned upstream combat-rewrite generators. Nuxe is
a GUI application, so this tool prints the exact Advanced-mode values, launches
it, and refuses to continue until every selected file is present.

No game-derived file is written to Git. The resulting ZIP contains hashes for
every payload and can be validated/imported on the target Mac.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tempfile
from typing import Any, Iterable
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parents[1]
SCHEMA_VERSION = 1
UPSTREAM_COMMIT = "177a3b5ec14a9bde39b9ab458a3400b4038224e3"
UPSTREAM_URL = (
    "https://github.com/Funny-Bones/ELDEN-RING-Combat-Rewrite/archive/"
    f"{UPSTREAM_COMMIT}.zip"
)
UPSTREAM_SHA256 = "26ac5d53c417e0837f6b58065887febbf9c9a0d91ec75a870a44ce7faa3ae4ec"

NUXE_VERSION = "1.2.1"
NUXE_URL = "https://github.com/JKAnderson/Nuxe/releases/download/v1.2.1/Nuxe.1.2.1.zip"
NUXE_SHA256 = "023d3c2a349b6667a7267f98eb6b1a6a2b37a0a0b820cf6dfd5cb1375a78267a"

WITCHY_VERSION = "3.0.1.0"
WITCHY_URL = (
    "https://github.com/ividyon/WitchyBND/releases/download/3.0.1.0/"
    "WitchyBND-3.0.1.0-win-x64.zip"
)
WITCHY_SHA256 = "a3e6b2a0f7eac13f5e83b6602a1149322439c0662baa140ecdd84be28af50364"

ANIMATION_BINDERS = (
    "c0000.anibnd.dcx",
    "c0000_a00_hi.anibnd.dcx",
    "c0000_a00_lo.anibnd.dcx",
    "c0000_a00_md.anibnd.dcx",
    "c0000_a0x.anibnd.dcx",
    "c0000_a1x.anibnd.dcx",
    "c0000_a2x.anibnd.dcx",
    "c0000_a3x.anibnd.dcx",
    "c0000_a4x.anibnd.dcx",
    "c0000_a5x.anibnd.dcx",
)
SOUND_BANKS = ("sd/cs_smain.bnk", "sd/enus/cs_main.bnk")
GENERATED_REQUIRED = ("src/sim/extracted.rs", "assets/player_anims.bin")
GENERATED_SOUND = ("assets/player_sounds.bin",)


class OwnerDataError(RuntimeError):
    pass


def validate_action_list(actions: list[dict]) -> None:
    if not actions:
        raise OwnerDataError("action list is empty")
    for i, action in enumerate(actions):
        if not isinstance(action, dict):
            raise OwnerDataError(f"action at index {i} is not a dictionary")
        # Required keys
        for key in ("id", "kind", "total", "stamina_cost", "hit_windows"):
            if key not in action:
                raise OwnerDataError(f"action at index {i} missing required key: {key}")
        # Check types
        if not isinstance(action["id"], (str, int)):
            raise OwnerDataError(f"action at index {i} id must be string or integer")
        if not isinstance(action["kind"], str):
            raise OwnerDataError(f"action at index {i} kind must be string")
        if not isinstance(action["total"], int):
            raise OwnerDataError(f"action at index {i} total must be integer")
        if not isinstance(action["stamina_cost"], int):
            raise OwnerDataError(f"action at index {i} stamina_cost must be integer")
        if action["stamina_cost"] < 0:
            raise OwnerDataError(f"action at index {i} stamina_cost must be non-negative")
        # Hit windows
        if not isinstance(action["hit_windows"], list):
            raise OwnerDataError(f"action at index {i} hit_windows must be a list")
        seen_windows = set()
        for j, window in enumerate(action["hit_windows"]):
            if not isinstance(window, (list, tuple)) or len(window) != 2:
                raise OwnerDataError(f"action at index {i}, hit window {j} must be a list or tuple of two integers")
            start, end = window
            if not isinstance(start, int) or not isinstance(end, int):
                raise OwnerDataError(f"action at index {i}, hit window {j} start and end must be integers")
            if start > end:
                raise OwnerDataError(f"action at index {i}, hit window {j} start must be <= end")
            if (start, end) in seen_windows:
                raise OwnerDataError(f"action at index {i} has duplicate hit window range: ({start}, {end})")
            seen_windows.add((start, end))


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def relative_files(root: Path) -> list[Path]:
    return sorted(path for path in root.rglob("*") if path.is_file())


def safe_member(name: str) -> PurePosixPath:
    if "\\" in name or "\x00" in name:
        raise OwnerDataError(f"unsafe ZIP member path: {name!r}")
    member = PurePosixPath(name)
    if member.is_absolute() or ".." in member.parts or not member.parts:
        raise OwnerDataError(f"unsafe ZIP member path: {name!r}")
    if any(":" in part for part in member.parts):
        raise OwnerDataError(f"unsafe ZIP member colon path: {name!r}")
    return member


def safe_extract_zip(archive: Path, destination: Path) -> None:
    with zipfile.ZipFile(archive) as bundle:
        for info in bundle.infolist():
            member = safe_member(info.filename)
            target = destination.joinpath(*member.parts)
            if info.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with bundle.open(info) as source, target.open("wb") as output:
                shutil.copyfileobj(source, output, length=1024 * 1024)


def download(url: str, expected_sha256: str, destination: Path) -> Path:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() and sha256(destination) == expected_sha256:
        return destination
    temporary = destination.with_suffix(destination.suffix + ".partial")
    if temporary.exists():
        temporary.unlink()
    print(f"Downloading {url}")
    try:
        curl = shutil.which("curl")
        if curl:
            completed = subprocess.run(
                [curl, "-L", "--fail", "--silent", "--show-error", url, "-o", str(temporary)],
                check=False,
            )
            if completed.returncode != 0:
                raise OwnerDataError(f"curl exited with code {completed.returncode}")
        else:
            request = urllib.request.Request(url, headers={"User-Agent": "SkyRing-owner-data/1"})
            with urllib.request.urlopen(request) as response, temporary.open("wb") as output:
                shutil.copyfileobj(response, output, length=1024 * 1024)
    except Exception as error:
        if temporary.exists():
            temporary.unlink()
        raise OwnerDataError(f"download failed: {url}: {error}") from error
    actual = sha256(temporary)
    if actual != expected_sha256:
        temporary.unlink()
        raise OwnerDataError(
            f"download hash mismatch for {url}: expected {expected_sha256}, got {actual}"
        )
    temporary.replace(destination)
    return destination


def unpack_tool(archive: Path, destination: Path, executable_name: str) -> Path:
    executable = next(iter(destination.rglob(executable_name)), None) if destination.exists() else None
    if executable:
        return executable
    destination.mkdir(parents=True, exist_ok=True)
    safe_extract_zip(archive, destination)
    executable = next(iter(destination.rglob(executable_name)), None)
    if not executable:
        raise OwnerDataError(f"{executable_name} was not found in {archive.name}")
    return executable


def bootstrap_tools(work_dir: Path, offline: bool = False) -> tuple[Path, Path]:
    downloads = work_dir / "downloads"
    tool_root = work_dir / "tools"
    nuxe_archive = downloads / f"Nuxe.{NUXE_VERSION}.zip"
    witchy_archive = downloads / f"WitchyBND-{WITCHY_VERSION}-win-x64.zip"
    if offline:
        for archive, expected in (
            (nuxe_archive, NUXE_SHA256),
            (witchy_archive, WITCHY_SHA256),
        ):
            if not archive.is_file() or sha256(archive) != expected:
                raise OwnerDataError(f"offline mode requires verified archive: {archive}")
    else:
        download(NUXE_URL, NUXE_SHA256, nuxe_archive)
        download(WITCHY_URL, WITCHY_SHA256, witchy_archive)
    nuxe = unpack_tool(nuxe_archive, tool_root / f"Nuxe-{NUXE_VERSION}", "Nuxe.exe")
    witchy = unpack_tool(
        witchy_archive,
        tool_root / f"WitchyBND-{WITCHY_VERSION}",
        "WitchyBND.exe",
    )
    return nuxe, witchy


def steam_library_candidates() -> Iterable[Path]:
    roots: list[Path] = []
    program_files = os.environ.get("PROGRAMFILES(X86)") or os.environ.get("PROGRAMFILES")
    if program_files:
        roots.append(Path(program_files) / "Steam")
    steam_path = os.environ.get("STEAM_PATH")
    if steam_path:
        roots.append(Path(steam_path))
    for steam_root in roots:
        yield steam_root
        libraries = steam_root / "steamapps" / "libraryfolders.vdf"
        if not libraries.is_file():
            continue
        text = libraries.read_text(encoding="utf-8", errors="ignore")
        for raw in re.findall(r'"path"\s+"([^"]+)"', text):
            yield Path(raw.replace("\\\\", "\\"))


def find_game_dir(explicit: Path | None) -> Path:
    candidates: list[Path] = []
    if explicit:
        candidates.append(explicit)
    configured = os.environ.get("ER_GAME_DIR")
    if configured:
        candidates.append(Path(configured))
    for library in steam_library_candidates():
        candidates.append(library / "steamapps" / "common" / "ELDEN RING" / "Game")
    for candidate in candidates:
        resolved = candidate.expanduser().resolve()
        if (resolved / "eldenring.exe").is_file():
            return resolved
    rendered = "\n".join(f"  - {candidate}" for candidate in candidates) or "  (none discovered)"
    raise OwnerDataError(
        "could not find Elden Ring's Game directory; pass --game-dir. Checked:\n" + rendered
    )


def validate_game_dir(game_dir: Path) -> None:
    missing = [
        name
        for name in ("eldenring.exe", "regulation.bin", "oo2core_6_win64.dll")
        if not (game_dir / name).is_file()
    ]
    if missing:
        raise OwnerDataError(f"not an Elden Ring Game directory; missing: {', '.join(missing)}")


def nuxe_filter(include_sounds: bool) -> str:
    animation_names = "|".join(re.escape(name) for name in ANIMATION_BINDERS)
    branches = [rf"/chr/(?:{animation_names})"]
    if include_sounds:
        sound_names = "|".join(re.escape(name.removeprefix("sd/")) for name in SOUND_BANKS)
        branches.append(rf"/sd/(?:{sound_names})")
    return rf"^(?:{'|'.join(branches)})$"


def required_source_paths(include_sounds: bool) -> list[Path]:
    paths = [Path("chr") / name for name in ANIMATION_BINDERS]
    if include_sounds:
        paths.extend(Path(name) for name in SOUND_BANKS)
    return paths


def validate_nuxe_output(source_dir: Path, include_sounds: bool) -> None:
    missing = [path.as_posix() for path in required_source_paths(include_sounds) if not (source_dir / path).is_file()]
    if missing:
        joined = "\n".join(f"  - {path}" for path in missing)
        raise OwnerDataError(f"Nuxe output is incomplete. Missing:\n{joined}")


def print_nuxe_plan(game_dir: Path, source_dir: Path, include_sounds: bool) -> None:
    print("\nNuxe requires one GUI step. Use the Advanced tab with:")
    print("  Advanced > Unpack")
    print("  Game Type:                     Elden Ring (PC)")
    print(f"  Game Directory:                {game_dir}")
    print("  [x] Unpack to separate directory")
    print(f"      Unpack directory:          {source_dir}")
    print("  [x] Filter files (Regex)")
    print("      Filter: " + nuxe_filter(include_sounds))
    print("  Click Unpack. NEVER use Patch for this task.")
    print("\nClose Nuxe after Unpack completes; this process will then validate every file.\n")


def launch_nuxe(nuxe: Path, game_dir: Path, source_dir: Path, include_sounds: bool) -> None:
    print_nuxe_plan(game_dir, source_dir, include_sounds)
    source_dir.mkdir(parents=True, exist_ok=True)
    completed = subprocess.run([str(nuxe)], cwd=nuxe.parent, check=False)
    if completed.returncode != 0:
        raise OwnerDataError(f"Nuxe exited with code {completed.returncode}")
    validate_nuxe_output(source_dir, include_sounds)


def run_witchy(witchy: Path, game_dir: Path, source_dir: Path, reuse: bool) -> None:
    regulation = source_dir / "regulation.bin"
    regulation_folder = source_dir / "regulation-bin"
    if reuse and regulation_folder.is_dir() and any(regulation_folder.rglob("*.param")):
        return
    shutil.copy2(game_dir / "regulation.bin", regulation)
    command = [str(witchy), "--passive", "--silent", "--unpack", str(regulation)]
    print("Running WitchyBND on regulation.bin...")
    completed = subprocess.run(command, cwd=source_dir, check=False)
    if completed.returncode != 0:
        raise OwnerDataError(f"WitchyBND exited with code {completed.returncode}")
    if not regulation_folder.is_dir() or not any(regulation_folder.rglob("*.param")):
        raise OwnerDataError(
            "WitchyBND did not create regulation-bin/*.param; rerun without --reuse or inspect its log"
        )


def prepare_upstream(work_dir: Path, offline: bool) -> Path:
    destination = work_dir / "upstream" / UPSTREAM_COMMIT
    setup = destination / "tools" / "setup.py"
    if setup.is_file():
        return destination
    archive = work_dir / "downloads" / f"combat-rewrite-{UPSTREAM_COMMIT}.zip"
    if offline:
        if not archive.is_file() or sha256(archive) != UPSTREAM_SHA256:
            raise OwnerDataError(f"offline mode requires verified archive: {archive}")
    else:
        download(UPSTREAM_URL, UPSTREAM_SHA256, archive)
    with tempfile.TemporaryDirectory(prefix="skyring-upstream-", dir=work_dir) as temporary:
        expanded = Path(temporary)
        safe_extract_zip(archive, expanded)
        roots = [path for path in expanded.iterdir() if path.is_dir()]
        if len(roots) != 1 or not (roots[0] / "tools" / "setup.py").is_file():
            raise OwnerDataError("pinned upstream archive has an unexpected layout")
        destination.parent.mkdir(parents=True, exist_ok=True)
        roots[0].replace(destination)
    return destination


def run_generators(upstream: Path, game_dir: Path, source_dir: Path) -> None:
    environment = os.environ.copy()
    environment["ER_FILES"] = str(source_dir)
    environment["ER_GAME_DIR"] = str(game_dir)
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    command = [sys.executable, str(upstream / "tools" / "setup.py")]
    print("Running pinned combat-rewrite generators...")
    completed = subprocess.run(command, cwd=upstream, env=environment, check=False)
    if completed.returncode != 0:
        raise OwnerDataError(f"upstream generators exited with code {completed.returncode}")
    missing = [path for path in GENERATED_REQUIRED if not (upstream / path).is_file()]
    if missing:
        raise OwnerDataError(f"generators did not produce: {', '.join(missing)}")


def source_payload(source_dir: Path, include_sounds: bool) -> list[tuple[Path, str]]:
    inputs: list[tuple[Path, str]] = []
    for relative in required_source_paths(include_sounds):
        inputs.append((source_dir / relative, f"source/{relative.as_posix()}"))
    for relative in (Path("regulation.bin"), Path("oo2core_6_win64.dll")):
        inputs.append((source_dir / relative, f"source/{relative.as_posix()}"))
    regulation = source_dir / "regulation-bin"
    for path in relative_files(regulation):
        relative = path.relative_to(source_dir).as_posix()
        inputs.append((path, f"source/{relative}"))
    return inputs


def generated_payload(upstream: Path, include_sounds: bool) -> list[tuple[Path, str]]:
    output = [(upstream / path, f"generated/{path}") for path in GENERATED_REQUIRED]
    if include_sounds:
        for relative in GENERATED_SOUND:
            path = upstream / relative
            if not path.is_file():
                raise OwnerDataError(f"requested sound output is missing: {relative}")
            output.append((path, f"generated/{relative}"))
        sounds = upstream / "assets" / "sounds"
        if not sounds.is_dir():
            raise OwnerDataError("requested generated sounds directory is missing")
        for path in relative_files(sounds):
            output.append((path, f"generated/{path.relative_to(upstream).as_posix()}"))
    return output


def build_manifest(
    payload: list[tuple[Path, str]],
    game_dir: Path,
    include_sounds: bool,
) -> dict[str, Any]:
    files: dict[str, dict[str, Any]] = {}
    for source, archive_name in payload:
        if not source.is_file():
            raise OwnerDataError(f"bundle input is missing: {source}")
        if source.is_symlink():
            raise OwnerDataError(f"bundle inputs may not be symbolic links: {source}")
        safe_member(archive_name)
        files[archive_name] = {"size": source.stat().st_size, "sha256": sha256(source)}
    game_exe = game_dir / "eldenring.exe"
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "SkyRing owner data",
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "upstream": {
            "repository": "Funny-Bones/ELDEN-RING-Combat-Rewrite",
            "commit": UPSTREAM_COMMIT,
            "archive_sha256": UPSTREAM_SHA256,
        },
        "tools": {
            "nuxe": {"version": NUXE_VERSION, "archive_sha256": NUXE_SHA256},
            "witchybnd": {"version": WITCHY_VERSION, "archive_sha256": WITCHY_SHA256},
        },
        "source_game": {
            "eldenring_exe_size": game_exe.stat().st_size,
            "eldenring_exe_sha256": sha256(game_exe),
            "sounds_included": include_sounds,
        },
        "files": files,
    }


def create_bundle(
    source_dir: Path,
    upstream: Path,
    game_dir: Path,
    include_sounds: bool,
    output: Path,
    force: bool,
) -> Path:
    if output.exists() and not force:
        raise OwnerDataError(f"bundle already exists; pass --force to replace it: {output}")
    payload = source_payload(source_dir, include_sounds) + generated_payload(upstream, include_sounds)
    # First, check that all bundle inputs are present and not symlinks.
    for source, archive_name in payload:
        if not source.is_file():
            raise OwnerDataError(f"bundle input is missing: {source}")
        if source.is_symlink():
            raise OwnerDataError(f"bundle inputs may not be symbolic links: {source}")
    # Then, validate the action list in extracted.rs.
    for source, archive_name in payload:
        if archive_name == "generated/src/sim/extracted.rs":
            content = source.read_text(encoding="utf-8")
            try:
                actions = json.loads(content)
            except json.JSONDecodeError as e:
                raise OwnerDataError(f"failed to parse extracted.rs as JSON: {e}. File contents: {content!r}")
            validate_action_list(actions)
            break
    names = [name for _, name in payload]
    if len(names) != len(set(names)):
        raise OwnerDataError("internal error: duplicate bundle member")
    manifest = build_manifest(payload, game_dir, include_sounds)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(output.suffix + ".partial")
    if temporary.exists():
        temporary.unlink()
    with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_STORED, allowZip64=True) as bundle:
        bundle.writestr("manifest.json", json.dumps(manifest, indent=2) + "\n")
        for source, archive_name in payload:
            print(f"Bundling {archive_name}")
            bundle.write(source, archive_name)
    temporary.replace(output)
    validate_bundle(output)
    return output


def validate_bundle(bundle_path: Path) -> dict[str, Any]:
    if not bundle_path.is_file():
        raise OwnerDataError(f"bundle does not exist: {bundle_path}")
    with zipfile.ZipFile(bundle_path) as bundle:
        names = bundle.namelist()
        if len(names) != len(set(names)):
            raise OwnerDataError("bundle contains duplicate member names")
        normalized = [str(safe_member(name)) for name in names]
        if len(normalized) != len(set(normalized)):
            raise OwnerDataError("bundle contains paths that normalize to the same destination")
        try:
            manifest = json.loads(bundle.read("manifest.json"))
        except (KeyError, json.JSONDecodeError) as error:
            raise OwnerDataError(f"bundle manifest is missing or invalid: {error}") from error
        if manifest.get("schema_version") != SCHEMA_VERSION or manifest.get("kind") != "SkyRing owner data":
            raise OwnerDataError("unsupported owner-data bundle schema")
        files = manifest.get("files")
        if not isinstance(files, dict):
            raise OwnerDataError("bundle manifest has no file table")
        if set(names) != {"manifest.json", *files.keys()}:
            raise OwnerDataError("bundle contents do not exactly match the manifest")
        for required in ("generated/src/sim/extracted.rs", "generated/assets/player_anims.bin"):
            if required not in files:
                raise OwnerDataError(f"bundle is missing required generated output: {required}")
        for name, metadata in files.items():
            safe_member(name)
            if not isinstance(metadata, dict):
                raise OwnerDataError(f"invalid metadata for {name}")
            digest = hashlib.sha256()
            size = 0
            with bundle.open(name) as source:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    size += len(chunk)
                    digest.update(chunk)
            if size != metadata.get("size") or digest.hexdigest() != metadata.get("sha256"):
                raise OwnerDataError(f"bundle member failed size/hash verification: {name}")
    return manifest


def copy_zip_member(bundle: zipfile.ZipFile, name: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(destination.suffix + ".partial")
    with bundle.open(name) as source, temporary.open("wb") as output:
        shutil.copyfileobj(source, output, length=1024 * 1024)
    temporary.replace(destination)


def import_bundle(bundle_path: Path, repo: Path, force: bool) -> Path:
    manifest = validate_bundle(bundle_path)
    if not (repo / "verification" / "contract.json").is_file() or not (repo / "core" / "src" / "sim").is_dir():
        raise OwnerDataError(f"import destination is not a SkyRing checkout: {repo}")
    fingerprint = sha256(bundle_path)[:16]
    private_root = repo / "owner_data" / fingerprint
    if private_root.exists():
        raise OwnerDataError(f"this bundle is already imported: {private_root}")
    action_table = repo / "core" / "src" / "sim" / "extracted.rs"
    expected_action_hash = manifest["files"]["generated/src/sim/extracted.rs"]["sha256"]
    if action_table.exists() and sha256(action_table) != expected_action_hash and not force:
        raise OwnerDataError(
            f"a different extracted.rs already exists at {action_table}; pass --force to replace it"
        )

    with zipfile.ZipFile(bundle_path) as bundle:
        for name in manifest["files"]:
            member = safe_member(name)
            if member.parts[0] == "source":
                destination = private_root.joinpath(*member.parts)
            elif name == "generated/src/sim/extracted.rs":
                destination = action_table
                # Validate the action list before writing
                with bundle.open(name) as f:
                    content = f.read().decode("utf-8")
                try:
                    actions = json.loads(content)
                except json.JSONDecodeError as e:
                    raise OwnerDataError(f"failed to parse extracted.rs as JSON: {e}")
                validate_action_list(actions)
            else:
                destination = private_root.joinpath(*member.parts)
            copy_zip_member(bundle, name, destination)
    (private_root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return private_root


def default_bundle_path(work_dir: Path) -> Path:
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return work_dir / "bundles" / f"SkyRing-owner-data-{timestamp}.zip"


def command_plan(args: argparse.Namespace) -> None:
    game_dir = find_game_dir(args.game_dir)
    validate_game_dir(game_dir)
    source_dir = args.work_dir.resolve() / "source"
    print_nuxe_plan(game_dir, source_dir, args.include_sounds)
    print(f"Pinned Nuxe:      {NUXE_VERSION} ({NUXE_SHA256})")
    print(f"Pinned WitchyBND: {WITCHY_VERSION} ({WITCHY_SHA256})")
    print(f"Pinned generator: {UPSTREAM_COMMIT} ({UPSTREAM_SHA256})")


def command_bootstrap(args: argparse.Namespace) -> None:
    work_dir = args.work_dir.resolve()
    nuxe, witchy = bootstrap_tools(work_dir, args.offline)
    upstream = prepare_upstream(work_dir, args.offline)
    print(f"Nuxe:      {nuxe}")
    print(f"WitchyBND: {witchy}")
    print(f"Generator: {upstream} ({UPSTREAM_COMMIT})")


def command_dump(args: argparse.Namespace) -> None:
    if os.name != "nt" and not args.skip_nuxe:
        raise OwnerDataError("the Nuxe dump step requires Windows; use --skip-nuxe only with a complete source directory")
    work_dir = args.work_dir.resolve()
    source_dir = work_dir / "source"
    game_dir = find_game_dir(args.game_dir)
    validate_game_dir(game_dir)
    nuxe, witchy = bootstrap_tools(work_dir, args.offline)
    if args.skip_nuxe:
        validate_nuxe_output(source_dir, args.include_sounds)
    else:
        launch_nuxe(nuxe, game_dir, source_dir, args.include_sounds)
    shutil.copy2(game_dir / "oo2core_6_win64.dll", source_dir / "oo2core_6_win64.dll")
    run_witchy(witchy, game_dir, source_dir, args.reuse)
    upstream = prepare_upstream(work_dir, args.offline)
    run_generators(upstream, game_dir, source_dir)
    output = args.output.resolve() if args.output else default_bundle_path(work_dir)
    bundle = create_bundle(
        source_dir,
        upstream,
        game_dir,
        args.include_sounds,
        output,
        args.force,
    )
    print(f"\nOwner-data bundle complete: {bundle}")
    print(f"SHA-256: {sha256(bundle)}")
    print("On the Mac, run:")
    print(f"  python3 tools/owner_data.py import-bundle \"{bundle.name}\"")
    print("  python3 tools/verify.py --profile source")


def command_validate(args: argparse.Namespace) -> None:
    manifest = validate_bundle(args.bundle.resolve())
    print(f"PASS: {args.bundle}")
    print(f"Files: {len(manifest['files'])}")
    print(f"Upstream commit: {manifest['upstream']['commit']}")
    print(f"Bundle SHA-256: {sha256(args.bundle.resolve())}")


def command_import(args: argparse.Namespace) -> None:
    destination = import_bundle(args.bundle.resolve(), args.repo.resolve(), args.force)
    print(f"Imported private owner data to: {destination}")
    print("Installed action table: core/src/sim/extracted.rs")
    print("Next: python3 tools/verify.py --profile source")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)

    def work_options(command: argparse.ArgumentParser) -> None:
        command.add_argument("--work-dir", type=Path, default=Path("skyring-owner-dump"))
        command.add_argument("--offline", action="store_true", help="use only already downloaded, hash-verified archives")

    plan = subcommands.add_parser("plan", help="show the exact Nuxe extraction plan")
    plan.add_argument("--game-dir", type=Path)
    plan.add_argument("--work-dir", type=Path, default=Path("skyring-owner-dump"))
    plan.add_argument(
        "--no-sounds",
        dest="include_sounds",
        action="store_false",
        default=True,
        help="omit the optional player sound banks and generated audio",
    )
    plan.set_defaults(function=command_plan)

    bootstrap = subcommands.add_parser("bootstrap", help="download and verify Nuxe and WitchyBND")
    work_options(bootstrap)
    bootstrap.set_defaults(function=command_bootstrap)

    dump = subcommands.add_parser("dump", help="extract, generate, and bundle all owner-local data")
    work_options(dump)
    dump.add_argument("--game-dir", type=Path)
    dump.add_argument(
        "--no-sounds",
        dest="include_sounds",
        action="store_false",
        default=True,
        help="omit the optional player sound banks and generated audio",
    )
    dump.add_argument("--skip-nuxe", action="store_true", help="validate and reuse work-dir/source instead of launching Nuxe")
    dump.add_argument("--reuse", action="store_true", help="reuse an existing valid regulation-bin folder")
    dump.add_argument("--output", type=Path)
    dump.add_argument("--force", action="store_true", help="replace an existing output bundle")
    dump.set_defaults(function=command_dump)

    validate = subcommands.add_parser("validate-bundle", help="verify every member against the bundle manifest")
    validate.add_argument("bundle", type=Path)
    validate.set_defaults(function=command_validate)

    import_command = subcommands.add_parser("import-bundle", help="install a private bundle into a SkyRing checkout")
    import_command.add_argument("bundle", type=Path)
    import_command.add_argument("--repo", type=Path, default=ROOT)
    import_command.add_argument("--force", action="store_true", help="replace a different existing extracted.rs")
    import_command.set_defaults(function=command_import)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        args.function(args)
    except (OwnerDataError, OSError, zipfile.BadZipFile) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
