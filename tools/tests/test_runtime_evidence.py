"""Release runtime evidence: placeholders, missing fields, and every stale dimension."""

from __future__ import annotations

import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import evidence  # noqa: E402
import verify  # noqa: E402

KEYS = ["input", "stamina", "hit_stop"]
FINGERPRINT = "a" * 64
ACTION_TABLE = "b" * 64
LOG_TEXT = "tick=101 event=roll.start\ntick=140 event=hit.confirm id=contact-7\n"


def git(repo: Path, *args: str) -> str:
    done = subprocess.run(["git", *args], cwd=repo, check=True, text=True, stdout=subprocess.PIPE)
    return done.stdout.strip()


def make_repo(root: Path) -> Path:
    repo = root / "repo"
    repo.mkdir()
    git(repo, "init", "-q")
    git(repo, "config", "user.name", "Test")
    git(repo, "config", "user.email", "test@example.com")
    (repo / ".gitignore").write_text("plugin/dist/SKSE/Plugins/*.dll\nverification/out/\nverification/runtime-evidence.json\n")
    (repo / "tracked.txt").write_text("source\n")
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "-m", "init")
    for name in evidence.EVIDENCE_ARTIFACTS:
        path = repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"MZ " + name.encode())
    (repo / "verification" / "out").mkdir(parents=True)
    (repo / "verification" / "out" / "runtime.log").write_text(LOG_TEXT)
    return repo


ACCEPT = [
    ("input", {"kind": "log_event", "id": "roll.start"}),
    ("stamina", {"kind": "tick_range", "start": 100, "end": 160}),
    ("hit_stop", {"kind": "log_event", "id": "contact-7"}),
]
FACTS = {
    "elden_ring_build": "13272501",
    "skyrim_version": "1.6.1170",
    "crossover_version": "CrossOver 26.0",
    "apple_hardware": "Mac14,12 / Apple M2 Pro",
}


class EvidenceCase(unittest.TestCase):
    def setUp(self) -> None:
        self._temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self._temporary.cleanup)
        self.repo = make_repo(Path(self._temporary.name))
        self.head = git(self.repo, "rev-parse", "HEAD")
        self.manifest, problems = evidence.capture_runtime_evidence(
            self.repo,
            source_fingerprint=FINGERPRINT,
            action_table_sha256=ACTION_TABLE,
            facts=FACTS,
            log="verification/out/runtime.log",
            accept=copy.deepcopy(ACCEPT),
            acceptance_keys=KEYS,
        )
        self.assertEqual(problems, [], "the baseline manifest must be acceptable")

    def validate(self, manifest=None, **overrides) -> list[str]:
        arguments = dict(
            repo=self.repo,
            head=self.head,
            working_tree_clean=True,
            source_fingerprint=FINGERPRINT,
            action_table_sha256=ACTION_TABLE,
            acceptance_keys=KEYS,
        )
        arguments.update(overrides)
        return evidence.validate_runtime_evidence(
            self.manifest if manifest is None else manifest, **arguments
        )

    def mutated(self, edit) -> dict:
        manifest = copy.deepcopy(self.manifest)
        edit(manifest)
        return manifest

    def assertRejected(self, problems: list[str], fragment: str) -> None:
        self.assertTrue(any(fragment in problem for problem in problems), f"{fragment!r} not in {problems}")

    def rewrite_capture(self, edit) -> None:
        """Change the capture manifest on disk and re-record its hash so only that dimension is stale."""
        path = self.repo / self.manifest["capture_manifest"]["path"]
        capture = json.loads(path.read_text())
        edit(capture)
        path.write_text(json.dumps(capture, indent=2) + "\n")
        self.manifest["capture_manifest"]["sha256"] = evidence.sha256_file(path)


class AcceptableEvidenceTests(EvidenceCase):
    def test_complete_evidence_is_acceptable(self) -> None:
        self.assertEqual(self.validate(), [])


