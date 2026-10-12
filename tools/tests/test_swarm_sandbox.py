import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from tools import swarm_sandbox


class SwarmSandboxTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads(swarm_sandbox.DEFAULT_MANIFEST.read_text(encoding="utf-8"))

    def test_repository_manifest_is_valid(self):
        swarm_sandbox.validate_manifest(self.manifest)

    def test_manifest_has_actionable_and_honestly_blocked_work(self):
        tasks = self.manifest["tasks"]
        self.assertGreaterEqual(sum(map(swarm_sandbox.task_is_locally_actionable, tasks)), 6)
        self.assertTrue(any(task["requires"]["owner_dump"] for task in tasks))
        self.assertTrue(any(task["acceptance_ceiling"] == "release" for task in tasks))

    def test_dependency_cycle_is_rejected(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["tasks"][0]["depends_on"] = [manifest["tasks"][1]["id"]]
        manifest["tasks"][1]["depends_on"] = [manifest["tasks"][0]["id"]]
        with self.assertRaisesRegex(swarm_sandbox.SandboxError, "dependency cycle"):
            swarm_sandbox.validate_manifest(manifest)

    def test_unknown_check_is_rejected(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["tasks"][0]["checks"] = ["not-a-check"]
        with self.assertRaisesRegex(swarm_sandbox.SandboxError, "unknown checks"):
            swarm_sandbox.validate_manifest(manifest)

    def test_unknown_phase_is_rejected(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["tasks"][0]["phase"] = "not-a-phase"
        with self.assertRaisesRegex(swarm_sandbox.SandboxError, "unknown phase"):
            swarm_sandbox.validate_manifest(manifest)

    def test_empty_subtasks_are_rejected(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["tasks"][0]["subtasks"] = []
        with self.assertRaisesRegex(swarm_sandbox.SandboxError, "must define subtasks"):
            swarm_sandbox.validate_manifest(manifest)

    def test_later_phase_dependency_is_rejected(self):
        manifest = copy.deepcopy(self.manifest)
        first = manifest["tasks"][0]
        first["depends_on"] = [manifest["tasks"][-1]["id"]]
        with self.assertRaisesRegex(swarm_sandbox.SandboxError, "later-phase"):
            swarm_sandbox.validate_manifest(manifest)

    def test_scope_matching_is_path_normalized(self):
        patterns = ["core/**", "docs/architecture.md"]
        self.assertTrue(swarm_sandbox.path_allowed("core/src/ffi.rs", patterns))
        self.assertTrue(swarm_sandbox.path_allowed("./docs/architecture.md", patterns))
        self.assertTrue(swarm_sandbox.path_allowed("core\\src\\ffi.rs", patterns))
        self.assertFalse(swarm_sandbox.path_allowed("plugin/src/Bridge.cpp", patterns))

    def test_rust_check_uses_rustup_cargo_when_not_on_path(self):
        with mock.patch.object(swarm_sandbox.shutil, "which", return_value=None), mock.patch.object(
            swarm_sandbox, "find_cargo", return_value="/toolchain/bin/cargo"
        ):
            self.assertEqual(
                swarm_sandbox.resolve_command(["cargo", "test", "--all-targets"]),
                ["/toolchain/bin/cargo", "test", "--all-targets"],
            )

    def test_prompt_carries_claim_ceiling_and_no_push_rule(self):
        task = self.manifest["tasks"][0]
        prompt = swarm_sandbox.render_prompt(task, "a" * 40)
        self.assertIn("Do not push", prompt)
        self.assertIn("Maximum defensible claim", prompt)
        self.assertIn(task["done_when"], prompt)
        self.assertIn(task["subtasks"][0]["work"], prompt)
        self.assertIn(task["produces"][0], prompt)

    def test_unsafe_workspace_targets_are_rejected(self):
        with self.assertRaisesRegex(swarm_sandbox.SandboxError, "unsafe workspace"):
            swarm_sandbox.safe_workspace(Path.home())


class DefaultBaseTests(unittest.TestCase):
    """`check` must be runnable by a worker that was never given --base."""

    def git(self, repo, *args):
        done = subprocess.run(
            ["git", "-c", "user.name=T", "-c", "user.email=t@example.com", *args],
            cwd=repo, check=True, text=True, stdout=subprocess.PIPE,
        )
        return done.stdout.strip()

    def make_repo(self, root, branch="main"):
        repo = Path(root) / "repo"
        repo.mkdir()
        self.git(repo, "init", "-q", "-b", branch)
        (repo / "a.txt").write_text("one\n")
        self.git(repo, "add", "-A")
        self.git(repo, "commit", "-q", "-m", "base")
        return repo

    def test_merge_base_with_trunk_wins_over_main(self):
        with tempfile.TemporaryDirectory() as root:
            repo = self.make_repo(root)
            self.git(repo, "checkout", "-q", "-b", "swarm/trunk")
            (repo / "t.txt").write_text("trunk\n")
            self.git(repo, "add", "-A")
            self.git(repo, "commit", "-q", "-m", "trunk work")
            trunk = self.git(repo, "rev-parse", "HEAD")
            self.git(repo, "checkout", "-q", "-b", "swarm/task")
            self.assertEqual(swarm_sandbox.default_base(repo), trunk)

    def test_falls_back_to_main_then_to_nothing(self):
        with tempfile.TemporaryDirectory() as root:
            repo = self.make_repo(root)
            main = self.git(repo, "rev-parse", "HEAD")
            self.git(repo, "checkout", "-q", "-b", "feature")
            self.assertEqual(swarm_sandbox.default_base(repo), main)
        with tempfile.TemporaryDirectory() as root:
            repo = self.make_repo(root, branch="elsewhere")
            self.assertIsNone(swarm_sandbox.default_base(repo))

    def check_args(self, workspace):
        return argparse.Namespace(workspace=str(workspace), task="t", base=None, report=None)

    manifest = {
        "checks": {"ok": ["true"]},
        "tasks": [{"id": "t", "allowed_paths": ["a.txt"], "checks": ["ok"], "acceptance_ceiling": "local"}],
    }

    def test_check_runs_without_base_and_enforces_scope(self):
        with tempfile.TemporaryDirectory() as root:
            repo = self.make_repo(root)
            self.git(repo, "checkout", "-q", "-b", "feature")
            (repo / "a.txt").write_text("two\n")
            self.git(repo, "commit", "-q", "-am", "in scope")
            with mock.patch.object(swarm_sandbox, "get_task", return_value=self.manifest["tasks"][0]):
                self.assertEqual(swarm_sandbox.command_check(self.check_args(repo), self.manifest), 0)
                (repo / "outside.txt").write_text("x\n")
                self.git(repo, "add", "-A")
                self.git(repo, "commit", "-q", "-m", "out of scope")
                self.assertEqual(swarm_sandbox.command_check(self.check_args(repo), self.manifest), 1)
            report = json.loads((repo / "verification" / "out" / "swarm-t.json").read_text())
            self.assertIn("outside.txt", report["checks"][0]["detail"])

    def test_check_without_any_base_names_the_options(self):
        with tempfile.TemporaryDirectory() as root:
            repo = self.make_repo(root, branch="elsewhere")
            with mock.patch.object(swarm_sandbox, "get_task", return_value=self.manifest["tasks"][0]):
                with self.assertRaisesRegex(swarm_sandbox.SandboxError, "pass --base.*swarm/trunk or main"):
                    swarm_sandbox.command_check(self.check_args(repo), self.manifest)

    def test_rustup_cargo_gets_its_own_toolchain_on_path(self):
        with mock.patch.object(swarm_sandbox.shutil, "which", return_value=None):
            environment = swarm_sandbox.command_environment(["/toolchain/bin/cargo", "test"])
        self.assertEqual(environment["PATH"].split(os.pathsep)[0], "/toolchain/bin")
        with mock.patch.object(swarm_sandbox.shutil, "which", return_value="/usr/bin/cargo"):
            self.assertIsNone(swarm_sandbox.command_environment(["/toolchain/bin/cargo", "test"]))
        self.assertIsNone(swarm_sandbox.command_environment(["python3", "x.py"]))

