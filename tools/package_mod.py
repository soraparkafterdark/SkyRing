#!/usr/bin/env python3
"""Stage the SkyRing mod archive from explicit inputs, with a forbidden-file gate.

Only the paths in `ALLOWED_MEMBERS` can enter the archive; anything else found
under `plugin/dist` stops packaging rather than being skipped silently. The
resulting inventory is checked for owner data, generated animation/audio,
logs, and secrets. The zip is byte-for-byte reproducible from its inputs.

A missing Windows DLL is BLOCKED (exit 2), never a pass.
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "plugin" / "dist"
EXTRACTED = ROOT / "core" / "src" / "sim" / "extracted.rs"
DEFAULT_OUT = ROOT / "verification" / "out" / "package"

# archive path -> required? (binaries are required; the ini ships as source)
ALLOWED_MEMBERS: dict[str, bool] = {
    "SKSE/Plugins/ERCombat.dll": True,
    "SKSE/Plugins/er_combat_core.dll": True,
    "SKSE/Plugins/ERCombat.ini": True,
}

# Matched case-insensitively against each archive path and each of its parts.
FORBIDDEN_PATTERNS: dict[str, tuple[str, ...]] = {
    "owner data": (
        "extracted.rs", "owner_data", "owner-data*", "eldenring.exe", "regulation*", "oo2core*",
        "*.param", "*.anibnd*", "*.bnd", "*.dcx", "*.hkx", "*.tae",
    ),
    "generated animation or audio": (
        "player_anims.bin", "player_sounds.bin", "*.bnk", "*.wav", "*.fsb", "*.fev", "*.wem", "*.ogg",
    ),
    "log or runtime evidence": (
        "*.log", "*.pdb", "runtime-evidence*.json", "last-run.json",
    ),
    "secret": (
        ".env", ".env.*", "*.pem", "*.key", "*.p12", "id_rsa*", "id_ed25519*", "credentials*", "*token*", "*.secret",
    ),
}
SECRET_TEXT = re.compile(
    rb"(ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9]{20,}|"
    rb"-----BEGIN [A-Z ]*PRIVATE KEY-----|AKIA[0-9A-Z]{16})"
)
# A packaged build must not carry the packager's home directory.
LOCAL_PATH_TEXT = re.compile(rb"/Users/[A-Za-z0-9._-]+/|[A-Za-z]:\\Users\\[A-Za-z0-9._-]+\\")


class PackageError(RuntimeError):
    pass


class Blocked(PackageError):
    pass


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def forbidden_reason(archive_path: str) -> str | None:
    pure = PurePosixPath(archive_path)
    candidates = [pure.name, *pure.parts, archive_path]
    for reason, patterns in FORBIDDEN_PATTERNS.items():
        for pattern in patterns:
            if any(fnmatch.fnmatch(candidate.lower(), pattern) for candidate in candidates):
                return f"{reason} (matches {pattern!r})"
    return None


def check_content(archive_path: str, data: bytes) -> None:
    if SECRET_TEXT.search(data):
        raise PackageError(f"{archive_path}: contains what looks like a secret")
    if archive_path.endswith((".ini", ".json", ".txt", ".md")) and LOCAL_PATH_TEXT.search(data):
        raise PackageError(f"{archive_path}: contains a local home-directory path")


def collect_inputs(dist: Path) -> dict[str, bytes]:
    """Read the explicit inputs. Missing binaries are BLOCKED; strays fail closed."""
    if not dist.is_dir():
        raise Blocked(f"distribution directory is missing: {dist}")
    present = {
        path.relative_to(dist).as_posix(): path
        for path in sorted(dist.rglob("*"))
        if path.is_file() or path.is_symlink()
    }
    for name, path in present.items():
        if path.is_symlink():
            raise PackageError(f"symbolic links are not packaged: {name}")
    strays = sorted(set(present) - set(ALLOWED_MEMBERS))
    if strays:
        reasons = [f"{name} ({forbidden_reason(name) or 'not in the allowed input list'})" for name in strays]
        raise PackageError("unexpected files under the distribution directory: " + "; ".join(reasons))
    missing = sorted(name for name, required in ALLOWED_MEMBERS.items() if required and name not in present)
    if missing:
        raise Blocked("required inputs are not present (build them on Windows first): " + ", ".join(missing))
    return {name: present[name].read_bytes() for name in sorted(present)}


def build_inventory(members: dict[str, bytes], *, owner_build: bool) -> dict:
    files = []
    for name in sorted(members):
        data = members[name]
        reason = forbidden_reason(name)
        if reason is not None:
            raise PackageError(f"forbidden file in the artifact inventory: {name}: {reason}")
        check_content(name, data)
        files.append({"path": name, "size": len(data), "sha256": sha256_bytes(data)})
    return {
        "schema_version": 1,
        "kind": "SkyRing mod artifact inventory",
        "synthetic": not owner_build,
        "contains_owner_data": owner_build,
        "files": files,
    }


def write_zip(members: dict[str, bytes], destination: Path) -> str:
    """Write a byte-reproducible zip (sorted, fixed timestamps and modes, stored)."""
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(destination.suffix + ".partial")
    with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_STORED) as archive:
        for name in sorted(members):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = 0o100644 << 16
            info.create_system = 3
            archive.writestr(info, members[name])
    temporary.replace(destination)
    return sha256_bytes(destination.read_bytes())


def package(dist: Path, out_dir: Path, *, name: str = "SkyRing-synthetic", owner_build: bool = False,
            extracted: Path = EXTRACTED) -> tuple[Path, dict]:
    if extracted.exists() and not owner_build:
        raise PackageError(
            f"{extracted} exists, so a build here embeds owner-derived data; "
            "refusing to package it as a synthetic artifact (pass --owner-build for a private build)"
        )
    members = collect_inputs(dist)
    inventory = build_inventory(members, owner_build=owner_build)
    suffix = "-OWNER-PRIVATE" if owner_build else ""
    archive = out_dir / f"{name}{suffix}.zip"
    inventory["archive"] = {"name": archive.name, "sha256": write_zip(members, archive)}
    (out_dir / f"{archive.name}.inventory.json").write_text(
        json.dumps(inventory, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return archive, inventory


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", type=Path, default=DIST)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--owner-build", action="store_true", help="private build that embeds owner data")
    args = parser.parse_args()
    try:
        archive, inventory = package(args.dist, args.out, owner_build=args.owner_build)
    except Blocked as error:
        print(f"BLOCKED  package: {error}", file=sys.stderr)
        return 2
    except PackageError as error:
        print(f"FAIL  package: {error}", file=sys.stderr)
        return 1
    print(f"PASS  package: {archive} ({len(inventory['files'])} files, sha256 {inventory['archive']['sha256']})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
