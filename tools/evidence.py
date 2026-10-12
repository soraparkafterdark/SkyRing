#!/usr/bin/env python3
"""Evidence fingerprinting tool.

Computes a deterministic, repository-relative fingerprint of files in a Git
repository. A fingerprint records dirty state; callers preparing release
evidence can additionally require an exact clean commit.
"""

from __future__ import annotations

import hashlib
import json
import platform
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath
from typing import Any, Dict, List, Optional


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
      files: mapping from canonical repository-relative path to
             {sha256: hex, bytes: size}
      platform: {system, release, machine, python}
    """
    repo = repo.resolve()

    # Git info
    commit = _run_git(["rev-parse", "HEAD"], repo)
    dirty = bool(_run_git(["status", "--porcelain", "--untracked-files=all"], repo))

    # File hashes. Never serialize an absolute host path into evidence, and do
    # not let a symlink make a file outside the checkout appear repository-local.
    canonical: Dict[str, Path] = {}
    for supplied in paths:
        candidate = supplied if supplied.is_absolute() else repo / supplied
        if candidate.is_symlink():
            raise EvidenceError(f"Symbolic links are not valid evidence inputs: {supplied}")
        resolved = candidate.resolve()
        try:
            relative = resolved.relative_to(repo).as_posix()
        except ValueError as error:
            raise EvidenceError(f"Evidence input is outside the repository: {supplied}") from error
        if relative in canonical:
            raise EvidenceError(f"Duplicate evidence input: {relative}")
        if not resolved.is_file():
            raise EvidenceError(f"Not a file: {supplied}")
        canonical[relative] = resolved

    files: Dict[str, Dict[str, Any]] = {}
    for relative in sorted(canonical):
        data = canonical[relative].read_bytes()
        files[relative] = {
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
        "schema_version": 1,
        "commit": commit,
        "dirty": dirty,
        "clean_commit_capture": not dirty,
        "files": files,
        "platform": platform_info,
    }


VALID_PROFILES = {"local", "source", "release"}


class ValidationError(RuntimeError):
    """Errors from manifest validation."""


def validate(manifest: Dict[str, Any]) -> List[str]:
    """Validate a verification-summary manifest (profile + checks).

    This is not the release evidence format; see `validate_runtime_evidence`.
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


# --- Release runtime evidence ------------------------------------------------
#
# `verification/runtime-evidence.json` (ignored) records one CrossOver run of
# one exact commit against one source-data fingerprint. `validate_runtime_
# evidence` is the single authority on whether it is acceptable; tools/verify.py
# calls it for the release profile and `capture` runs it before writing.

EVIDENCE_SCHEMA_VERSION = 2
EVIDENCE_ARTIFACTS = (
    "plugin/dist/SKSE/Plugins/ERCombat.dll",
    "plugin/dist/SKSE/Plugins/er_combat_core.dll",
)
EVIDENCE_TOP_LEVEL = {
    "schema_version",
    "commit",
    "working_tree_clean",
    "source_fingerprint",
    "action_table_sha256",
    "capture_manifest",
    "elden_ring_build",
    "skyrim_version",
    "crossover_version",
    "apple_hardware",
    "artifacts",
    "runtime_log",
    "acceptance",
    "acceptance_evidence",
    "notes",
}
RUNTIME_FACT_FIELDS = ("elden_ring_build", "skyrim_version", "crossover_version", "apple_hardware")
_SHA256 = re.compile(r"[0-9a-f]{64}")
_COMMIT = re.compile(r"[0-9a-f]{40}")
_PLACEHOLDER = re.compile(r"replace-with|record-|\btodo\b|\btbd\b|placeholder|<[^>]*>|^\s*$", re.IGNORECASE)
_EVENT_ID = re.compile(r"[A-Za-z0-9_.:/-]{3,128}")


