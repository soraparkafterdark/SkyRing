import copy
import json
from pathlib import Path
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
        self.assertGreaterEqual(sum(map(swarm_sandbox.task_is_locally_actionable, tasks)), 8)
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

    def test_scope_matching_is_path_normalized(self):
        patterns = ["core/**", "architecture.md"]
        self.assertTrue(swarm_sandbox.path_allowed("core/src/ffi.rs", patterns))
        self.assertTrue(swarm_sandbox.path_allowed("./architecture.md", patterns))
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

    def test_unsafe_workspace_targets_are_rejected(self):
        with self.assertRaisesRegex(swarm_sandbox.SandboxError, "unsafe workspace"):
            swarm_sandbox.safe_workspace(Path.home())


if __name__ == "__main__":
    unittest.main()
