#!/usr/bin/env python3
"""Evidence fingerprinting tool.

Computes a deterministic fingerprint of files in a git repository.
"""

from __future__ import annotations

import hashlib
import json
import platform
import subprocess
import sys
from pathlib import Path
from typing import Any, Dict, List


class EvidenceError(RuntimeError):
    """Errors from the evidence tool."""


def _run_git(args: List[str], cwd: Path) -> str:
    """Run a git command and return stdout stripped."""
    try:
        result = subprocess.run(
            ["git"] + args,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            check=True,
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        raise EvidenceError(f"git {' '.join(args)} failed") from e


def fingerprint(paths: List[Path], repo: Path) -> Dict[str, Any]:
    """Return a fingerprint dict for the given paths in the git repo.

    The dict contains:
      commit: SHA-1 of HEAD
      dirty: True if git status --porcelain is non-empty
      files: mapping from path string to {sha256: hex, bytes: size}
      platform: {system, release, machine, python}
    """
    # Git info
    commit = _run_git(["rev-parse", "HEAD"], repo)
    dirty = bool(_run_git(["status", "--porcelain"], repo))

    # File hashes
    files: Dict[str, Dict[str, Any]] = {}
    for p in paths:
        if not p.is_file():
            raise EvidenceError(f"Not a file: {p}")
        data = p.read_bytes()
        files[str(p)] = {
            "sha256": hashlib.sha256(data).hexdigest(),
            "bytes": len(data),
        }

    # Platform info
    plat = platform.uname()
    platform_info = {
        "system": plat.system,
        "release": plat.release,
        "machine": plat.machine,
        "python": platform.python_version(),
    }

    return {
        "commit": commit,
        "dirty": dirty,
        "files": files,
        "platform": platform_info,
    }


VALID_PROFILES = {"local", "source", "release"}


class ValidationError(RuntimeError):
    """Errors from manifest validation."""


def validate(manifest: Dict[str, Any]) -> List[str]:
    """Validate a runtime evidence manifest.

    Returns a list of error strings; an empty list means valid.
    """
    errors = []

    # Check required git_commit field
    if "git_commit" not in manifest:
        errors.append("missing required field: git_commit")
    elif not isinstance(manifest["git_commit"], str) or len(manifest["git_commit"]) != 40:
        errors.append("git_commit must be a 40-character SHA-1 hex string")
    else:
        # Validate it's lowercase hex
        if not all(c in '0123456789abcdef' for c in manifest["git_commit"]):
            errors.append("git_commit must be a 40-character lowercase hex string")

    # Check data_fingerprint field
    if "data_fingerprint" not in manifest:
        errors.append("missing required field: data_fingerprint")
    elif not isinstance(manifest["data_fingerprint"], str) or len(manifest["data_fingerprint"]) != 64:
        errors.append("data_fingerprint must be a 64-character SHA-256 hex string")
    else:
        # Validate it's lowercase hex
        if not all(c in '0123456789abcdef' for c in manifest["data_fingerprint"]):
            errors.append("data_fingerprint must be a 64-character lowercase hex string")

    # Check profile field
    if "profile" not in manifest:
        errors.append("missing required field: profile")
    elif manifest["profile"] not in VALID_PROFILES:
        errors.append(f"invalid profile: {manifest['profile']!r}; must be one of {sorted(VALID_PROFILES)}")

    # Check checks array
    if "checks" not in manifest:
        errors.append("missing required field: checks")
    elif not isinstance(manifest["checks"], list):
        errors.append("checks must be an array")
    else:
        for i, check in enumerate(manifest["checks"]):
            if not isinstance(check, dict):
                errors.append(f"checks[{i}] must be an object")
            elif "name" not in check or "status" not in check:
                errors.append(f"checks[{i}] missing required 'name' or 'status' field")

    return errors


def _is_tracked(path: Path, repo: Path) -> bool:
    """Return True if path is tracked by git."""
    try:
        _run_git(["ls-files", "--error-unmatch", str(path)], repo)
        return True
    except EvidenceError:
        return False


def _is_ignored(path: Path, repo: Path) -> bool:
    """Return True if path is ignored by git."""
    try:
        _run_git(["check-ignore", "-q", str(path)], repo)
        return True
    except EvidenceError:
        return False


def main() -> None:
    import argparse

    parser = argparse.ArgumentParser(description="Compute evidence fingerprint")
    subparsers = parser.add_subparsers(dest="command")
    fingerprint_parser = subparsers.add_parser("fingerprint", help="Compute fingerprint of files")
    fingerprint_parser.add_argument(
        "files",
        nargs="+",
        type=Path,
        help="Files to fingerprint",
    )
    fingerprint_parser.add_argument(
        "--out",
        type=Path,
        default=Path("verification/out/evidence.json"),
        help="Output JSON file (default: verification/out/evidence.json)",
    )
    args = parser.parse_args()

    if args.command != "fingerprint":
        sys.stderr.write("Error: expected 'fingerprint' subcommand\n")
        sys.exit(2)

    # Find the git repository root
    try:
        repo_str = _run_git(["rev-parse", "--show-toplevel"], Path("."))
    except EvidenceError:
        sys.stderr.write("Error: not in a git repository\n")
        sys.exit(2)
    repo = Path(repo_str)

    # Resolve output path relative to current working directory
    out_path = args.out.resolve()
    # Check if output path is tracked or not ignored
    if _is_tracked(out_path, repo) or not _is_ignored(out_path, repo):
        sys.stderr.write(
            f"Error: {out_path} is tracked by git or not ignored by git\n"
        )
        sys.exit(2)

    # Compute fingerprint
    result = fingerprint(args.files, repo)

    # Write output
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()