class PlaceholderAndMissingFieldTests(EvidenceCase):
    def test_every_required_field_is_required(self) -> None:
        for field in sorted(evidence.EVIDENCE_TOP_LEVEL - {"notes"}):
            with self.subTest(field=field):
                manifest = self.mutated(lambda m: m.pop(field))
                self.assertRejected(self.validate(manifest), f"missing required field: {field}")

    def test_runtime_facts_reject_placeholders_and_blanks(self) -> None:
        for field in evidence.RUNTIME_FACT_FIELDS:
            for value in ("record-source-game-build", "replace-with-version", "", "   ", "TBD", "<fill in>", None, 7):
                with self.subTest(field=field, value=value):
                    manifest = self.mutated(lambda m: m.__setitem__(field, value))
                    self.assertRejected(self.validate(manifest), f"{field} is missing or still a placeholder")

    def test_unknown_fields_and_schema_version(self) -> None:
        self.assertRejected(self.validate(self.mutated(lambda m: m.__setitem__("extra", 1))), "unknown field: extra")
        self.assertRejected(self.validate(self.mutated(lambda m: m.__setitem__("schema_version", 1))), "unsupported runtime evidence schema")
        self.assertEqual(self.validate([]), ["runtime evidence must be a JSON object"])

    def test_example_template_is_rejected(self) -> None:
        template = json.loads((TOOLS.parent / "verification" / "runtime-evidence.example.json").read_text())
        contract = json.loads((TOOLS.parent / "verification" / "contract.json").read_text())
        problems = self.validate(template, acceptance_keys=contract["release_acceptance"])
        self.assertRejected(problems, "is missing or still a placeholder")
        self.assertRejected(problems, "release assertion has not passed")


class StaleCommitTests(EvidenceCase):
    def test_other_commit_is_stale(self) -> None:
        self.assertRejected(self.validate(head="c" * 40), "stale: evidence is for commit")

    def test_malformed_commit(self) -> None:
        manifest = self.mutated(lambda m: m.__setitem__("commit", "replace-with-full-git-commit"))
        self.assertRejected(self.validate(manifest), "commit is missing, malformed or a placeholder")

    def test_evidence_not_captured_clean(self) -> None:
        manifest = self.mutated(lambda m: m.__setitem__("working_tree_clean", False))
        self.assertRejected(self.validate(manifest), "not captured from a clean working tree")

    def test_tree_modified_since_capture(self) -> None:
        self.assertRejected(self.validate(working_tree_clean=False), "working tree has changes since")


class StaleSourceDataTests(EvidenceCase):
    def test_other_source_fingerprint_is_stale(self) -> None:
        self.assertRejected(self.validate(source_fingerprint="d" * 64), "does not match the current owner-data source fingerprint")

    def test_other_action_table_is_stale(self) -> None:
        self.assertRejected(self.validate(action_table_sha256="e" * 64), "does not match the current extracted action table")


class StaleArtifactTests(EvidenceCase):
    def test_rebuilt_dll_is_stale(self) -> None:
        (self.repo / evidence.EVIDENCE_ARTIFACTS[0]).write_bytes(b"rebuilt")
        self.assertRejected(self.validate(), "artifacts[0]: stale")

    def test_missing_dll(self) -> None:
        (self.repo / evidence.EVIDENCE_ARTIFACTS[1]).unlink()
        self.assertRejected(self.validate(), "file does not exist")

    def test_only_the_two_dlls_are_accepted(self) -> None:
        self.assertRejected(self.validate(self.mutated(lambda m: m["artifacts"].pop())), "artifacts must be exactly")
        extra = self.mutated(lambda m: m["artifacts"].append(dict(m["artifacts"][0])))
        self.assertRejected(self.validate(extra), "artifacts must be exactly")
        renamed = self.mutated(lambda m: m["artifacts"][0].__setitem__("path", "plugin/dist/SKSE/Plugins/Other.dll"))
        self.assertRejected(self.validate(renamed), "artifacts must be exactly")

    def test_paths_may_not_escape_or_be_links(self) -> None:
        for path in ("../outside.dll", "/etc/passwd", "plugin\\dist\\x.dll"):
            with self.subTest(path=path):
                manifest = self.mutated(lambda m: m["artifacts"][0].__setitem__("path", path))
                self.assertRejected(self.validate(manifest), "path must be repository-relative")
        target = self.repo / evidence.EVIDENCE_ARTIFACTS[0]
        elsewhere = self.repo.parent / "elsewhere.dll"
        elsewhere.write_bytes(target.read_bytes())
        target.unlink()
        target.symlink_to(elsewhere)
        self.assertRejected(self.validate(), "symbolic links are not valid evidence")

    def test_malformed_hash(self) -> None:
        manifest = self.mutated(lambda m: m["artifacts"][0].__setitem__("sha256", "replace-with-sha256"))
        self.assertRejected(self.validate(manifest), "sha256 is missing or malformed")


