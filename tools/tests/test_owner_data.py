from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

import pytest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import owner_data  # noqa: E402
import verify as verification_tool  # noqa: E402


class OwnerDataTests(unittest.TestCase):
    def test_nuxe_filter_is_exact_and_sound_is_opt_in(self) -> None:
        without_sound = re.compile(owner_data.nuxe_filter(False))
        with_sound = re.compile(owner_data.nuxe_filter(True))
        for name in owner_data.ANIMATION_BINDERS:
            self.assertIsNotNone(without_sound.fullmatch(f"/chr/{name}"))
        self.assertIsNone(without_sound.fullmatch("/chr/c0000_a6x.anibnd.dcx"))
        self.assertIsNone(without_sound.fullmatch("/sd/cs_smain.bnk"))
        for name in owner_data.SOUND_BANKS:
            self.assertIsNotNone(with_sound.fullmatch(f"/{name}"))

    def test_zip_member_validation_rejects_traversal(self) -> None:
        for name in (
            "../secret",
            "/absolute",
            "C:/windows/system32",
            "source\\..\\secret",
            "source/file:alternate-stream",
        ):
            with self.assertRaises(owner_data.OwnerDataError):
                owner_data.safe_member(name)
        self.assertEqual(owner_data.safe_member("source/chr/file"), PurePosixPath("source/chr/file"))

    def make_fixture(self, root: Path) -> tuple[Path, Path, Path]:
        source = root / "source"
        upstream = root / "upstream"
        game = root / "Game"
        (source / "chr").mkdir(parents=True)
        (source / "regulation-bin").mkdir(parents=True)
        (source / "sd").mkdir(parents=True)
        (source / "sd" / "enus").mkdir(parents=True)
        (upstream / "src" / "sim").mkdir(parents=True)
        (upstream / "assets").mkdir(parents=True)
        (upstream / "assets" / "sounds").mkdir(parents=True)
        game.mkdir()
        for name in owner_data.ANIMATION_BINDERS:
            (source / "chr" / name).write_bytes(f"animation:{name}".encode())
        (source / "regulation.bin").write_bytes(b"encrypted regulation")
        (source / "regulation-bin" / "EquipParamWeapon.param").write_bytes(b"param")
        (source / "oo2core_6_win64.dll").write_bytes(b"owner oodle")
        for name in owner_data.SOUND_BANKS:
            (source / name).write_bytes(f"sound:{name}".encode())
        (upstream / "src" / "sim" / "extracted.rs").write_text("// extracted\n", encoding="utf-8")
        (upstream / "assets" / "player_anims.bin").write_bytes(b"animations")
        (upstream / "assets" / "player_sounds.bin").write_bytes(b"player sounds")
        (upstream / "assets" / "sounds" / "footstep.wav").write_bytes(b"footstep")
        (game / "eldenring.exe").write_bytes(b"game build")
        return source, upstream, game

    def test_bundle_round_trip_and_mac_import(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, False, bundle, False, "12345678")
            manifest = owner_data.validate_bundle(bundle)
            self.assertFalse(manifest["source_game"]["sounds_included"])
            self.assertEqual(manifest["source_game"]["game_id"], owner_data.SOURCE_GAME_ID)
            self.assertEqual(manifest["source_game"]["steam_build_id"], "12345678")
            self.assertRegex(manifest["source_fingerprint"], r"^[0-9a-f]{64}$")
            self.assertNotIn(str(root), json.dumps(manifest), "manifest must not leak local paths")

            repo = root / "repo"
            (repo / "core" / "src" / "sim").mkdir(parents=True)
            (repo / "verification").mkdir()
            (repo / "verification" / "contract.json").write_text("{}\n", encoding="utf-8")
            private_root = owner_data.import_bundle(bundle, repo, False)
            self.assertEqual(
                (repo / "core" / "src" / "sim" / "extracted.rs").read_text(encoding="utf-8"),
                "// extracted\n",
            )
            self.assertTrue((private_root / "source" / "chr" / owner_data.ANIMATION_BINDERS[0]).is_file())
            self.assertTrue((private_root / "generated" / "assets" / "player_anims.bin").is_file())
            self.assertTrue((private_root / "generated" / "src" / "sim" / "extracted.rs").is_file())
            self.assertTrue((private_root / "manifest.json").is_file())
            receipt = json.loads((repo / owner_data.RECEIPT_PATH).read_text(encoding="utf-8"))
            self.assertEqual(receipt["source_fingerprint"], manifest["source_fingerprint"])
            self.assertEqual(receipt["action_table_sha256"], owner_data.sha256(repo / "core/src/sim/extracted.rs"))
            self.assertEqual(receipt["private_root"], private_root.relative_to(repo).as_posix())

            with mock.patch.multiple(
                verification_tool,
                ROOT=repo,
                EXTRACTED_PATH=repo / "core/src/sim/extracted.rs",
                OWNER_RECEIPT_PATH=repo / owner_data.RECEIPT_PATH,
            ):
                detail, fingerprint = verification_tool.check_source_data(True)
                self.assertEqual(fingerprint, manifest["source_fingerprint"])
                self.assertIn("Steam build 12345678", detail)

                (repo / "core/src/sim/extracted.rs").write_text("// tampered\n", encoding="utf-8")
                with self.assertRaisesRegex(verification_tool.VerificationError, "not the table"):
                    verification_tool.check_source_data(True)

    def test_bundle_hash_mismatch_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, False, bundle, False, "12345678")

            broken = root / "broken.zip"
            with zipfile.ZipFile(bundle) as original, zipfile.ZipFile(broken, "w") as output:
                for info in original.infolist():
                    payload = original.read(info.filename)
                    if info.filename == "generated/src/sim/extracted.rs":
                        payload += b"tampered"
                    output.writestr(info.filename, payload)
            with self.assertRaisesRegex(owner_data.OwnerDataError, "size/hash verification"):
                owner_data.validate_bundle(broken)

    def test_cyberpunk_source_identity_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, False, bundle, False, "12345678")
            mislabeled = root / "cyberpunk.zip"
            with zipfile.ZipFile(bundle) as original, zipfile.ZipFile(mislabeled, "w") as output:
                for info in original.infolist():
                    payload = original.read(info.filename)
                    if info.filename == "manifest.json":
                        manifest = json.loads(payload)
                        manifest["source_game"]["game_id"] = "cyberpunk-2077"
                        manifest["source_game"]["title"] = "Cyberpunk 2077"
                        manifest["source_fingerprint"] = owner_data.calculate_source_fingerprint(manifest)
                        payload = (json.dumps(manifest, indent=2) + "\n").encode()
                    output.writestr(info, payload)
            with self.assertRaisesRegex(owner_data.OwnerDataError, "unsupported source game"):
                owner_data.validate_bundle(mislabeled)

    def test_pinned_upstream_metadata_is_enforced(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, False, bundle, False, "12345678")
            changed = root / "wrong-upstream.zip"
            with zipfile.ZipFile(bundle) as original, zipfile.ZipFile(changed, "w") as output:
                for info in original.infolist():
                    payload = original.read(info.filename)
                    if info.filename == "manifest.json":
                        manifest = json.loads(payload)
                        manifest["upstream"]["commit"] = "0" * 40
                        manifest["source_fingerprint"] = owner_data.calculate_source_fingerprint(manifest)
                        payload = (json.dumps(manifest, indent=2) + "\n").encode()
                    output.writestr(info, payload)
            with self.assertRaisesRegex(owner_data.OwnerDataError, "pinned generator"):
                owner_data.validate_bundle(changed)

    def test_source_fingerprint_is_stable_across_bundle_creation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            first = root / "first.zip"
            second = root / "second.zip"
            owner_data.create_bundle(source, upstream, game, False, first, False, "12345678")
            owner_data.create_bundle(source, upstream, game, False, second, False, "12345678")
            first_manifest = owner_data.validate_bundle(first)
            second_manifest = owner_data.validate_bundle(second)
            self.assertEqual(first_manifest["source_fingerprint"], second_manifest["source_fingerprint"])

    def test_missing_required_raw_input_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, False, bundle, False, "12345678")
            incomplete = root / "incomplete.zip"
            omitted = f"source/chr/{owner_data.ANIMATION_BINDERS[0]}"
            with zipfile.ZipFile(bundle) as original, zipfile.ZipFile(incomplete, "w") as output:
                manifest = json.loads(original.read("manifest.json"))
                manifest["files"].pop(omitted)
                manifest["source_fingerprint"] = owner_data.calculate_source_fingerprint(manifest)
                output.writestr("manifest.json", json.dumps(manifest, indent=2) + "\n")
                for info in original.infolist():
                    if info.filename not in {"manifest.json", omitted}:
                        output.writestr(info, original.read(info.filename))
            with self.assertRaisesRegex(owner_data.OwnerDataError, "required Elden Ring"):
                owner_data.validate_bundle(incomplete)

    def test_normalized_duplicate_zip_paths_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory) / "ambiguous.zip"
            with zipfile.ZipFile(bundle, "w") as output:
                output.writestr("source//same", b"one")
                output.writestr("source/same", b"two")
            with self.assertRaisesRegex(owner_data.OwnerDataError, "normalize to the same"):
                owner_data.validate_bundle(bundle)

    def test_reimport_of_same_bundle_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, False, bundle, False)

            repo = root / "repo"
            (repo / "core" / "src" / "sim").mkdir(parents=True)
            (repo / "verification").mkdir()
            (repo / "verification" / "contract.json").write_text("{}\n", encoding="utf-8")

            fingerprint = owner_data.sha256(bundle)[:16]
            private_root = owner_data.import_bundle(bundle, repo, False)
            self.assertTrue((private_root / "manifest.json").is_file())

            with self.assertRaises(owner_data.OwnerDataError) as context:
                owner_data.import_bundle(bundle, repo, False)
            self.assertIn("already imported", str(context.exception))

            self.assertTrue((private_root / "manifest.json").is_file())
            self.assertEqual(private_root, repo / "owner_data" / fingerprint)

    def test_tampered_bundle_is_refused_with_no_residue(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, False, bundle, False)

            tampered = root / "tampered.zip"
            first_binder = owner_data.ANIMATION_BINDERS[0]
            with zipfile.ZipFile(bundle) as original, zipfile.ZipFile(tampered, "w") as output:
                for info in original.infolist():
                    payload = original.read(info.filename)
                    if info.filename == f"source/chr/{first_binder}":
                        payload = b"tampered"
                    output.writestr(info.filename, payload)

            with self.assertRaisesRegex(owner_data.OwnerDataError, "bundle member failed size/hash verification: source/chr/"):
                owner_data.validate_bundle(tampered)

            repo = root / "repo"
            (repo / "core" / "src" / "sim").mkdir(parents=True)
            (repo / "verification").mkdir()
            (repo / "verification" / "contract.json").write_text("{}\n", encoding="utf-8")
            with self.assertRaisesRegex(owner_data.OwnerDataError, "bundle member failed size/hash verification: source/chr/"):
                owner_data.import_bundle(tampered, repo, False)

            fingerprint = owner_data.sha256(bundle)[:16]
            self.assertFalse((repo / "owner_data" / fingerprint).is_dir())
            self.assertFalse((repo / "owner_data" / "current.json").is_file())
            self.assertFalse((repo / "core" / "src" / "sim" / "extracted.rs").is_file())

    def test_sounds_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, True, bundle, False)
            manifest = owner_data.validate_bundle(bundle)
            self.assertTrue(manifest["source_game"]["sounds_included"])
            for name in owner_data.SOUND_BANKS:
                self.assertIn(f"source/{name}", manifest["files"])
            for name in owner_data.ANIMATION_BINDERS:
                self.assertIn(f"source/chr/{name}", manifest["files"])
            self.assertIn("generated/assets/player_sounds.bin", manifest["files"])
            self.assertNotIn(str(root), json.dumps(manifest), "manifest must not leak local paths")

    def test_sounds_import(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, True, bundle, False)

            repo = root / "repo"
            (repo / "core" / "src" / "sim").mkdir(parents=True)
            (repo / "verification").mkdir()
            (repo / "verification" / "contract.json").write_text("{}\n", encoding="utf-8")

            fingerprint = owner_data.sha256(bundle)[:16]
            private_root = owner_data.import_bundle(bundle, repo, False)
            self.assertEqual(private_root, repo / "owner_data" / fingerprint)
            self.assertTrue((repo / "core" / "src" / "sim" / "extracted.rs").is_file())
            self.assertTrue((private_root / "source" / "sd" / "cs_smain.bnk").is_file())
            self.assertTrue((private_root / "generated" / "assets" / "player_sounds.bin").is_file())
            self.assertTrue((private_root / "manifest.json").is_file())

    def test_sounds_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            upstream = root / "upstream"
            game = root / "Game"
            (source / "chr").mkdir(parents=True)
            (source / "regulation-bin").mkdir(parents=True)
            (source / "sd").mkdir(parents=True)
            (source / "sd" / "enus").mkdir(parents=True)
            (upstream / "src" / "sim").mkdir(parents=True)
            (upstream / "assets").mkdir(parents=True)
            (upstream / "assets" / "sounds").mkdir(parents=True)
            game.mkdir()
            for name in owner_data.ANIMATION_BINDERS:
                (source / "chr" / name).write_bytes(f"animation:{name}".encode())
            (source / "regulation.bin").write_bytes(b"encrypted regulation")
            (source / "regulation-bin" / "EquipParamWeapon.param").write_bytes(b"param")
            (source / "oo2core_6_win64.dll").write_bytes(b"owner oodle")
            (source / "sd" / "cs_smain.bnk").write_bytes(b"sound:cs_smain")
            (upstream / "src" / "sim" / "extracted.rs").write_text("// extracted\n", encoding="utf-8")
            (upstream / "assets" / "player_anims.bin").write_bytes(b"animations")
            (upstream / "assets" / "player_sounds.bin").write_bytes(b"player sounds")
            (upstream / "assets" / "sounds" / "footstep.wav").write_bytes(b"footstep")
            (game / "eldenring.exe").write_bytes(b"game build")
            with self.assertRaisesRegex(owner_data.OwnerDataError, "bundle input is missing:"):
                owner_data.create_bundle(source, upstream, game, True, root / "bundle.zip", False)


    def test_action_validation(self) -> None:
        # (a) missing required keys
        with self.assertRaisesRegex(owner_data.OwnerDataError, "action at index 0 missing required key: id"):
            owner_data.validate_action_list([{"kind": "attack", "total": 5, "stamina_cost": 0, "hit_windows": [[1, 2]]}])
        with self.assertRaisesRegex(owner_data.OwnerDataError, "action at index 0 missing required key: kind"):
            owner_data.validate_action_list([{"id": 1, "total": 5, "stamina_cost": 0, "hit_windows": [[1, 2]]}])
        with self.assertRaisesRegex(owner_data.OwnerDataError, "action at index 0 missing required key: total"):
            owner_data.validate_action_list([{"id": 1, "kind": "attack", "stamina_cost": 0, "hit_windows": [[1, 2]]}])
        with self.assertRaisesRegex(owner_data.OwnerDataError, "action at index 0 missing required key: stamina_cost"):
            owner_data.validate_action_list([{"id": 1, "kind": "attack", "total": 5, "hit_windows": [[1, 2]]}])
        with self.assertRaisesRegex(owner_data.OwnerDataError, "action at index 0 missing required key: hit_windows"):
            owner_data.validate_action_list([{"id": 1, "kind": "attack", "total": 5, "stamina_cost": 0}])

        # (b) negative stamina_cost rejected
        with self.assertRaisesRegex(owner_data.OwnerDataError, "action at index 0 stamina_cost must be non-negative"):
            owner_data.validate_action_list([{"id": 1, "kind": "attack", "total": 5, "stamina_cost": -1, "hit_windows": [[1, 2]]}])

        # (c) duplicate hit window ranges rejected
        with self.assertRaisesRegex(owner_data.OwnerDataError, r"action at index 0 has duplicate hit window range: \(1, 2\)"):
            owner_data.validate_action_list([{"id": 1, "kind": "attack", "total": 5, "stamina_cost": 0, "hit_windows": [[1, 2], [1, 2]]}])

        # (d) empty actions list rejected
        with self.assertRaisesRegex(owner_data.OwnerDataError, "action list is empty"):
            owner_data.validate_action_list([])

        # (e) valid action list accepted
        valid_actions = [
            {"id": 1, "kind": "attack", "total": 10, "stamina_cost": 5, "hit_windows": [[2, 4], [6, 8]]},
            {"id": "special", "kind": "skill", "total": 20, "stamina_cost": 0, "hit_windows": [[5, 5]]},
        ]
        # Should not raise
        owner_data.validate_action_list(valid_actions)

