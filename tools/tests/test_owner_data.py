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
        (upstream / "src" / "sim").mkdir(parents=True)
        (upstream / "assets").mkdir(parents=True)
        game.mkdir()
        for name in owner_data.ANIMATION_BINDERS:
            (source / "chr" / name).write_bytes(f"animation:{name}".encode())
        (source / "regulation.bin").write_bytes(b"encrypted regulation")
        (source / "regulation-bin" / "EquipParamWeapon.param").write_bytes(b"param")
        (source / "oo2core_6_win64.dll").write_bytes(b"owner oodle")
        (upstream / "src" / "sim" / "extracted.rs").write_text("// extracted\n", encoding="utf-8")
        (upstream / "assets" / "player_anims.bin").write_bytes(b"animations")
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


if __name__ == "__main__":
    unittest.main()
