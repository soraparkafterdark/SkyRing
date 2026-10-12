#!/usr/bin/env python3
"""Inspect an owned Skyrim SE/AE install on SteamOS (Steam + Proton).

SteamOS has no Windows registry, drive letters, or PowerShell, and Skyrim runs
inside a Proton prefix. This tool finds the install through Steam's own
library files, reads the build identity (Steam build ID, SkyrimSE.exe file
version and hashes, base masters), and checks the runtime prerequisites the
SkyRing plugin needs: SKSE for that exact exe version, the matching Address
Library file, and the Proton prefix where SKSE logs land.

It reads the install and never modifies it. Nothing game-derived is written to
Git: the report goes to the ignored verification/out/ directory.

A missing install, prefix, SKSE build, or Address Library file is BLOCKED, not
a pass. A clean report here is a SteamOS/Proton preflight. It is not
`release` evidence, which is tied to CrossOver on Apple Silicon.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import re
import sys
from typing import Any, Iterable

sys.path.insert(0, str(Path(__file__).resolve().parent))
import skyrim_re  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
SCHEMA_VERSION = 1
REPORT_KIND = "SkyRing SteamOS Skyrim probe"
DEFAULT_REPORT = Path("verification/out/skyrim-steamos.json")

STEAM_APP_ID = "489830"  # Skyrim SE and AE share this app ID.
LEGENDARY_APP_ID = "72850"  # Original/Legendary Edition: unsupported.
TARGET_VERSION = "1.6.1170"  # the runtime the plugin is built and tested for.
STEAM_ROOT_CANDIDATES = (
    "~/.local/share/Steam",
    "~/.steam/steam",
    "~/.steam/root",
    "~/.var/app/com.valvesoftware.Steam/.local/share/Steam",
    "~/.var/app/com.valvesoftware.Steam/data/Steam",
)
BASE_MASTERS = ("Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm")
PLUGIN_FILES = ("ERCombat.dll", "er_combat_core.dll", "ERCombat.ini")
STATE_FULLY_INSTALLED = 4


class SteamOSProbeError(RuntimeError):
    """The install cannot be located or read. Callers report this as BLOCKED."""


# --- Valve KeyValues (appmanifest, libraryfolders, config.vdf) ---------------

_VDF_TOKEN = re.compile(r'"((?:[^"\\]|\\.)*)"|([{}])')
_VDF_ESCAPES = {"n": "\n", "t": "\t", "\\": "\\", '"': '"'}


def _vdf_unescape(value: str) -> str:
    return re.sub(r"\\(.)", lambda m: _VDF_ESCAPES.get(m.group(1), m.group(0)), value)


def parse_vdf(text: str) -> dict[str, Any]:
    lines = [line for line in text.splitlines() if not line.lstrip().startswith("//")]
    stack: list[dict[str, Any]] = [{}]
    key: str | None = None
    for match in _VDF_TOKEN.finditer("\n".join(lines)):
        if match.group(2) == "{":
            if key is None:
                raise SteamOSProbeError("malformed Steam file: '{' without a key")
            child: dict[str, Any] = {}
            stack[-1][key] = child
            stack.append(child)
            key = None
        elif match.group(2) == "}":
            if len(stack) == 1 or key is not None:
                raise SteamOSProbeError("malformed Steam file: unbalanced '}'")
            stack.pop()
        else:
            token = _vdf_unescape(match.group(1))
            if key is None:
                key = token
            else:
                stack[-1][key] = token
                key = None
    if len(stack) != 1 or key is not None:
        raise SteamOSProbeError("malformed Steam file: truncated")
    return stack[0]


def vdf_get(node: Any, *path: str) -> Any:
    """Case-insensitive descent; Steam files are inconsistent about key case."""
    for part in path:
        if not isinstance(node, dict):
            return None
        match = next((k for k in node if k.lower() == part.lower()), None)
        if match is None:
            return None
        node = node[match]
    return node


def read_vdf(path: Path) -> dict[str, Any]:
    try:
        return parse_vdf(path.read_text(encoding="utf-8", errors="replace"))
    except OSError as error:
        raise SteamOSProbeError(f"cannot read {path}: {error}") from error


# --- Filesystem helpers (Linux is case-sensitive; Windows-era names are not) --

def find_ci(directory: Path, name: str) -> Path | None:
    exact = directory / name
    if exact.exists():
        return exact
    try:
        lowered = name.lower()
        return next((entry for entry in directory.iterdir() if entry.name.lower() == lowered), None)
    except OSError:
        return None


def find_path_ci(base: Path, *parts: str) -> Path | None:
    current: Path | None = base
    for part in parts:
        current = find_ci(current, part) if current is not None else None
    return current


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def _unique_dirs(paths: Iterable[Path]) -> list[Path]:
    seen: set[Path] = set()
    result: list[Path] = []
    for path in paths:
        try:
            resolved = path.resolve()
        except OSError:
            continue
        if resolved not in seen and resolved.is_dir():
            seen.add(resolved)
            result.append(resolved)
    return result


# --- Steam discovery ----------------------------------------------------------

def steam_roots(override: Path | None = None) -> list[Path]:
    if override is not None:
        if not override.is_dir():
            raise SteamOSProbeError(f"--steam-root is not a directory: {override}")
        return [override.resolve()]
    roots = _unique_dirs(Path(candidate).expanduser() for candidate in STEAM_ROOT_CANDIDATES)
    return [root for root in roots if (root / "steamapps").is_dir() or (root / "config").is_dir()]


def steam_libraries(roots: Iterable[Path]) -> list[Path]:
    """Every library folder, including SD-card and external-drive libraries."""
    candidates: list[Path] = []
    for root in roots:
        candidates.append(root)
        folders = root / "steamapps" / "libraryfolders.vdf"
        if not folders.is_file():
            continue
        tree = vdf_get(read_vdf(folders), "libraryfolders")
        if not isinstance(tree, dict):
            continue
        for entry in tree.values():
            path = entry if isinstance(entry, str) else vdf_get(entry, "path")  # pre-2021 format is a bare path
            if isinstance(path, str):
                candidates.append(Path(path))
    return [lib for lib in _unique_dirs(candidates) if (lib / "steamapps").is_dir()]


def read_manifest(path: Path) -> dict[str, Any]:
    state = vdf_get(read_vdf(path), "AppState")
    if not isinstance(state, dict):
        raise SteamOSProbeError(f"{path} has no AppState")
    flags_text = vdf_get(state, "StateFlags")
    return {
        "path": str(path),
        "app_id": vdf_get(state, "appid"),
        "name": vdf_get(state, "name"),
        "installdir": vdf_get(state, "installdir"),
        "buildid": vdf_get(state, "buildid"),
        "target_buildid": vdf_get(state, "TargetBuildID"),
        "state_flags": int(flags_text) if isinstance(flags_text, str) and flags_text.isdigit() else None,
        "last_updated": vdf_get(state, "LastUpdated"),
        "size_on_disk": vdf_get(state, "SizeOnDisk"),
    }


def locate(steam_root: Path | None = None, game_dir: Path | None = None) -> dict[str, Any]:
    """Find the install, its Steam manifest, and its Proton prefix location."""
    roots = steam_roots(steam_root)
    libraries = steam_libraries(roots)
    library: Path | None = None
    manifest: dict[str, Any] | None = None
    for candidate in libraries:
        manifest_path = candidate / "steamapps" / f"appmanifest_{STEAM_APP_ID}.acf"
        if manifest_path.is_file():
            library, manifest = candidate, read_manifest(manifest_path)
            break

    if game_dir is not None:
        if not game_dir.is_dir():
            raise SteamOSProbeError(f"--game-dir is not a directory: {game_dir}")
        resolved_game = game_dir.resolve()
        if library is None and resolved_game.parent.name == "common" and resolved_game.parent.parent.name == "steamapps":
            candidate = resolved_game.parent.parent.parent
            manifest_path = candidate / "steamapps" / f"appmanifest_{STEAM_APP_ID}.acf"
            if manifest_path.is_file():
                library, manifest = candidate, read_manifest(manifest_path)
    else:
        if library is None or manifest is None:
            legendary = any((lib / "steamapps" / f"appmanifest_{LEGENDARY_APP_ID}.acf").is_file() for lib in libraries)
            searched = ", ".join(str(r) for r in roots) or "no Steam installation under the usual SteamOS paths"
            hint = (" Only Skyrim Legendary Edition (app 72850) is installed, which SkyRing does not support."
                    if legendary else "")
            raise SteamOSProbeError(
                f"BLOCKED: Skyrim SE/AE (Steam app {STEAM_APP_ID}) not found. Searched: {searched}.{hint} "
                "Install it through Steam, or pass --steam-root / --game-dir."
            )
        installdir = manifest["installdir"]
        if not isinstance(installdir, str) or not installdir:
            raise SteamOSProbeError(f"{manifest['path']} has no installdir")
        resolved_game = library / "steamapps" / "common" / installdir
        if not resolved_game.is_dir():
            raise SteamOSProbeError(
                f"BLOCKED: Steam lists Skyrim at {resolved_game} but the folder is missing "
                "(an unmounted SD card or external drive?)"
            )
    return {
        "steam_roots": [str(r) for r in roots],
        "library": str(library) if library is not None else None,
        "game_dir": str(resolved_game),
        "manifest": manifest,
        "compatdata_dir": str(library / "steamapps" / "compatdata" / STEAM_APP_ID) if library is not None else None,
    }


# --- Binary inspection --------------------------------------------------------

def inspect_pe(path: Path) -> dict[str, Any]:
    """Header-level identity of an executable, read without loading it."""
    try:
        info = skyrim_re.analyze_pe(path, deep=False)
    except skyrim_re.ReError as error:
        raise SteamOSProbeError(str(error)) from error
    return {key: info[key] for key in ("machine", "is_x64", "timestamp_utc", "steam_drm_wrapper", "file_version")} | {
        "sections": info["section_names"]}


def classify_edition(version: str | None) -> str:
    if version is None:
        return "unknown"
    parts = version.split(".")
    if parts[:2] == ["1", "5"]:
        return "SE"
    if parts[:2] == ["1", "6"]:
        return "AE"
    return "unknown"


def short_version(version: str) -> str:
    """1.6.1170.0 -> 1.6.1170, the form SKSE and the plugin use."""
    return ".".join(version.split(".")[:3])


# --- Prerequisites ------------------------------------------------------------

def check_skse(game_dir: Path, version: str | None) -> dict[str, Any]:
    loader = find_ci(game_dir, "skse64_loader.exe")
    steam_loader = find_ci(game_dir, "skse64_steam_loader.dll")
    runtime_dlls = sorted(p.name for p in game_dir.iterdir() if re.fullmatch(r"(?i)skse64_\d+_\d+_\d+\.dll", p.name))
    expected = f"skse64_{short_version(version).replace('.', '_')}.dll" if version else None
    return {
        "loader_present": loader is not None,
        "steam_loader_present": steam_loader is not None,
        "runtime_dlls": runtime_dlls,
        "expected_runtime_dll": expected,
        "matches_exe": expected is not None and expected.lower() in (name.lower() for name in runtime_dlls),
    }


def expected_address_library(version: str | None) -> str | None:
    if version is None:
        return None
    parts = version.split(".")
    stem = "version" if parts[:2] == ["1", "5"] else "versionlib"
    return f"{stem}-{'-'.join(parts)}.bin"


def check_address_library(data_dir: Path | None, version: str | None) -> dict[str, Any]:
    plugins = find_path_ci(data_dir, "SKSE", "Plugins") if data_dir is not None else None
    present = sorted(p.name for p in plugins.iterdir()
                     if re.fullmatch(r"(?i)version(lib)?-[\d-]+\.bin", p.name)) if plugins else []
    expected = expected_address_library(version)
    return {
        "plugins_dir": str(plugins) if plugins else None,
        "present": present,
        "expected": expected,
        "matches_exe": expected is not None and expected.lower() in (name.lower() for name in present),
    }


def check_plugin(data_dir: Path | None) -> dict[str, bool]:
    plugins = find_path_ci(data_dir, "SKSE", "Plugins") if data_dir is not None else None
    return {name: bool(plugins and find_ci(plugins, name)) for name in PLUGIN_FILES}


def check_data(data_dir: Path | None, hash_data: bool) -> dict[str, Any]:
    result: dict[str, Any] = {"dir": str(data_dir) if data_dir else None, "masters": {}, "archives": [],
                              "creation_club_plugins": []}
    if data_dir is None:
        return result
    for name in BASE_MASTERS:
        path = find_ci(data_dir, name)
        result["masters"][name] = (
            {"size": path.stat().st_size, "sha256": sha256_file(path) if hash_data else None} if path else None
        )
    entries = sorted(data_dir.iterdir(), key=lambda p: p.name.lower())
    result["archives"] = [{"name": p.name, "size": p.stat().st_size} for p in entries
                          if p.suffix.lower() == ".bsa" and p.is_file()]
    result["creation_club_plugins"] = [p.name for p in entries if re.fullmatch(r"(?i)cc[\w-]+\.(esm|esl)", p.name)]
    return result


def check_prefix(compatdata: Path | None, prefix_override: Path | None, roots: list[Path]) -> dict[str, Any]:
    base = prefix_override if prefix_override is not None else compatdata
    prefix = base / "pfx" if base is not None and (base / "pfx").is_dir() else base
    result: dict[str, Any] = {
        "compatdata_dir": str(base) if base else None,
        "prefix_dir": str(prefix) if prefix and prefix.is_dir() else None,
        "proton_build": None,
        "compat_tool_mapping": None,
        "skse_log_dir": None,
        "logs": [],
    }
    if base is not None and (base / "version").is_file():
        result["proton_build"] = (base / "version").read_text(encoding="utf-8", errors="replace").strip()
    for root in roots:
        config = root / "config" / "config.vdf"
        if config.is_file():
            mapping = vdf_get(read_vdf(config), "InstallConfigStore", "Software", "Valve", "Steam", "CompatToolMapping")
            tool = vdf_get(mapping, STEAM_APP_ID, "name") or vdf_get(mapping, "0", "name")
            if isinstance(tool, str):
                result["compat_tool_mapping"] = tool
                break
    if result["prefix_dir"]:
        log_dir = find_path_ci(Path(result["prefix_dir"]), "drive_c", "users", "steamuser", "Documents",
                               "My Games", "Skyrim Special Edition", "SKSE")
        if log_dir:
            result["skse_log_dir"] = str(log_dir)
            result["logs"] = sorted(p.name for p in log_dir.iterdir() if p.suffix.lower() == ".log")
    return result


def platform_facts() -> dict[str, Any]:
    release: dict[str, str] = {}
    try:
        for line in Path("/etc/os-release").read_text(encoding="utf-8").splitlines():
            key, sep, value = line.partition("=")
            if sep:
                release[key] = value.strip().strip('"')
    except OSError:
        pass
    return {
        "system": platform.system(),
        "machine": platform.machine(),
        "os_id": release.get("ID"),
        "os_pretty_name": release.get("PRETTY_NAME"),
        "os_version_id": release.get("VERSION_ID"),
        "is_steamos": release.get("ID") == "steamos" or "steamos" in release.get("ID_LIKE", ""),
    }


# --- Probe --------------------------------------------------------------------

def probe(steam_root: Path | None = None, game_dir: Path | None = None, prefix: Path | None = None,
          hash_data: bool = True) -> dict[str, Any]:
    location = locate(steam_root, game_dir)
    game = Path(location["game_dir"])
    manifest = location["manifest"]
    blockers: list[str] = []
    warnings: list[str] = []

    if manifest is None:
        warnings.append("no Steam appmanifest found; the Steam build ID is unrecorded")
    else:
        flags = manifest["state_flags"]
        if flags is None or not flags & STATE_FULLY_INSTALLED:
            blockers.append(f"Steam does not report a fully installed game (StateFlags={flags})")
        elif flags != STATE_FULLY_INSTALLED:
            warnings.append(f"Steam has pending state bits (StateFlags={flags}); let any update finish first")
        if manifest["target_buildid"] and manifest["target_buildid"] != manifest["buildid"]:
            warnings.append(f"Steam is mid-update (build {manifest['buildid']} -> {manifest['target_buildid']})")

    exe_path = find_ci(game, "SkyrimSE.exe")
    exe: dict[str, Any] | None = None
    version: str | None = None
    if exe_path is None:
        blockers.append(f"SkyrimSE.exe not found in {game}")
    else:
        exe = {"path": str(exe_path), "size": exe_path.stat().st_size, "sha256": sha256_file(exe_path),
               **inspect_pe(exe_path)}
        version = exe["file_version"]
        if version is None:
            blockers.append("SkyrimSE.exe has no readable file version; cannot pick SKSE or Address Library")
        elif not exe["is_x64"]:
            blockers.append(f"SkyrimSE.exe is not x64 (machine {exe['machine']})")
        elif short_version(version) != TARGET_VERSION:
            warnings.append(f"installed {short_version(version)}; the plugin targets {TARGET_VERSION}")

    data_dir = find_ci(game, "Data")
    if data_dir is None:
        blockers.append(f"Data folder not found in {game}")
    data = check_data(data_dir, hash_data)
    if data_dir is not None and data["masters"].get("Skyrim.esm") is None:
        blockers.append("Data/Skyrim.esm is missing")

    skse = check_skse(game, version)
    if not skse["loader_present"]:
        blockers.append("skse64_loader.exe is not installed in the game folder")
    elif version is not None and not skse["matches_exe"]:
        blockers.append(f"SKSE build for {version} missing: expected {skse['expected_runtime_dll']}, "
                        f"found {skse['runtime_dlls'] or 'none'}")

    address = check_address_library(data_dir, version)
    if version is not None and not address["matches_exe"]:
        blockers.append(f"Address Library for {short_version(version)} missing: expected {address['expected']} "
                        "in Data/SKSE/Plugins")

    proton = check_prefix(Path(location["compatdata_dir"]) if location["compatdata_dir"] else None, prefix,
                          [Path(r) for r in location["steam_roots"]])
    if proton["prefix_dir"] is None:
        blockers.append("no Proton prefix for Skyrim; enable Steam Play for it and launch the game once")
    if proton["compat_tool_mapping"] is None:
        warnings.append("Steam does not record a compatibility tool for Skyrim; select one in Properties > Compatibility")

    fingerprint_inputs = {
        "steam_build_id": manifest["buildid"] if manifest else None,
        "exe_sha256": exe["sha256"] if exe else None,
        "masters": {name: (entry["sha256"] if entry else None) for name, entry in data["masters"].items()},
    }
    if not hash_data:
        warnings.append("data hashing skipped; the runtime fingerprint is incomplete")
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": REPORT_KIND,
        "generated_utc": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "claim_ceiling": "SteamOS/Proton preflight; not release evidence (release requires CrossOver on Apple Silicon)",
        "platform": platform_facts(),
        "status": "blocked" if blockers else "ready",
        "blockers": blockers,
        "warnings": warnings,
        "location": location,
        "executable": exe,
        "skyrim_version": version,
        "edition": classify_edition(version),
        "data": data,
        "skse": skse,
        "address_library": address,
        "skyring_plugin": check_plugin(data_dir),
        "proton": proton,
        "runtime_fingerprint": hashlib.sha256(canonical_json(fingerprint_inputs)).hexdigest() if exe else None,
        "runtime_fingerprint_complete": bool(exe and hash_data and all(fingerprint_inputs["masters"].values())),
    }


# --- CLI ----------------------------------------------------------------------

def print_summary(report: dict[str, Any]) -> None:
    manifest = report["location"]["manifest"] or {}
    version = report["skyrim_version"]
    print(f"Skyrim {report['edition']} {version or '(version unreadable)'}"
          f"  Steam build {manifest.get('buildid', '?')}"
          f"  [{report['platform']['os_pretty_name'] or report['platform']['system']}]")
    print(f"Game:    {report['location']['game_dir']}")
    print(f"Prefix:  {report['proton']['prefix_dir'] or 'missing'}"
          f"  (Proton: {report['proton']['compat_tool_mapping'] or report['proton']['proton_build'] or 'unknown'})")
    skse, address, plugin = report["skse"], report["address_library"], report["skyring_plugin"]
    print(f"SKSE:    loader={'yes' if skse['loader_present'] else 'NO'}"
          f" runtime={'match' if skse['matches_exe'] else 'MISSING'}")
    print(f"Address: {'match' if address['matches_exe'] else 'MISSING'} ({address['expected']})")
    print("Plugin:  " + ", ".join(f"{name}={'yes' if present else 'no'}" for name, present in plugin.items()))
    if report["runtime_fingerprint"]:
        suffix = "" if report["runtime_fingerprint_complete"] else " (incomplete)"
        print(f"Runtime fingerprint: {report['runtime_fingerprint']}{suffix}")
    for warning in report["warnings"]:
        print(f"WARN: {warning}")
    for blocker in report["blockers"]:
        print(f"BLOCKED: {blocker}")
    print("READY (SteamOS/Proton preflight only)" if report["status"] == "ready" else "NOT READY")


def _write_report(args: argparse.Namespace, name: str, data: Any) -> Path:
    default = Path(f"verification/out/skyrim-steamos-{name}.json")
    out = getattr(args, "out", None) or default
    out = out if out.is_absolute() else ROOT / out
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return out


def _game(args: argparse.Namespace) -> tuple[dict[str, Any], Path, Path | None]:
    location = locate(args.steam_root, args.game_dir)
    game = Path(location["game_dir"])
    return location, game, find_ci(game, "Data")


def _target(args: argparse.Namespace, game: Path, default_name: str, base: Path | None = None) -> Path:
    if getattr(args, "file", None) is not None:
        if not args.file.is_file():
            raise SteamOSProbeError(f"not a file: {args.file}")
        return args.file
    found = find_ci(base or game, default_name)
    if found is None:
        raise SteamOSProbeError(f"{default_name} not found in {base or game}")
    return found


def _mapped(path: Path):
    import mmap
    handle = path.open("rb")
    try:
        return handle, mmap.mmap(handle.fileno(), 0, access=mmap.ACCESS_READ)
    except (ValueError, OSError):
        handle.close()
        raise


def command_exe(args: argparse.Namespace) -> int:
    _, game, _ = _game(args)
    path = _target(args, game, "SkyrimSE.exe")
    try:
        pe = skyrim_re.analyze_pe(path)
    except skyrim_re.ReError as error:
        raise SteamOSProbeError(str(error)) from error
    pe["path"], pe["sha256"] = str(path), sha256_file(path)
    out = _write_report(args, f"pe-{path.stem}", pe)
    print(f"{path.name}  version {pe['file_version']}  built {pe['timestamp_utc']}  image base {pe.get('image_base')}")
    print(f"entry {pe.get('entry_point_rva')}  ASLR={pe.get('aslr')} NX={pe.get('nx')}  sha256 {pe['sha256']}")
    for section in pe["sections"]:
        print(f"  {section['name']:<8} va={section['virtual_address']:#010x} vsize={section['virtual_size']:#010x}"
              f" raw={section['raw_size']:#010x} entropy={section.get('entropy', '?')} {','.join(section['flags'])}")
    if pe["steam_drm_wrapper"]:
        print("NOTE: SteamStub .bind section present; .text is encrypted on disk, so static disassembly and "
              "on-disk byte reads of code are not trustworthy. Dump from a running process instead.")
    if pe.get("packed_sections"):
        print(f"WARN: high-entropy code section(s): {', '.join(pe['packed_sections'])}")
    for entry in pe.get("imports", []):
        print(f"  imports {entry['dll']} ({len(entry['functions'])})")
    exports = pe.get("exports", {}).get("functions", [])
    if exports:
        print(f"  exports {len(exports)}: {', '.join(e['name'] or '#' + str(e['ordinal']) for e in exports[:12])}")
    for entry in pe.get("debug", []):
        if "pdb_path" in entry:
            print(f"  PDB {entry['pdb_path']} guid {entry['pdb_guid']} age {entry['pdb_age']}")
    for error in pe.get("errors", []):
        print(f"WARN: {error}")
    print(f"Report: {out}")
    return 0


def command_address_library(args: argparse.Namespace) -> int:
    _, game, data_dir = _game(args)
    exe = find_ci(game, "SkyrimSE.exe")
    version = skyrim_re.analyze_pe(exe, deep=False)["file_version"] if exe else None
    plugins = find_path_ci(data_dir, "SKSE", "Plugins") if data_dir else None
    if args.file is None and plugins is not None and expected_address_library(version):
        found = find_ci(plugins, expected_address_library(version))
        if found is None:
            raise SteamOSProbeError(f"BLOCKED: Address Library for {version} missing: "
                                    f"expected {expected_address_library(version)} in {plugins}")
        args.file = found
    path = _target(args, game, "versionlib.bin")
    try:
        database = skyrim_re.parse_address_library(path.read_bytes())
    except skyrim_re.ReError as error:
        raise SteamOSProbeError(f"{path.name}: {error}") from error
    print(f"{path.name}: format {database['format']} module {database['module']!r} version {database['version']} "
          f"{len(database['entries'])}/{database['declared_count']} entries")
    if not database["parsed_exactly"]:
        print(f"WARN: parse did not consume the file exactly ({database['bytes_unread']} bytes left); "
              "treat every decoded address as unverified")
    pe = skyrim_re.analyze_pe(exe) if exe and (args.peek and (args.id or args.rva)) else None
    lookups = []
    for ident in args.id:
        hit = skyrim_re.lookup_address(database, ident=int(ident, 0))
        lookups.append(hit or {"id": int(ident, 0), "rva": None})
    for rva in args.rva:
        hit = skyrim_re.lookup_address(database, rva=int(rva, 0))
        lookups.append(hit or {"id": None, "rva": hex(int(rva, 0))})
    for hit in lookups:
        peek = ""
        if pe is not None and hit["rva"]:
            raw = skyrim_re.read_at_rva(exe, pe, int(hit["rva"], 16))
            peek = f"  bytes {raw.hex(' ')}" if raw else "  (not in a file-backed section)"
        print(f"  id {hit['id']} -> rva {hit['rva']}{peek}")
    if pe is not None and pe["steam_drm_wrapper"]:
        print("NOTE: bytes read from a SteamStub-wrapped exe are encrypted on disk; do not decode them.")
    summary = {key: value for key, value in database.items() if key != "entries"}
    summary["lookups"] = lookups
    if args.dump:
        summary["entries"] = {str(i): hex(r) for i, r in database["entries"].items()}
    print(f"Report: {_write_report(args, 'address-library', summary)}")
    return 0 if database["parsed_exactly"] else 1


def command_plugins(args: argparse.Namespace) -> int:
    _, game, data_dir = _game(args)
    plugins = find_path_ci(data_dir, "SKSE", "Plugins") if data_dir else None
    if plugins is None:
        raise SteamOSProbeError("Data/SKSE/Plugins does not exist; no SKSE plugins are installed")
    report = []
    for path in sorted(plugins.iterdir(), key=lambda p: p.name.lower()):
        if path.suffix.lower() != ".dll":
            continue
        entry: dict[str, Any] = {"name": path.name, "size": path.stat().st_size, "sha256": sha256_file(path)}
        try:
            pe = skyrim_re.analyze_pe(path)
        except skyrim_re.ReError as error:
            entry["error"] = str(error)
        else:
            names = {f["name"] for f in pe.get("exports", {}).get("functions", []) if f["name"]}
            entry.update({
                "is_x64": pe["is_x64"], "built": pe["timestamp_utc"], "file_version": pe["file_version"],
                "skse_exports": sorted(n for n in names if n.startswith("SKSEPlugin_")),
                "imports": sorted({i["dll"].lower() for i in pe.get("imports", [])}),
                "pdb": next((d["pdb_path"] for d in pe.get("debug", []) if "pdb_path" in d), None),
            })
            if "SKSEPlugin_Version" in names and "SKSEPlugin_Load" in names:
                entry["loads_on_ae"] = True
            elif "SKSEPlugin_Query" in names:
                entry["loads_on_ae"] = False
                entry["note"] = "SE-only SKSEPlugin_Query plugin; it will not load on 1.6.x"
            else:
                entry["loads_on_ae"] = None
                entry["note"] = "no SKSE plugin exports found (library DLL?)"
        report.append(entry)
    for entry in report:
        state = {True: "loads on AE", False: "SE only", None: "?"}[entry.get("loads_on_ae")]
        print(f"{entry['name']:<32} {state:<12} x64={entry.get('is_x64')} {entry.get('built') or entry.get('error', '')}")
    for name in PLUGIN_FILES[:2]:
        if not any(e["name"].lower() == name.lower() for e in report):
            print(f"MISSING: {name} (the SkyRing plugin is not installed)")
    print(f"Report: {_write_report(args, 'plugins', report)}")
    return 0


def command_esm(args: argparse.Namespace) -> int:
    _, game, data_dir = _game(args)
    path = _target(args, game, "Skyrim.esm", data_dir)
    handle, view = _mapped(path)
    try:
        summary = skyrim_re.summarize_plugin(view)
    except skyrim_re.ReError as error:
        raise SteamOSProbeError(f"{path.name}: {error}") from error
    finally:
        view.close()
        handle.close()
    summary["file"] = path.name
    print(f"{path.name}: master={summary['master']} light={summary['light']} localized={summary['localized']} "
          f"form version {summary['form_version']} declared records {summary.get('record_count')} "
          f"walked {summary['records_walked']}")
    print(f"  masters: {', '.join(summary['masters']) or 'none'}")
    print("  groups: " + ", ".join(f"{k}={v}" for k, v in summary["groups"].items()))
    print(f"Report: {_write_report(args, 'esm-' + path.stem.lower(), summary)}")
    return 0


def command_weapons(args: argparse.Namespace) -> int:
    _, game, data_dir = _game(args)
    path = _target(args, game, "Skyrim.esm", data_dir)
    handle, view = _mapped(path)
    try:
        weapons = skyrim_re.extract_weapons(view)
    except skyrim_re.ReError as error:
        raise SteamOSProbeError(f"{path.name}: {error}") from error
    finally:
        view.close()
        handle.close()
    if args.filter:
        wanted = re.compile(args.filter, re.IGNORECASE)
        weapons = [w for w in weapons if wanted.search(f"{w['editor_id']} {w.get('animation_type', '')}")]
    for weapon in weapons[:args.show]:
        print(f"  {weapon['form_id']} {weapon['editor_id']!s:<32} {weapon.get('animation_type', '?'):<13} "
              f"dmg={weapon.get('damage')} speed={weapon.get('speed')} reach={weapon.get('reach')} "
              f"wt={weapon.get('weight')} value={weapon.get('value')}")
    print(f"{len(weapons)} WEAP record(s) from {path.name} (form IDs are file-local; the top byte is the load order)")
    print("NOTE: field layout follows community docs; names are in the strings tables and are not decoded here.")
    print(f"Report: {_write_report(args, 'weapons-' + path.stem.lower(), weapons)}")
    return 0


def command_strings(args: argparse.Namespace) -> int:
    _, game, _ = _game(args)
    path = _target(args, game, "SkyrimSE.exe")
    handle, view = _mapped(path)
    try:
        found = skyrim_re.extract_strings(view, args.min_length, args.pattern, args.wide, args.limit)
    finally:
        view.close()
        handle.close()
    for item in found[:args.show]:
        print(f"  {item['offset']:#010x} {item['text'][:120]}")
    print(f"{len(found)} string(s) in {path.name}" + (" (limit reached)" if len(found) >= args.limit else ""))
    print(f"Report: {_write_report(args, 'strings-' + path.stem.lower(), found)}")
    return 0


def command_logs(args: argparse.Namespace) -> int:
    location, _, _ = _game(args)
    proton = check_prefix(Path(location["compatdata_dir"]) if location["compatdata_dir"] else None, args.prefix,
                          [Path(r) for r in location["steam_roots"]])
    if not proton["skse_log_dir"]:
        raise SteamOSProbeError("BLOCKED: no SKSE log directory in the Proton prefix; launch Skyrim through "
                                "skse64_loader once (see `launch-help`)")
    log_dir = Path(proton["skse_log_dir"])
    interesting = re.compile(r"plugin|error|fail|disabled|not found|version", re.IGNORECASE)
    report = {}
    for name in proton["logs"]:
        lines = (log_dir / name).read_text(encoding="utf-8", errors="replace").splitlines()
        report[name] = {"lines": len(lines), "notable": [l for l in lines if interesting.search(l)][:200],
                        "tail": lines[-args.tail:]}
        print(f"== {name} ({len(lines)} lines)")
        for line in report[name]["notable"][:40]:
            print(f"  {line}")
        for line in report[name]["tail"]:
            print(f"  | {line}")
    print(f"Report: {_write_report(args, 'logs', report)}")
    return 0


def command_launch_help(_args: argparse.Namespace) -> int:
    print("""Running Skyrim with SKSE under Proton on SteamOS (community-standard method; verify on your install):