class StaleLogTests(EvidenceCase):
    def test_changed_log_is_stale(self) -> None:
        (self.repo / "verification" / "out" / "runtime.log").write_text(LOG_TEXT + "tick=999\n")
        self.assertRejected(self.validate(), "runtime_log: stale")

    def test_empty_log(self) -> None:
        log = self.repo / "verification" / "out" / "runtime.log"
        log.write_text("")
        self.manifest["runtime_log"]["sha256"] = evidence.sha256_file(log)
        self.assertRejected(self.validate(), "runtime_log is empty")

    def test_cited_log_event_must_occur_in_the_log(self) -> None:
        manifest = self.mutated(
            lambda m: m["acceptance_evidence"]["input"].__setitem__(0, {"kind": "log_event", "id": "never.happened"})
        )
        self.assertRejected(self.validate(manifest), "does not occur in the recorded runtime log")


class StaleCaptureManifestTests(EvidenceCase):
    def test_edited_capture_manifest_is_stale(self) -> None:
        path = self.repo / self.manifest["capture_manifest"]["path"]
        path.write_text(path.read_text() + " ")
        self.assertRejected(self.validate(), "capture_manifest: stale")

    def test_capture_from_another_commit(self) -> None:
        self.rewrite_capture(lambda capture: capture.__setitem__("commit", "f" * 40))
        self.assertRejected(self.validate(), "capture_manifest was taken at a different commit")

    def test_capture_from_dirty_tree(self) -> None:
        self.rewrite_capture(lambda capture: capture.__setitem__("clean_commit_capture", False))
        self.assertRejected(self.validate(), "capture_manifest was not taken from a clean working tree")

    def test_capture_must_cover_every_recorded_hash(self) -> None:
        self.rewrite_capture(lambda capture: capture["files"].pop(evidence.EVIDENCE_ARTIFACTS[0]))
        self.assertRejected(self.validate(), f"does not cover {evidence.EVIDENCE_ARTIFACTS[0]}")
        self.rewrite_capture(lambda capture: capture["files"].update({"verification/out/runtime.log": {"sha256": "0" * 64, "bytes": 1}}))
        self.assertRejected(self.validate(), "does not cover verification/out/runtime.log")


class AcceptanceKeyTests(EvidenceCase):
    def test_unknown_assertion_is_stale(self) -> None:
        manifest = self.mutated(lambda m: m["acceptance"].__setitem__("renamed_assertion", True))
        self.assertRejected(self.validate(manifest), "stale: acceptance names an assertion the contract does not define")
        manifest = self.mutated(lambda m: m["acceptance_evidence"].__setitem__("renamed_assertion", []))
        self.assertRejected(self.validate(manifest), "stale: acceptance_evidence names an assertion")

    def test_missing_assertion(self) -> None:
        manifest = self.mutated(lambda m: m["acceptance"].pop("stamina"))
        self.assertRejected(self.validate(manifest), "acceptance is missing assertion: stamina")
        manifest = self.mutated(lambda m: m["acceptance_evidence"].pop("stamina"))
        self.assertRejected(self.validate(manifest), "acceptance_evidence is missing assertion: stamina")

    def test_contract_gaining_an_assertion_makes_old_evidence_stale(self) -> None:
        self.assertRejected(self.validate(acceptance_keys=[*KEYS, "new_assertion"]), "acceptance is missing assertion: new_assertion")

    def test_false_or_non_boolean_assertion_fails(self) -> None:
        for value in (False, "true", 1, None):
            with self.subTest(value=value):
                manifest = self.mutated(lambda m: m["acceptance"].__setitem__("input", value))
                self.assertRejected(self.validate(manifest), "release assertion has not passed: input")

    def test_bare_boolean_is_not_evidence(self) -> None:
        manifest = self.mutated(lambda m: m["acceptance_evidence"].__setitem__("input", []))
        self.assertRejected(self.validate(manifest), "promoted without any cited evidence")

    def test_malformed_locators(self) -> None:
        bad = [
            {"kind": "log_event", "id": "replace-with-event"},
            {"kind": "log_event"},
            {"kind": "tick_range", "start": 10, "end": 5},
            {"kind": "tick_range", "start": -1, "end": 5},
            {"kind": "tick_range", "start": True, "end": 5},
            {"kind": "tick_range", "start": "1", "end": 5},
            {"kind": "hunch", "id": "x"},
            "log line 4",
        ]
        for entry in bad:
            with self.subTest(entry=entry):
                manifest = self.mutated(lambda m: m["acceptance_evidence"]["stamina"].__setitem__(0, entry))
                self.assertTrue(self.validate(manifest), "a malformed locator must be rejected")


class CaptureTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self._temporary.cleanup)
        self.repo = make_repo(Path(self._temporary.name))

    def capture(self, **overrides):
        arguments = dict(
            source_fingerprint=FINGERPRINT,
            action_table_sha256=ACTION_TABLE,
            facts=FACTS,
            log="verification/out/runtime.log",
            accept=copy.deepcopy(ACCEPT),
            acceptance_keys=KEYS,
        )
        arguments.update(overrides)
        return evidence.capture_runtime_evidence(self.repo, **arguments)

    def test_nothing_is_promoted_by_default(self) -> None:
        manifest, problems = self.capture(accept=[])
        self.assertEqual(manifest["acceptance"], {key: False for key in KEYS})
        self.assertEqual(len([p for p in problems if "has not passed" in p]), len(KEYS))

    def test_only_cited_assertions_are_promoted(self) -> None:
        manifest, problems = self.capture(accept=ACCEPT[:1])
        self.assertEqual(manifest["acceptance"], {"input": True, "stamina": False, "hit_stop": False})
        self.assertEqual(len(problems), 2)

    def test_dirty_tree_is_refused(self) -> None:
        (self.repo / "tracked.txt").write_text("edited\n")
        with self.assertRaisesRegex(evidence.EvidenceError, "clean working tree"):
            self.capture()
        (self.repo / "tracked.txt").write_text("source\n")
        (self.repo / "untracked.txt").write_text("stray\n")
        with self.assertRaisesRegex(evidence.EvidenceError, "clean working tree"):
            self.capture()

    def test_capture_output_must_be_ignored(self) -> None:
        with self.assertRaisesRegex(evidence.EvidenceError, "ignored, untracked"):
            self.capture(capture_out="capture.json")

    def test_unknown_assertion_is_refused(self) -> None:
        with self.assertRaisesRegex(evidence.EvidenceError, "does not define: bogus"):
            self.capture(accept=[("bogus", {"kind": "log_event", "id": "roll.start"})])

    def test_missing_dll_is_an_error_not_a_pass(self) -> None:
        (self.repo / evidence.EVIDENCE_ARTIFACTS[0]).unlink()
        with self.assertRaisesRegex(evidence.EvidenceError, "Not a file"):
            self.capture()

    def test_capture_locator_is_hashed_into_the_manifest(self) -> None:
        screenshot = self.repo / "verification" / "out" / "slice.txt"
        screenshot.write_text("frame dump")
        manifest, problems = self.capture(accept=[*ACCEPT[1:], ("input", {"kind": "capture", "path": "verification/out/slice.txt"})])
        self.assertEqual(problems, [])
        entry = manifest["acceptance_evidence"]["input"][0]
        self.assertEqual(entry["sha256"], evidence.sha256_file(screenshot))
        screenshot.write_text("different frame dump")
        self.assertTrue(evidence.validate_runtime_evidence(
            manifest, repo=self.repo, head=git(self.repo, "rev-parse", "HEAD"), working_tree_clean=True,
            source_fingerprint=FINGERPRINT, action_table_sha256=ACTION_TABLE, acceptance_keys=KEYS,
        ))

    def test_parse_accept(self) -> None:
        self.assertEqual(evidence.parse_accept("input=log_event:roll.start"), ("input", {"kind": "log_event", "id": "roll.start"}))
        self.assertEqual(evidence.parse_accept("stamina=ticks:10-20"), ("stamina", {"kind": "tick_range", "start": 10, "end": 20}))
        self.assertEqual(evidence.parse_accept("input=capture:verification/out/a.png"), ("input", {"kind": "capture", "path": "verification/out/a.png"}))
        for bad in ("input", "input=", "input=ticks:10", "input=ticks:a-b", "input=guess:1", "=log_event:x"):
            with self.subTest(bad=bad), self.assertRaises(evidence.EvidenceError):
                evidence.parse_accept(bad)


