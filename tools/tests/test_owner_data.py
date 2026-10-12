from __future__ import annotations

import json
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile
import unittest
import zipfile

import pytest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import owner_data  # noqa: E402


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
            owner_data.create_bundle(source, upstream, game, False, bundle, False)
            manifest = owner_data.validate_bundle(bundle)
            self.assertFalse(manifest["source_game"]["sounds_included"])
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
            self.assertTrue((private_root / "manifest.json").is_file())

    def test_bundle_hash_mismatch_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, upstream, game = self.make_fixture(root)
            bundle = root / "owner-data.zip"
            owner_data.create_bundle(source, upstream, game, False, bundle, False)

            broken = root / "broken.zip"
            with zipfile.ZipFile(bundle) as original, zipfile.ZipFile(broken, "w") as output:
                for info in original.infolist():
                    payload = original.read(info.filename)
                    if info.filename == "generated/src/sim/extracted.rs":
                        payload += b"tampered"
                    output.writestr(info.filename, payload)
            with self.assertRaisesRegex(owner_data.OwnerDataError, "size/hash verification"):
                owner_data.validate_bundle(broken)

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

    def test_windows_game_dir_accept(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            game = Path(directory) / "Game"
            game.mkdir()
            for name in ("eldenring.exe", "regulation.bin", "oo2core_6_win64.dll"):
                (game / name).write_bytes(b"placeholder")
            # Should not raise
            owner_data.validate_game_dir(game)

    def test_windows_game_dir_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            for missing in ("eldenring.exe", "regulation.bin", "oo2core_6_win64.dll"):
                with self.subTest(missing=missing):
                    game = Path(directory) / f"game_without_{missing}"
                    game.mkdir()
                    for name in ("eldenring.exe", "regulation.bin", "oo2core_6_win64.dll"):
                        if name != missing:
                            (game / name).write_bytes(b"placeholder")
                    with self.assertRaisesRegex(
                        owner_data.OwnerDataError,
                        r"not an Elden Ring Game directory",
                    ) as context:
                        owner_data.validate_game_dir(game)
                    self.assertIn(missing, str(context.exception))

    def test_print_nuxe_plan_text(self) -> None:
        import contextlib
        import io

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            game = root / "Game"
            source = root / "Source"
            game.mkdir()
            source.mkdir()
            for include_sounds in (True, False):
                with self.subTest(include_sounds=include_sounds):
                    buffer = io.StringIO()
                    with contextlib.redirect_stdout(buffer):
                        owner_data.print_nuxe_plan(game, source, include_sounds)
                    output = buffer.getvalue()
                    self.assertIn("Elden Ring (PC)", output)
                    self.assertIn(str(game), output)
                    self.assertIn("Unpack directory", output)
                    self.assertIn(str(source), output)
                    expected_filter = owner_data.nuxe_filter(include_sounds)
                    self.assertIn(f"Filter: {expected_filter}", output)
                    if include_sounds:
                        self.assertIn("sd/", expected_filter)
                    else:
                        self.assertNotIn("sd/", expected_filter)

    def test_validate_nuxe_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            (source / "chr").mkdir(parents=True)
            for name in owner_data.ANIMATION_BINDERS:
                (source / "chr" / name).write_bytes(b"animation")
            (source / "regulation.bin").write_bytes(b"regulation")
            (source / "oo2core_6_win64.dll").write_bytes(b"oodle")
            # Should not raise
            owner_data.validate_nuxe_output(source, False)
            # Remove one binder and expect failure
            missing = owner_data.ANIMATION_BINDERS[0]
            (source / "chr" / missing).unlink()
            with self.assertRaisesRegex(
                owner_data.OwnerDataError,
                r"Nuxe output is incomplete\. Missing:",
            ) as context:
                owner_data.validate_nuxe_output(source, False)
            self.assertIn(missing, str(context.exception))

if __name__ == "__main__":
    unittest.main()