1. Install the SKSE64 build that matches SkyrimSE.exe (see `probe`) into the game folder:
   skse64_loader.exe, skse64_<ver>.dll, skse64_steam_loader.dll. Put the matching Address Library
   .bin in Data/SKSE/Plugins. Copy the SkyRing DLLs (built on Windows or CrossOver) to Data/SKSE/Plugins.
2. In Steam: Skyrim > Properties > Compatibility > force a Proton version (Proton-GE or Experimental).
3. Properties > Launch Options (replaces the launcher with the SKSE loader):

   bash -c 'exec "${@/SkyrimSELauncher.exe/skse64_loader.exe}"' -- %command%

4. Launch once, quit, then run `skyrim_steamos.py logs` to see which plugins SKSE loaded.

Plugins must be x64 Windows DLLs; nothing built on SteamOS natively can load into this process.
This is a SteamOS/Proton preflight and is not release evidence (release requires CrossOver on Apple Silicon).""")
    return 0


def command_locate(args: argparse.Namespace) -> int:
    location = locate(args.steam_root, args.game_dir)
    if args.json:
        print(json.dumps(location, indent=2))
    else:
        manifest = location["manifest"] or {}
        print(f"Game dir:   {location['game_dir']}")
        print(f"Build ID:   {manifest.get('buildid', 'unknown')}")
        print(f"compatdata: {location['compatdata_dir']}")
    return 0


def command_probe(args: argparse.Namespace) -> int:
    report = probe(args.steam_root, args.game_dir, args.prefix, hash_data=not args.no_hash_data)
    out = args.out if args.out.is_absolute() else ROOT / args.out
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print_summary(report)
        print(f"Report: {out}")
    return 0 if report["status"] == "ready" else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    subcommands = parser.add_subparsers(dest="command", required=True)

    def add(name: str, function: Any, help_text: str, *, file_arg: bool = False, out: bool = True) -> argparse.ArgumentParser:
        command = subcommands.add_parser(name, help=help_text)
        command.add_argument("--steam-root", type=Path, help="Steam directory (default: search the usual SteamOS paths)")
        command.add_argument("--game-dir", type=Path, help="Skyrim folder, when Steam metadata is unavailable")
        if file_arg:
            command.add_argument("--file", type=Path, help="inspect this file instead of the installed default")
        if out:
            command.add_argument("--out", type=Path, help="report path (default: verification/out/, git-ignored)")
        command.set_defaults(function=function)
        return command

    locate_command = add("locate", command_locate, "find the install and Proton prefix", out=False)
    locate_command.add_argument("--json", action="store_true", help="print JSON instead of a summary")

    probe_command = add("probe", command_probe, "identify the build and check runtime prerequisites")
    probe_command.add_argument("--json", action="store_true", help="print JSON instead of a summary")
    probe_command.add_argument("--prefix", type=Path, help="Proton compatdata/489830 directory, if not beside the game")
    probe_command.add_argument("--no-hash-data", action="store_true", help="skip hashing the (large) base masters")
    probe_command.set_defaults(out=DEFAULT_REPORT)

    add("exe", command_exe, "deep PE analysis: sections, entropy, imports, exports, PDB, DRM wrapper", file_arg=True)

    address = add("address-library", command_address_library, "decode the Address Library and look up IDs/RVAs",
                  file_arg=True)
    address.add_argument("--id", action="append", default=[], help="Address Library ID to resolve (repeatable)")
    address.add_argument("--rva", action="append", default=[], help="RVA to map back to an ID (repeatable)")
    address.add_argument("--peek", action="store_true", help="show on-disk bytes at each RVA (unreliable under SteamStub)")
    address.add_argument("--dump", action="store_true", help="include the full ID table in the report (large)")

    add("plugins", command_plugins, "audit installed SKSE plugin DLLs (AE loadability, hashes, imports)")
    add("esm", command_esm, "plugin header, masters and per-type record counts (default Skyrim.esm)", file_arg=True)

    weapons = add("weapons", command_weapons, "decode WEAP records: damage, speed, reach, animation type", file_arg=True)
    weapons.add_argument("--filter", help="regex over editor ID and animation type, e.g. TwoHand")
    weapons.add_argument("--show", type=int, default=25, help="rows to print (the report holds all)")

    strings = add("strings", command_strings, "printable strings in SkyrimSE.exe or --file", file_arg=True)
    strings.add_argument("--min-length", type=int, default=6)
    strings.add_argument("--pattern", help="regex filter")
    strings.add_argument("--wide", action="store_true", help="UTF-16LE strings")
    strings.add_argument("--limit", type=int, default=100000)
    strings.add_argument("--show", type=int, default=40, help="rows to print (the report holds all)")

    logs = add("logs", command_logs, "SKSE and plugin logs from the Proton prefix")
    logs.add_argument("--prefix", type=Path, help="Proton compatdata/489830 directory, if not beside the game")
    logs.add_argument("--tail", type=int, default=15)

    help_command = subcommands.add_parser("launch-help", help="how to start Skyrim with SKSE under Proton")
    help_command.set_defaults(function=command_launch_help)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return args.function(args)
    except (SteamOSProbeError, OSError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