class ReleaseGateTests(unittest.TestCase):
    """`verify.check_runtime_evidence` is the release profile's entry point."""

    def setUp(self) -> None:
        self._temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self._temporary.cleanup)
        self.repo = make_repo(Path(self._temporary.name))
        (self.repo / ".gitignore").write_text(
            (self.repo / ".gitignore").read_text() + "core/src/sim/extracted.rs\n"
        )
        git(self.repo, "add", ".gitignore")
        git(self.repo, "commit", "-q", "-m", "ignore owner table")
        self.extracted = self.repo / "core" / "src" / "sim" / "extracted.rs"
        self.extracted.parent.mkdir(parents=True)
        self.extracted.write_text("// owner table\n")
        self.contract = {"release_acceptance": KEYS}
        self.evidence_path = self.repo / "verification" / "runtime-evidence.json"
        manifest, problems = evidence.capture_runtime_evidence(
            self.repo,
            source_fingerprint=FINGERPRINT,
            action_table_sha256=evidence.sha256_file(self.extracted),
            facts=FACTS,
            log="verification/out/runtime.log",
            accept=copy.deepcopy(ACCEPT),
            acceptance_keys=KEYS,
        )
        self.assertEqual(problems, [])
        self.evidence_path.write_text(json.dumps(manifest))
        real_run = verify.run
        self.patch = mock.patch.multiple(
            verify,
            ROOT=self.repo,
            EVIDENCE_PATH=self.evidence_path,
            EXTRACTED_PATH=self.extracted,
            run=lambda command, cwd=None, env=None: real_run(command, cwd=self.repo, env=env),
        )
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def test_matching_evidence_passes(self) -> None:
        self.assertIn("3 cited acceptance assertions verified", verify.check_runtime_evidence(self.contract, FINGERPRINT))

    def test_requires_owner_extracted_source_data(self) -> None:
        with self.assertRaisesRegex(verify.VerificationError, "without owner-extracted source data"):
            verify.check_runtime_evidence(self.contract, None)

    def test_new_commit_makes_the_evidence_stale(self) -> None:
        (self.repo / "tracked.txt").write_text("changed\n")
        git(self.repo, "commit", "-q", "-am", "change")
        with self.assertRaisesRegex(verify.VerificationError, "stale: evidence is for commit"):
            verify.check_runtime_evidence(self.contract, FINGERPRINT)

    def test_dirty_tree_makes_the_evidence_stale(self) -> None:
        (self.repo / "tracked.txt").write_text("changed\n")
        with self.assertRaisesRegex(verify.VerificationError, "working tree has changes"):
            verify.check_runtime_evidence(self.contract, FINGERPRINT)

    def test_replaced_action_table_makes_the_evidence_stale(self) -> None:
        self.extracted.write_text("// a different owner table\n")
        with self.assertRaisesRegex(verify.VerificationError, "current extracted action table"):
            verify.check_runtime_evidence(self.contract, FINGERPRINT)

    def test_other_source_fingerprint_makes_the_evidence_stale(self) -> None:
        with self.assertRaisesRegex(verify.VerificationError, "owner-data source fingerprint"):
            verify.check_runtime_evidence(self.contract, "9" * 64)

    def test_contract_change_makes_the_evidence_stale(self) -> None:
        with self.assertRaisesRegex(verify.VerificationError, "missing assertion: perspective_toggle_continuity"):
            verify.check_runtime_evidence({"release_acceptance": [*KEYS, "perspective_toggle_continuity"]}, FINGERPRINT)


if __name__ == "__main__":
    unittest.main()