def is_placeholder(value: Any) -> bool:
    return not isinstance(value, str) or _PLACEHOLDER.search(value) is not None


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _repo_file(repo: Path, raw: Any, label: str, errors: List[str]) -> Optional[Path]:
    """Resolve a repository-relative evidence path, refusing escapes and links."""
    if not isinstance(raw, str) or is_placeholder(raw):
        errors.append(f"{label}: path is missing or a placeholder")
        return None
    relative = PurePosixPath(raw)
    if relative.is_absolute() or ".." in relative.parts or "\\" in raw:
        errors.append(f"{label}: path must be repository-relative without '..': {raw}")
        return None
    candidate = repo
    for part in relative.parts:
        candidate = candidate / part
        if candidate.is_symlink():
            errors.append(f"{label}: symbolic links are not valid evidence: {raw}")
            return None
    if not candidate.is_file():
        errors.append(f"{label}: file does not exist: {raw}")
        return None
    return candidate


def _check_hashed_file(repo: Path, entry: Any, label: str, errors: List[str]) -> Optional[Path]:
    if not isinstance(entry, dict):
        errors.append(f"{label}: must be an object with path and sha256")
        return None
    path = _repo_file(repo, entry.get("path"), label, errors)
    digest = entry.get("sha256")
    if not isinstance(digest, str) or not _SHA256.fullmatch(digest):
        errors.append(f"{label}: sha256 is missing or malformed")
        return None
    if path is not None and sha256_file(path) != digest:
        errors.append(f"{label}: stale, {entry.get('path')} no longer matches its recorded sha256")
        return None
    return path


def _check_acceptance_entry(repo: Path, key: str, index: int, entry: Any, log_text: Optional[str],
                            errors: List[str]) -> None:
    label = f"acceptance_evidence[{key}][{index}]"
    if not isinstance(entry, dict):
        errors.append(f"{label}: must be an object")
        return
    kind = entry.get("kind")
    if kind == "log_event":
        event = entry.get("id")
        if is_placeholder(event) or not _EVENT_ID.fullmatch(str(event)):
            errors.append(f"{label}: log event id is missing or a placeholder")
        elif log_text is not None and str(event) not in log_text:
            errors.append(f"{label}: log event {event!r} does not occur in the recorded runtime log")
    elif kind == "tick_range":
        start, end = entry.get("start"), entry.get("end")
        if (
            not isinstance(start, int) or not isinstance(end, int)
            or isinstance(start, bool) or isinstance(end, bool)
            or start < 0 or end < start
        ):
            errors.append(f"{label}: tick range needs integer 0 <= start <= end")
    elif kind == "capture":
        _check_hashed_file(repo, entry, label, errors)
    else:
        errors.append(f"{label}: kind must be log_event, tick_range or capture")