class WindowsDiscoveryTests(unittest.TestCase):
    REQUIRED_GAME_FILES = ("eldenring.exe", "regulation.bin", "oo2core_6_win64.dll")

    def make_game_dir(self, root: Path) -> Path:
        game = root / "Game"
        game.mkdir()
        for name in self.REQUIRED_GAME_FILES:
            (game / name).write_bytes(b"synthetic")
        return game

    def test_valid_game_dir_is_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            owner_data.validate_game_dir(self.make_game_dir(Path(directory)))

    def test_missing_required_file_fails_closed_and_is_named(self) -> None:
        for missing in self.REQUIRED_GAME_FILES:
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as directory:
                game = self.make_game_dir(Path(directory))
                (game / missing).unlink()
                with self.assertRaises(owner_data.OwnerDataError) as context:
                    owner_data.validate_game_dir(game)
                message = str(context.exception)
                self.assertIn("not an Elden Ring Game directory", message)
                self.assertIn(missing, message)
                for present in set(self.REQUIRED_GAME_FILES) - {missing}:
                    self.assertNotIn(present, message)

    def test_nuxe_plan_prints_exact_filter(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            game = self.make_game_dir(root)
            source = root / "source"
            for include_sounds in (False, True):
                with self.subTest(include_sounds=include_sounds):
                    captured = io.StringIO()
                    with contextlib.redirect_stdout(captured):
                        owner_data.print_nuxe_plan(game, source, include_sounds)
                    output = captured.getvalue()
                    expected_filter = owner_data.nuxe_filter(include_sounds)
                    self.assertIn("Elden Ring (PC)", output)
                    self.assertIn(str(game), output)
                    self.assertIn("Unpack directory:", output)
                    self.assertIn(str(source), output)
                    self.assertIn("Filter: " + expected_filter, output)
                    self.assertEqual("/sd/" in expected_filter, include_sounds)
                    self.assertEqual("/sd/" in output, include_sounds)
                    for name in owner_data.ANIMATION_BINDERS:
                        self.assertIn(re.escape(name), expected_filter)

    def test_validate_nuxe_output_names_missing_binder(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            (source / "chr").mkdir(parents=True)
            for name in owner_data.ANIMATION_BINDERS:
                (source / "chr" / name).write_bytes(b"binder")
            (source / "regulation.bin").write_bytes(b"regulation")
            (source / "oo2core_6_win64.dll").write_bytes(b"oodle")
            owner_data.validate_nuxe_output(source, False)

            absent = owner_data.ANIMATION_BINDERS[0]
            (source / "chr" / absent).unlink()
            with self.assertRaisesRegex(owner_data.OwnerDataError, "Nuxe output is incomplete. Missing:") as context:
                owner_data.validate_nuxe_output(source, False)
            self.assertIn(f"chr/{absent}", str(context.exception))

    def test_validate_nuxe_output_requires_sound_banks_only_when_requested(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            (source / "chr").mkdir(parents=True)
            for name in owner_data.ANIMATION_BINDERS:
                (source / "chr" / name).write_bytes(b"binder")
            owner_data.validate_nuxe_output(source, False)
            with self.assertRaisesRegex(owner_data.OwnerDataError, "Nuxe output is incomplete") as context:
                owner_data.validate_nuxe_output(source, True)
            for bank in owner_data.SOUND_BANKS:
                self.assertIn(bank, str(context.exception))


if __name__ == "__main__":
    unittest.main()
