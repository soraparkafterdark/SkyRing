from __future__ import annotations

import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import build_pins  # noqa: E402

COPIED = (
    "core/rust-toolchain.toml",
    "verification/build-pins.json",
    ".github/workflows/verify.yml",
    "plugin/vcpkg-configuration.json",
    "plugin/CMakePresets.json",
    "core/Cargo.lock",
)


def copy_tree(root: Path) -> None:
    for relative in COPIED:
        destination = root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(build_pins.ROOT / relative, destination)


def edit(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    assert old in text, f"{old!r} not found in {relative}"
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


class BuildPinTests(unittest.TestCase):
    def assert_rejected(self, mutate, expected: str) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            copy_tree(root)
            mutate(root)
            with self.assertRaisesRegex(build_pins.PinError, expected):
                build_pins.check(root)

    def test_repository_pins_are_consistent(self) -> None:
        detail = build_pins.check(build_pins.ROOT)
        self.assertIn("NOT yet verified", detail, "Windows must stay BLOCKED until a runner has built the DLLs")

    def test_toolchain_file_must_match_pin(self) -> None:
        self.assert_rejected(lambda r: edit(r, "core/rust-toolchain.toml", '"1.99.0"', '"stable"'), "channel 'stable'")

    def test_pin_must_be_exact_version(self) -> None:
        def mutate(root: Path) -> None:
            edit(root, "verification/build-pins.json", '"rust": "1.99.0"', '"rust": "stable"')
            edit(root, "core/rust-toolchain.toml", '"1.99.0"', '"stable"')

        self.assert_rejected(mutate, "exact x.y.z")

    def test_workflow_rust_version_must_match_pin(self) -> None:
        self.assert_rejected(
            lambda r: edit(r, ".github/workflows/verify.yml", 'RUST_VERSION: "1.99.0"', 'RUST_VERSION: "1.98.0"'),
            "RUST_VERSION '1.98.0'",
        )

    def test_unpinned_action_is_rejected(self) -> None:
        self.assert_rejected(
            lambda r: edit(r, ".github/workflows/verify.yml", "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1", "actions/checkout@v7"),
            "not pinned to a full commit SHA",
        )

    def test_floating_runner_is_rejected(self) -> None:
        self.assert_rejected(
            lambda r: edit(r, ".github/workflows/verify.yml", "runs-on: windows-2022", "runs-on: windows-latest"),
            "windows-latest",
        )

    def test_cargo_without_locked_is_rejected(self) -> None:
        self.assert_rejected(
            lambda r: edit(r, ".github/workflows/verify.yml", "cargo build --release --locked", "cargo build --release"),
            "without --locked",
        )

    def test_floating_toolchain_action_is_rejected(self) -> None:
        self.assert_rejected(
            lambda r: edit(r, ".github/workflows/verify.yml", "      - name: Install the pinned Rust toolchain\n        run: |", "      - uses: dtolnay/rust-toolchain@stable\n      - name: Install the pinned Rust toolchain\n        run: |"),
            "floating Rust toolchain",
        )

    def test_cmake_pin_must_be_installed(self) -> None:
        self.assert_rejected(
            lambda r: edit(r, ".github/workflows/verify.yml", '"cmake==3.31.10"', '"cmake"'),
            "cmake==3.31.10 is not installed",
        )

    def test_vcpkg_baseline_must_be_a_full_sha(self) -> None:
        def mutate(root: Path) -> None:
            path = root / "plugin/vcpkg-configuration.json"
            configuration = json.loads(path.read_text(encoding="utf-8"))
            configuration["default-registry"]["baseline"] = "master"
            path.write_text(json.dumps(configuration), encoding="utf-8")

        self.assert_rejected(mutate, "baseline is not a full commit SHA")

    def test_missing_lockfile_is_rejected(self) -> None:
        self.assert_rejected(lambda r: (r / "core/Cargo.lock").unlink(), "Cargo.lock is missing")

    def test_local_compiler_must_match_pin(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory) / "rustc"
            fake.write_text("#!/bin/sh\necho 'rustc 1.98.0 (abc 2026-01-01)'\n", encoding="utf-8")
            fake.chmod(0o755)
            with self.assertRaisesRegex(build_pins.PinError, "1.98.0 is not the pinned 1.99.0"):
                build_pins.check_local_compiler(fake, build_pins.load_pins(build_pins.ROOT))
            fake.write_text("#!/bin/sh\necho 'rustc 1.99.0 (abc 2026-01-01)'\n", encoding="utf-8")
            self.assertIn("matches the pin", build_pins.check_local_compiler(fake, build_pins.load_pins(build_pins.ROOT)))


if __name__ == "__main__":
    unittest.main()
