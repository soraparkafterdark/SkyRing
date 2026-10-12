from __future__ import annotations

import json
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile
import unittest
from unittest import mock
import zipfile


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


if __name__ == "__main__":
    unittest.main()