def validate_runtime_evidence(
    evidence: Any,
    *,
    repo: Path,
    head: str,
    working_tree_clean: bool,
    source_fingerprint: str,
    action_table_sha256: str,
    acceptance_keys: List[str],
) -> List[str]:
    """Return every reason `evidence` is not acceptable release evidence.

    Checks, in order: shape, placeholders, exact commit and clean tree, source
    data (fingerprint and action table), both DLLs, the runtime log, the capture
    manifest, and that every acceptance key is exactly the contract's and is
    promoted only with cited evidence. An empty list means acceptable.
    """
    if not isinstance(evidence, dict):
        return ["runtime evidence must be a JSON object"]
    errors: List[str] = []
    if evidence.get("schema_version") != EVIDENCE_SCHEMA_VERSION:
        errors.append(f"unsupported runtime evidence schema (need {EVIDENCE_SCHEMA_VERSION})")
    for missing in sorted(EVIDENCE_TOP_LEVEL - {"notes"} - set(evidence)):
        errors.append(f"missing required field: {missing}")
    for unknown in sorted(set(evidence) - EVIDENCE_TOP_LEVEL):
        errors.append(f"unknown field: {unknown}")

    # Exact commit, clean tree.
    commit = evidence.get("commit")
    if not isinstance(commit, str) or not _COMMIT.fullmatch(commit):
        errors.append("commit is missing, malformed or a placeholder")
    elif commit != head:
        errors.append(f"stale: evidence is for commit {commit[:12]}, the checkout is at {head[:12]}")
    if evidence.get("working_tree_clean") is not True:
        errors.append("evidence was not captured from a clean working tree")
    elif not working_tree_clean:
        errors.append("stale: the working tree has changes since the evidence was captured")

    # Source data.
    if evidence.get("source_fingerprint") != source_fingerprint:
        errors.append("stale: evidence does not match the current owner-data source fingerprint")
    if evidence.get("action_table_sha256") != action_table_sha256:
        errors.append("stale: evidence does not match the current extracted action table")

    for field in RUNTIME_FACT_FIELDS:
        if field in evidence and is_placeholder(evidence[field]):
            errors.append(f"{field} is missing or still a placeholder")

    # Artifacts: exactly the two DLLs, each current.
    recorded: Dict[str, str] = {}
    artifacts = evidence.get("artifacts")
    if not isinstance(artifacts, list):
        if "artifacts" in evidence:
            errors.append("artifacts must be a list")
    else:
        paths = [a.get("path") if isinstance(a, dict) else None for a in artifacts]
        if sorted(map(str, paths)) != sorted(EVIDENCE_ARTIFACTS):
            errors.append(f"artifacts must be exactly {list(EVIDENCE_ARTIFACTS)}, got {paths}")
        for index, artifact in enumerate(artifacts):
            if _check_hashed_file(repo, artifact, f"artifacts[{index}]", errors) is not None:
                recorded[artifact["path"]] = artifact["sha256"]

    # Runtime log.
    log_text: Optional[str] = None
    if "runtime_log" in evidence:
        log_path = _check_hashed_file(repo, evidence["runtime_log"], "runtime_log", errors)
        if log_path is not None:
            recorded[evidence["runtime_log"]["path"]] = evidence["runtime_log"]["sha256"]
            log_text = log_path.read_text(encoding="utf-8", errors="replace")
            if not log_text.strip():
                errors.append("runtime_log is empty")

    # Capture manifest: made by `fingerprint --require-clean` for this commit.
    if "capture_manifest" in evidence:
        capture_path = _check_hashed_file(repo, evidence["capture_manifest"], "capture_manifest", errors)
        if capture_path is not None:
            try:
                capture = json.loads(capture_path.read_text(encoding="utf-8"))
            except json.JSONDecodeError as error:
                errors.append(f"capture_manifest: not valid JSON: {error}")
            else:
                if capture.get("commit") != commit:
                    errors.append("stale: capture_manifest was taken at a different commit")
                if capture.get("clean_commit_capture") is not True:
                    errors.append("capture_manifest was not taken from a clean working tree")
                captured = capture.get("files", {})
                for path, digest in sorted(recorded.items()):
                    if captured.get(path, {}).get("sha256") != digest:
                        errors.append(f"stale: capture_manifest does not cover {path} at its recorded hash")

    # Acceptance: exactly the contract's keys, each promoted only with evidence.
    acceptance = evidence.get("acceptance")
    cited = evidence.get("acceptance_evidence")
    if "acceptance" in evidence and not isinstance(acceptance, dict):
        errors.append("acceptance must be an object")
        acceptance = None
    if "acceptance_evidence" in evidence and not isinstance(cited, dict):
        errors.append("acceptance_evidence must be an object")
        cited = None
    for label, table in (("acceptance", acceptance), ("acceptance_evidence", cited)):
        if table is None:
            continue
        for key in sorted(set(acceptance_keys) - set(table)):
            errors.append(f"{label} is missing assertion: {key}")
        for key in sorted(set(table) - set(acceptance_keys)):
            errors.append(f"stale: {label} names an assertion the contract does not define: {key}")
    if acceptance is not None and cited is not None:
        for key in acceptance_keys:
            if acceptance.get(key) is not True:
                errors.append(f"release assertion has not passed: {key}")
                continue
            entries = cited.get(key)
            if not isinstance(entries, list) or not entries:
                errors.append(f"assertion {key} is promoted without any cited evidence")
                continue
            for index, entry in enumerate(entries):
                _check_acceptance_entry(repo, key, index, entry, log_text, errors)
    return errors


