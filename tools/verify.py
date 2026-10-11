#!/usr/bin/env python3
"""Run SkyRing's host, source-data, and release verification gates.

The report is intentionally explicit about what was and was not proven. A
synthetic action table can validate plumbing, but it cannot satisfy the source
or release profiles.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
CONTRACT_PATH = ROOT / "verification" / "contract.json"
EXTRACTED_PATH = ROOT / "core" / "src" / "sim" / "extracted.rs"
EVIDENCE_PATH = ROOT / "verification" / "runtime-evidence.json"
REPORT_PATH = ROOT / "verification" / "out" / "last-run.json"
EXPECTED_NAME = "soraparkafterdark"
EXPECTED_EMAIL = "sorapark@proton.me"
EXPECTED_REMOTE_SUFFIX = "github.com/soraparkafterdark/SkyRing"


class VerificationError(RuntimeError):
    pass


def run(command: list[str], cwd: Path = ROOT, env: dict[str, str] | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if completed.returncode != 0:
        rendered = " ".join(command)
        raise VerificationError(f"command failed ({completed.returncode}): {rendered}\n{completed.stdout}")
    return completed.stdout


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def find_cargo() -> Path:
    found = shutil.which("cargo")
    if found:
        return Path(found)
    candidates = sorted((Path.home() / ".rustup" / "toolchains").glob("stable-*/bin/cargo"))
    if not candidates:
        raise VerificationError("cargo was not found in PATH or a stable rustup toolchain")
    return candidates[-1]


def cargo_environment(cargo: Path) -> dict[str, str]:
    env = os.environ.copy()
    if not shutil.which("rustc", path=env.get("PATH")):
        rustc = cargo.with_name("rustc")
        if rustc.exists():
            env["RUSTC"] = str(rustc)
    return env


def find_cxx() -> str:
    for candidate in ("clang++", "g++", "c++"):
        found = shutil.which(candidate)
        if found:
            return found
    raise VerificationError("a C++ compiler (clang++, g++, or c++) is required")


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise VerificationError(f"cannot read {path.relative_to(ROOT)}: {error}") from error
    if not isinstance(value, dict):
        raise VerificationError(f"{path.relative_to(ROOT)} must contain a JSON object")
    return value


def check_contract(contract: dict[str, Any]) -> str:
    if contract.get("schema_version") != 1:
        raise VerificationError("unsupported verification contract schema")
    if not contract.get("end_goal"):
        raise VerificationError("verification contract has no end goal")
    profiles = contract.get("profiles")
    if not isinstance(profiles, dict) or set(profiles) != {"local", "source", "release"}:
        raise VerificationError("verification contract must define local, source, and release profiles")
    acceptance = contract.get("release_acceptance")
    if not isinstance(acceptance, list) or len(acceptance) < 10:
        raise VerificationError("release acceptance is incomplete")
    invariants = contract.get("balance_invariants", {})
    if invariants.get("maximum_armament_multiplier") != 1.15:
        raise VerificationError("armament hard cap drifted from 1.15")
    if invariants.get("skyrim_owns_absolute_damage") is not True:
        raise VerificationError("Skyrim damage ownership invariant is missing")
    for name, profile in profiles.items():
        required = profile.get("required_gates") if isinstance(profile, dict) else None
        if not isinstance(required, list) or not required or not all(isinstance(gate, str) for gate in required):
            raise VerificationError(f"verification profile {name!r} has no valid required gates")
    return f"schema 1; {len(acceptance)} release acceptance assertions"


def required_gates(contract: dict[str, Any], profile_name: str) -> list[str]:
    profiles = contract["profiles"]
    required: list[str] = []
    seen: set[str] = set()
    current: str | None = profile_name
    while current is not None:
        if current in seen:
            raise VerificationError("verification profile inheritance contains a cycle")
        seen.add(current)
        profile = profiles.get(current)
        if not isinstance(profile, dict):
            raise VerificationError(f"unknown verification profile: {current}")
        required[:0] = profile["required_gates"]
        inherited = profile.get("inherits")
        current = inherited if isinstance(inherited, str) else None
    return list(dict.fromkeys(required))


def check_repository_identity() -> str:
    name = run(["git", "config", "--local", "--get", "user.name"]).strip()
    email = run(["git", "config", "--local", "--get", "user.email"]).strip()
    remote = run(["git", "remote", "get-url", "--push", "origin"]).strip()
    if (name, email) != (EXPECTED_NAME, EXPECTED_EMAIL):
        raise VerificationError(
            f"local Git identity is {name} <{email}>; expected {EXPECTED_NAME} <{EXPECTED_EMAIL}>"
        )
    normalized_remote = remote.rstrip("/").removesuffix(".git")
    if not normalized_remote.endswith(EXPECTED_REMOTE_SUFFIX):
        raise VerificationError(f"origin push URL is not the isolated Sora fork: {remote}")
    if re.match(r"https://[^/@:]+:[^/@]+@", remote):
        raise VerificationError("origin URL appears to contain an inline password or token")
    return f"{name} <{email}>; isolated fork remote"


def check_proprietary_data_guard() -> str:
    tracked = run(["git", "ls-files", "--", str(EXTRACTED_PATH.relative_to(ROOT))]).strip()
    if tracked:
        raise VerificationError("owner-generated extracted.rs is tracked by Git")
    ignored = subprocess.run(
        ["git", "check-ignore", "-q", str(EXTRACTED_PATH.relative_to(ROOT))],
        cwd=ROOT,
        check=False,
    )
    if ignored.returncode != 0:
        raise VerificationError("owner-generated extracted.rs is not ignored")
    return "generated action table is ignored and untracked"


def check_source_data(required: bool) -> tuple[str, str | None]:
    if not EXTRACTED_PATH.exists():
        if required:
            raise VerificationError("owner-generated core/src/sim/extracted.rs is required for this profile")
        return "synthetic stand-in active; source data not claimed", None
    fingerprint = sha256(EXTRACTED_PATH)
    return f"owner-generated action table present; sha256={fingerprint}", fingerprint


def check_rust_tests(cargo: Path, env: dict[str, str]) -> str:
    output = run([str(cargo), "test", "--all-targets"], cwd=ROOT / "core", env=env)
    match = re.search(r"test result: ok\. (\d+) passed", output)
    count = match.group(1) if match else "unknown"
    return f"cargo test --all-targets passed ({count} library tests reported)"


def check_c_abi(cxx: str, temporary: Path) -> str:
    source = temporary / "abi.cpp"
    source.write_text(
        "#include \"er_combat.h\"\n"
        "#include <cstddef>\n"
        "static_assert(sizeof(ErInput) == 32);\n"
        "static_assert(sizeof(ErWorld) == 36);\n"
        "static_assert(sizeof(ErOutput) == 224);\n"
        "static_assert(offsetof(ErOutput, sweep_a) == 88);\n"
        "int main() {}\n",
        encoding="utf-8",
    )
    run(
        [
            cxx,
            "-std=c++2b",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-I",
            str(ROOT / "core" / "include"),
            str(source),
            "-o",
            str(temporary / "abi"),
        ]
    )
    return "C header sizes and offsets match the Rust ABI contract"


def check_bridge_geometry(cxx: str, temporary: Path) -> str:
    executable = temporary / "combat-math-tests"
    run(
        [
            cxx,
            "-std=c++2b",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-I",
            str(ROOT / "core" / "include"),
            "-I",
            str(ROOT / "plugin" / "src"),
            str(ROOT / "plugin" / "src" / "BridgePlan.cpp"),
            str(ROOT / "plugin" / "src" / "CombatMath.cpp"),
            str(ROOT / "plugin" / "tests" / "CombatMathTests.cpp"),
            "-o",
            str(executable),
        ]
    )
    run([str(executable)])
    return "native bridge target/movement/geometry tests passed with warnings as errors"


def check_balance_model() -> str:
    env = os.environ.copy()
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    output = run([sys.executable, str(ROOT / "experiments" / "balance_model.py")], env=env)
    if "All five experiments ran." not in output:
        raise VerificationError("balance model did not complete all five experiments")
    return "Python balance envelope model passed"


def check_owner_data_tool() -> str:
    env = os.environ.copy()
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    output = run(
        [sys.executable, "-m", "unittest", "discover", "-s", "tools/tests", "-v"],
        env=env,
    )
    match = re.search(r"Ran (\d+) tests", output)
    count = match.group(1) if match else "unknown"
    return f"owner-data bundle safety and round-trip tests passed ({count} tests)"


def check_balance_report(cargo: Path, env: dict[str, str]) -> str:
    output = run(
        [str(cargo), "run", "--quiet", "--bin", "balance_report"],
        cwd=ROOT / "core",
        env=env,
    )
    if not output.strip():
        raise VerificationError("balance report produced no output")
    mode = "owner-extracted" if EXTRACTED_PATH.exists() else "synthetic"
    return f"action economy report generated in {mode} mode"


def resolve_evidence_path(raw: str) -> Path:
    path = Path(raw)
    return path if path.is_absolute() else ROOT / path


def check_runtime_evidence(contract: dict[str, Any], source_hash: str | None) -> str:
    if source_hash is None:
        raise VerificationError("release evidence cannot be checked without owner-extracted source data")
    evidence = load_json(EVIDENCE_PATH)
    if evidence.get("schema_version") != 1:
        raise VerificationError("unsupported runtime evidence schema")
    head = run(["git", "rev-parse", "HEAD"]).strip()
    if evidence.get("commit") != head:
        raise VerificationError("runtime evidence does not match the current Git commit")
    if evidence.get("source_data_sha256") != source_hash:
        raise VerificationError("runtime evidence does not match the current extracted action table")

    for field in ("elden_ring_build", "skyrim_version", "crossover_version", "apple_hardware"):
        value = evidence.get(field)
        if not isinstance(value, str) or not value.strip() or "record-" in value:
            raise VerificationError(f"runtime evidence field {field!r} is missing or still a placeholder")

    artifacts = evidence.get("artifacts")
    if not isinstance(artifacts, list) or len(artifacts) < 2:
        raise VerificationError("runtime evidence must fingerprint both DLL artifacts")
    for artifact in artifacts:
        if not isinstance(artifact, dict):
            raise VerificationError("invalid artifact evidence entry")
        path = resolve_evidence_path(str(artifact.get("path", "")))
        if not path.is_file() or artifact.get("sha256") != sha256(path):
            raise VerificationError(f"missing or hash-mismatched runtime artifact: {path}")

    runtime_log = evidence.get("runtime_log")
    if not isinstance(runtime_log, dict):
        raise VerificationError("runtime log evidence is missing")
    log_path = resolve_evidence_path(str(runtime_log.get("path", "")))
    if not log_path.is_file() or runtime_log.get("sha256") != sha256(log_path):
        raise VerificationError("runtime log is missing or its hash does not match")

    acceptance = evidence.get("acceptance")
    for assertion in contract["release_acceptance"]:
        if not isinstance(acceptance, dict) or acceptance.get(assertion) is not True:
            raise VerificationError(f"release assertion has not passed: {assertion}")
    return f"runtime artifacts, log, and {len(contract['release_acceptance'])} acceptance assertions verified"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("local", "source", "release"), default="local")
    parser.add_argument("--report", type=Path, default=REPORT_PATH)
    args = parser.parse_args()

    checks: list[dict[str, str]] = []
    contract: dict[str, Any] = {}
    source_hash: str | None = None

    def gate(name: str, action: Any) -> None:
        try:
            detail = action()
        except (VerificationError, OSError) as error:
            checks.append({"gate": name, "status": "FAIL", "detail": str(error)})
            print(f"FAIL  {name}: {error}")
        else:
            checks.append({"gate": name, "status": "PASS", "detail": str(detail)})
            print(f"PASS  {name}: {detail}")

    def unavailable(detail: str) -> str:
        raise VerificationError(detail)

    contract = load_json(CONTRACT_PATH)
    gate("contract", lambda: check_contract(contract))
    gate("repository_identity", check_repository_identity)
    gate("proprietary_data_guard", check_proprietary_data_guard)

    source_required = args.profile in {"source", "release"}

    def source_gate() -> str:
        nonlocal source_hash
        detail, source_hash = check_source_data(source_required)
        return detail

    gate("source_data", source_gate)

    try:
        cargo = find_cargo()
        cargo_env = cargo_environment(cargo)
    except VerificationError as error:
        cargo = Path("cargo")
        cargo_env = os.environ.copy()
        cargo_error = str(error)
    else:
        cargo_error = ""

    gate(
        "rust_tests",
        lambda: unavailable(cargo_error) if cargo_error else check_rust_tests(cargo, cargo_env),
    )

    try:
        cxx = find_cxx()
        cxx_error = ""
    except VerificationError as error:
        cxx = "c++"
        cxx_error = str(error)

    with tempfile.TemporaryDirectory(prefix="skyring-verify-") as directory:
        temporary = Path(directory)
        gate(
            "c_abi",
            lambda: unavailable(cxx_error) if cxx_error else check_c_abi(cxx, temporary),
        )
        gate(
            "bridge_geometry",
            lambda: unavailable(cxx_error) if cxx_error else check_bridge_geometry(cxx, temporary),
        )

    gate("balance_model", check_balance_model)
    gate("owner_data_tool", check_owner_data_tool)
    gate(
        "balance_report",
        lambda: unavailable(cargo_error) if cargo_error else check_balance_report(cargo, cargo_env),
    )

    if args.profile == "release":
        gate("runtime_evidence", lambda: check_runtime_evidence(contract, source_hash))

    def profile_gate() -> str:
        statuses = {check["gate"]: check["status"] for check in checks}
        required = required_gates(contract, args.profile)
        unsatisfied = [name for name in required if statuses.get(name) != "PASS"]
        if unsatisfied:
            raise VerificationError(f"required gates not satisfied: {', '.join(unsatisfied)}")
        return f"all {len(required)} gates required by the {args.profile} profile passed"

    gate("profile_requirements", profile_gate)

    failed = [check for check in checks if check["status"] != "PASS"]
    report = {
        "schema_version": 1,
        "profile": args.profile,
        "status": "FAIL" if failed else "PASS",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "commit": run(["git", "rev-parse", "HEAD"]).strip(),
        "source_data": {
            "mode": "owner-extracted" if source_hash else "synthetic",
            "sha256": source_hash,
        },
        "checks": checks,
        "claim": contract.get("profiles", {}).get(args.profile, {}).get("proves", ""),
    }
    report_path = args.report if args.report.is_absolute() else ROOT / args.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"\n{report['status']}  {args.profile} profile; report: {report_path}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
