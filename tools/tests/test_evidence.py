#!/usr/bin/env python3
"""Tests for the evidence fingerprint tool."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

# Add tools to path so we can import evidence
TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import evidence  # noqa: E402


class EvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = Path(tempfile.mkdtemp())
        self.repo = self.temp_dir / "repo"
        self.repo.mkdir()
        # Initialize a git repo
        subprocess.run(["git", "init"], cwd=self.repo, check=True, stdout=subprocess.DEVNULL)
        subprocess.run(["git", "config", "user.name", "Test"], cwd=self.repo, check=True)
        subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=self.repo, check=True)
        # Create an initial commit so that HEAD exists
        subprocess.run(["git", "commit", "--allow-empty", "-m", "Initial commit"], cwd=self.repo, check=True, stdout=subprocess.DEVNULL)
        # Verify that HEAD exists
        subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo, check=True, stdout=subprocess.DEVNULL)

    def make_file(self, *parts: str, content: bytes = b"") -> Path:
        p = self.repo.joinpath(*parts)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(content)
        return p

    def test_fingerprint_returns_correct_sha256_and_bytes(self) -> None:
        file1 = self.make_file("file1.txt", content=b"hello")
        file2 = self.make_file("file2.txt", content=b"world")
        result = evidence.fingerprint([file1, file2], self.repo)
        self.assertEqual(result["files"]["file1.txt"]["sha256"],
                         "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        self.assertEqual(result["files"]["file1.txt"]["bytes"], 5)
        self.assertEqual(result["files"]["file2.txt"]["sha256"],
                         "486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7")
        self.assertEqual(result["files"]["file2.txt"]["bytes"], 5)
        self.assertNotIn(str(self.temp_dir), json.dumps(result))

    def test_fingerprint_commit_matches_git_rev_parse(self) -> None:
        # Create a file and commit it
        f = self.make_file("commit.txt", content=b"data")
        subprocess.run(["git", "add", "commit.txt"], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=self.repo, check=True,
                       stdout=subprocess.DEVNULL)
        result = evidence.fingerprint([f], self.repo)
        # Get the commit from git
        commit_out = subprocess.run(["git", "rev-parse", "HEAD"],
                                    cwd=self.repo, capture_output=True, text=True, check=True)
        expected_commit = commit_out.stdout.strip()
        self.assertEqual(result["commit"], expected_commit)

    def test_fingerprint_dirty_flag(self) -> None:
        # Start with clean state
        f = self.make_file("clean.txt", content=b"clean")
        subprocess.run(["git", "add", "clean.txt"], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-m", "clean"], cwd=self.repo, check=True,
                       stdout=subprocess.DEVNULL)
        result_clean = evidence.fingerprint([f], self.repo)
        self.assertFalse(result_clean["dirty"])
        self.assertTrue(result_clean["clean_commit_capture"])
        # Modify the file
        f.write_bytes(b"modified")
        result_dirty = evidence.fingerprint([f], self.repo)
        self.assertTrue(result_dirty["dirty"])
        self.assertFalse(result_dirty["clean_commit_capture"])

    def test_fingerprint_rejects_outside_symlink_and_duplicate_inputs(self) -> None:
        inside = self.make_file("inside.txt", content=b"inside")
        outside = self.temp_dir / "outside.txt"
        outside.write_bytes(b"outside")
        with self.assertRaisesRegex(evidence.EvidenceError, "outside the repository"):
            evidence.fingerprint([outside], self.repo)

        link = self.repo / "link.txt"
        link.symlink_to(outside)
        with self.assertRaisesRegex(evidence.EvidenceError, "Symbolic links"):
            evidence.fingerprint([link], self.repo)

        with self.assertRaisesRegex(evidence.EvidenceError, "Duplicate"):
            evidence.fingerprint([inside, Path("inside.txt")], self.repo)

    def test_cli_writes_to_default_location(self) -> None:
        f = self.make_file("cli.txt", content=b"cli data")
        # Ensure the default output path is ignored by adding to .gitignore
        (self.repo / ".gitignore").write_text("verification/out/\n")
        # Run the CLI tool
        subprocess.run([sys.executable, str(TOOLS / "evidence.py"), "fingerprint", str(f)],
                       cwd=self.repo, check=True, stdout=subprocess.DEVNULL)
        out_path = self.repo / "verification" / "out" / "evidence.json"
        self.assertTrue(out_path.is_file())
        data = json.loads(out_path.read_text())
        self.assertEqual(data["files"]["cli.txt"]["sha256"],
                         "0c3f76f95f9e19afdf6bd4d9b66515c10a9d6fe2db6dd3a821cfcf67f229320f")
        self.assertEqual(data["files"]["cli.txt"]["bytes"], 8)
        self.assertNotIn(str(self.temp_dir), json.dumps(data))

    def test_cli_refuses_tracked_output_path(self) -> None:
        f = self.make_file("input.txt", content=b"input")
        # Create an output file and track it
        out_file = self.repo / "tracked_output.json"
        out_file.write_text("{}\n")
        subprocess.run(["git", "add", "tracked_output.json"], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-m", "add output"], cwd=self.repo, check=True,
                       stdout=subprocess.DEVNULL)
        # Run CLI with that output path
        completed = subprocess.run(
            [sys.executable, str(TOOLS / "evidence.py"), "fingerprint", str(f), "--out", str(out_file)],
            cwd=self.repo,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("tracked by git", completed.stderr)

    def test_cli_refuses_non_ignored_output_path(self) -> None:
        f = self.make_file("input2.txt", content=b"input2")
        # Create an output file that is not ignored and not tracked
        out_file = self.repo / "output.json"
        # Ensure it's not ignored by not adding to .gitignore
        # We don't add it to git, so it's untracked and not ignored -> should be refused
        completed = subprocess.run(
            [sys.executable, str(TOOLS / "evidence.py"), "fingerprint", str(f), "--out", str(out_file)],
            cwd=self.repo,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("not ignored by git", completed.stderr)

    def test_cli_accepts_ignored_output_path(self) -> None:
        f = self.make_file("input3.txt", content=b"input3")
        # Create a .gitignore that ignores the output path
        (self.repo / ".gitignore").write_text("ignored_output.json\n")
        out_file = self.repo / "ignored_output.json"
        # Run CLI
        completed = subprocess.run(
            [sys.executable, str(TOOLS / "evidence.py"), "fingerprint", str(f), "--out", str(out_file)],
            cwd=self.repo,
            capture_output=True,
            text=True,
        )
        if completed.returncode != 0:
            print(f"stderr: {completed.stderr}")
            print(f"stdout: {completed.stdout}")
        self.assertEqual(completed.returncode, 0)
        self.assertTrue(out_file.is_file())

    def test_cli_require_clean_rejects_dirty_tree_without_writing(self) -> None:
        f = self.make_file("dirty.txt", content=b"dirty")
        (self.repo / ".gitignore").write_text("evidence.json\n")
        out_file = self.repo / "evidence.json"
        completed = subprocess.run(
            [
                sys.executable,
                str(TOOLS / "evidence.py"),
                "fingerprint",
                str(f),
                "--out",
                str(out_file),
                "--require-clean",
            ],
            cwd=self.repo,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("clean Git working tree", completed.stderr)
        self.assertFalse(out_file.exists())

    def test_cli_require_clean_accepts_exact_commit(self) -> None:
        f = self.make_file("clean.txt", content=b"clean")
        (self.repo / ".gitignore").write_text("evidence.json\n")
        subprocess.run(["git", "add", "clean.txt", ".gitignore"], cwd=self.repo, check=True)
        subprocess.run(
            ["git", "commit", "-m", "clean evidence inputs"],
            cwd=self.repo,
            check=True,
            stdout=subprocess.DEVNULL,
        )
        out_file = self.repo / "evidence.json"
        completed = subprocess.run(
            [
                sys.executable,
                str(TOOLS / "evidence.py"),
                "fingerprint",
                "clean.txt",
                "--out",
                str(out_file),
                "--require-clean",
            ],
            cwd=self.repo,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        result = json.loads(out_file.read_text())
        self.assertTrue(result["clean_commit_capture"])


if __name__ == "__main__":
    unittest.main()
