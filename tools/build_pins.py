#!/usr/bin/env python3
"""Check that every build input is pinned and that CI agrees with the pins.

`verification/build-pins.json` is the source of truth. This tool reads the
toolchain file (core/rust-toolchain.toml), the workflow, and the vcpkg configuration, and fails on any
floating input. It cannot prove a Windows runner builds; that stays BLOCKED
until one has produced both DLLs (`notes.windows_runner_verified`).
"""

from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess
import sys
import tomllib
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FULL_SHA = re.compile(r"^[0-9a-f]{40}$")
USES = re.compile(r"^\s*-?\s*uses:\s*(\S+)", re.MULTILINE)
RUNS_ON = re.compile(r"^\s*runs-on:\s*(\S+)", re.MULTILINE)


class PinError(RuntimeError):
    pass


def load_pins(root: Path) -> dict[str, Any]:
    try:
        pins = json.loads((root / "verification" / "build-pins.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PinError(f"cannot read verification/build-pins.json: {error}") from error
    if pins.get("schema_version") != 1:
        raise PinError("unsupported build-pins schema")
    for key in ("rust", "runners", "windows"):
        if key not in pins:
            raise PinError(f"build-pins.json is missing {key!r}")
    return pins


def check_rust(root: Path, pins: dict[str, Any], problems: list[str]) -> None:
    path = root / "core" / "rust-toolchain.toml"
    if not path.is_file():
        problems.append("core/rust-toolchain.toml is missing")
        return
    channel = tomllib.loads(path.read_text(encoding="utf-8")).get("toolchain", {}).get("channel")
    if channel != pins["rust"]:
        problems.append(f"core/rust-toolchain.toml channel {channel!r} != pinned {pins['rust']!r}")
    if not re.fullmatch(r"\d+\.\d+\.\d+", str(pins["rust"])):
        problems.append(f"pinned Rust {pins['rust']!r} must be an exact x.y.z version, not a channel")
    if not (root / "core" / "Cargo.lock").is_file():
        problems.append("core/Cargo.lock is missing")


def check_workflow(root: Path, pins: dict[str, Any], problems: list[str]) -> None:
    workflows = sorted((root / ".github" / "workflows").glob("*.yml"))
    if not workflows:
        problems.append("no workflows found")
    for workflow in workflows:
        text = workflow.read_text(encoding="utf-8")
        name = workflow.name
        for action in USES.findall(text):
            if action.startswith("./"):
                continue
            ref = action.partition("@")[2]
            if not FULL_SHA.fullmatch(ref):
                problems.append(f"{name}: action {action} is not pinned to a full commit SHA")
        for image in RUNS_ON.findall(text):
            if "latest" in image or image not in pins["runners"].values():
                problems.append(f"{name}: runner {image!r} is not one of the pinned images {sorted(pins['runners'].values())}")
        for line in text.splitlines():
            stripped = line.strip()
            if re.search(r"\bcargo (build|test|run|check)\b", stripped) and "--locked" not in stripped:
                problems.append(f"{name}: cargo invocation without --locked: {stripped}")
        match = re.search(r'RUST_VERSION:\s*"?([^"\s]+)"?', text)
        if match is None or match.group(1) != pins["rust"]:
            problems.append(f"{name}: RUST_VERSION {match.group(1) if match else None!r} != pinned {pins['rust']!r}")
        if "dtolnay/rust-toolchain" in text or "@stable" in text:
            problems.append(f"{name}: floating Rust toolchain reference")
        if name == "verify.yml":
            for tool in ("cmake", "ninja"):
                if f'"{tool}=={pins["windows"][tool]}"' not in text:
                    problems.append(f"{name}: {tool}=={pins['windows'][tool]} is not installed from the pin")
            if f'python-version: "{pins["windows"]["python"]}"' not in text:
                problems.append(f"{name}: python-version does not match the pin")


def check_vcpkg(root: Path, problems: list[str]) -> None:
    path = root / "plugin" / "vcpkg-configuration.json"
    try:
        configuration = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        problems.append(f"cannot read plugin/vcpkg-configuration.json: {error}")
        return
    registries = [configuration.get("default-registry", {}), *configuration.get("registries", [])]
    if not registries[0]:
        problems.append("vcpkg has no default registry baseline")
    for registry in registries:
        if registry and not FULL_SHA.fullmatch(str(registry.get("baseline", ""))):
            problems.append(f"vcpkg registry {registry.get('repository')} baseline is not a full commit SHA")
    presets = (root / "plugin" / "CMakePresets.json").read_text(encoding="utf-8")
    if "VCPKG_TARGET_TRIPLET" not in presets or '"generator": "Ninja"' not in presets:
        problems.append("CMake preset must pin the vcpkg triplet and the Ninja generator")


def check(root: Path = ROOT) -> str:
    pins = load_pins(root)
    problems: list[str] = []
    check_rust(root, pins, problems)
    check_workflow(root, pins, problems)
    check_vcpkg(root, problems)
    if problems:
        raise PinError("floating or drifting build inputs:\n  " + "\n  ".join(problems))
    verified = pins.get("notes", {}).get("windows_runner_verified") is True
    return (
        f"Rust {pins['rust']}, CMake {pins['windows']['cmake']}, Ninja {pins['windows']['ninja']}, "
        "actions and vcpkg baselines pinned by SHA; "
        + ("Windows runner verified" if verified else "Windows runner NOT yet verified (BLOCKED)")
    )


def check_local_compiler(rustc: Path | str, pins: dict[str, Any]) -> str:
    output = subprocess.run([str(rustc), "--version"], check=False, text=True, capture_output=True).stdout
    match = re.match(r"rustc (\d+\.\d+\.\d+)", output)
    if match is None:
        raise PinError(f"cannot read the compiler version from {rustc}")
    if match.group(1) != pins["rust"]:
        raise PinError(f"local rustc {match.group(1)} is not the pinned {pins['rust']}")
    return f"local rustc {match.group(1)} matches the pin"


def main() -> int:
    try:
        print("PASS  build_pins: " + check())
    except PinError as error:
        print(f"FAIL  build_pins: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