def detect_apple_hardware() -> Optional[str]:
    """Model and chip of this Mac, or None where it cannot be read."""
    if platform.system() != "Darwin":
        return None
    def sysctl(name: str) -> str:
        done = subprocess.run(["sysctl", "-n", name], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        return done.stdout.strip() if done.returncode == 0 else ""
    model, chip = sysctl("hw.model"), sysctl("machdep.cpu.brand_string")
    return f"{model} / {chip}" if model and chip else None


def parse_accept(spec: str) -> "tuple[str, Dict[str, Any]]":
    """`KEY=log_event:ID`, `KEY=ticks:START-END` or `KEY=capture:PATH`."""
    key, separator, locator = spec.partition("=")
    kind, colon, value = locator.partition(":")
    if not (key and separator and colon and value):
        raise EvidenceError(f"--accept expects KEY=log_event:ID | ticks:START-END | capture:PATH, got {spec!r}")
    if kind == "log_event":
        return key, {"kind": "log_event", "id": value}
    if kind == "ticks":
        start, dash, end = value.partition("-")
        if not (dash and start.isdigit() and end.isdigit()):
            raise EvidenceError(f"--accept ticks needs START-END integers, got {value!r}")
        return key, {"kind": "tick_range", "start": int(start), "end": int(end)}
    if kind == "capture":
        return key, {"kind": "capture", "path": value}
    raise EvidenceError(f"--accept kind must be log_event, ticks or capture, got {kind!r}")


def capture_runtime_evidence(
    repo: Path,
    *,
    source_fingerprint: str,
    action_table_sha256: str,
    facts: Dict[str, str],
    log: str,
    accept: List["tuple[str, Dict[str, Any]]"],
    acceptance_keys: List[str],
    capture_out: str = "verification/out/evidence.json",
) -> "tuple[Dict[str, Any], List[str]]":
    """Build the release evidence manifest from a clean exact commit.

    Writes the hash capture to `capture_out` (which must be ignored) and
    returns `(manifest, remaining_problems)`. Only machine-derived facts are
    recorded as passing: an assertion is promoted solely by an `accept`
    locator, never by default.
    """
    repo = repo.resolve()
    commit = _run_git(["rev-parse", "HEAD"], repo)
    if _run_git(["status", "--porcelain", "--untracked-files=all"], repo):
        raise EvidenceError("release evidence needs a clean working tree at an exact commit")
    capture_path = repo / capture_out
    if _is_tracked(capture_path, repo) or not _is_ignored(capture_path, repo):
        raise EvidenceError(f"{capture_out} must be an ignored, untracked path")

    cited: Dict[str, List[Dict[str, Any]]] = {key: [] for key in acceptance_keys}
    for key, entry in accept:
        if key not in cited:
            raise EvidenceError(f"--accept names an assertion the contract does not define: {key}")
        cited[key].append(entry)
    capture_files = [repo / name for name in (*EVIDENCE_ARTIFACTS, log)]
    capture_files += [
        repo / entry["path"] for entries in cited.values() for entry in entries if entry["kind"] == "capture"
    ]
    capture = fingerprint(sorted(set(capture_files)), repo)
    capture_path.parent.mkdir(parents=True, exist_ok=True)
    capture_path.write_text(json.dumps(capture, indent=2) + "\n", encoding="utf-8")

    for entries in cited.values():
        for entry in entries:
            if entry["kind"] == "capture":
                entry["sha256"] = capture["files"][PurePosixPath(entry["path"]).as_posix()]["sha256"]
    manifest: Dict[str, Any] = {
        "schema_version": EVIDENCE_SCHEMA_VERSION,
        "commit": commit,
        "working_tree_clean": True,
        "source_fingerprint": source_fingerprint,
        "action_table_sha256": action_table_sha256,
        "capture_manifest": {"path": capture_out, "sha256": sha256_file(capture_path)},
        **facts,
        "artifacts": [
            {"path": name, "sha256": capture["files"][name]["sha256"]} for name in EVIDENCE_ARTIFACTS
        ],
        "runtime_log": {"path": log, "sha256": capture["files"][log]["sha256"]},
        "acceptance": {key: bool(cited[key]) for key in acceptance_keys},
        "acceptance_evidence": cited,
        "notes": "Captured by tools/evidence.py capture. Assertions are promoted only by cited log events, tick ranges or captures.",
    }
    problems = validate_runtime_evidence(
        manifest,
        repo=repo,
        head=commit,
        working_tree_clean=True,
        source_fingerprint=source_fingerprint,
        action_table_sha256=action_table_sha256,
        acceptance_keys=acceptance_keys,
    )
    return manifest, problems


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


def _capture_main(args: Any) -> int:
    """CLI for `capture`; returns the process exit code (1 while anything is unproven)."""
    import verify  # lazy: verify imports this module

    try:
        repo = Path(_run_git(["rev-parse", "--show-toplevel"], Path(".")))
        contract = verify.load_json(verify.CONTRACT_PATH)
        _, source_fingerprint = verify.check_source_data(True)
        action_table_sha256 = verify.sha256(verify.EXTRACTED_PATH)
        hardware = args.apple_hardware or detect_apple_hardware()
        if hardware is None:
            raise EvidenceError("cannot read this machine's hardware; pass --apple-hardware")
        out = (repo / args.out).resolve() if not args.out.is_absolute() else args.out
        if _is_tracked(out, repo) or not _is_ignored(out, repo):
            raise EvidenceError(f"{args.out} must be an ignored, untracked path")
        manifest, problems = capture_runtime_evidence(
            repo,
            source_fingerprint=source_fingerprint,
            action_table_sha256=action_table_sha256,
            facts={
                "elden_ring_build": args.elden_ring_build,
                "skyrim_version": args.skyrim_version,
                "crossover_version": args.crossover_version,
                "apple_hardware": hardware,
            },
            log=args.log,
            accept=[parse_accept(spec) for spec in args.accept],
            acceptance_keys=list(contract["release_acceptance"]),
        )
    except (EvidenceError, verify.VerificationError, OSError, KeyError) as error:
        sys.stderr.write(f"Error: {error}\n")
        return 2
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out} for commit {manifest['commit'][:12]}")
    if problems:
        print(f"{len(problems)} problem(s) keep this from being release evidence:")
        for problem in problems:
            print(f"  - {problem}")
        return 1
    print("release evidence is complete; tools/verify.py --profile release is the authority")
    return 0


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
    fingerprint_parser.add_argument(
        "--require-clean",
        action="store_true",
        help="refuse to write evidence unless the repository is at a clean exact commit",
    )
    capture_parser = subparsers.add_parser(
        "capture", help="Write verification/runtime-evidence.json from a clean exact commit"
    )
    capture_parser.add_argument("--elden-ring-build", required=True)
    capture_parser.add_argument("--skyrim-version", required=True)
    capture_parser.add_argument("--crossover-version", required=True)
    capture_parser.add_argument("--apple-hardware", help="defaults to this Mac's model and chip")
    capture_parser.add_argument("--log", default="verification/out/runtime.log")
    capture_parser.add_argument(
        "--accept", action="append", default=[], metavar="KEY=KIND:VALUE",
        help="promote an assertion: KEY=log_event:ID | KEY=ticks:START-END | KEY=capture:PATH",
    )
    capture_parser.add_argument("--out", type=Path, default=Path("verification/runtime-evidence.json"))
    args = parser.parse_args()

    if args.command == "capture":
        sys.exit(_capture_main(args))
    if args.command != "fingerprint":
        sys.stderr.write("Error: expected 'fingerprint' or 'capture' subcommand\n")
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
    try:
        result = fingerprint(args.files, repo)
    except EvidenceError as error:
        sys.stderr.write(f"Error: {error}\n")
        sys.exit(2)
    if args.require_clean and not result["clean_commit_capture"]:
        sys.stderr.write("Error: release evidence requires a clean Git working tree\n")
        sys.exit(2)

    # Write output
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
