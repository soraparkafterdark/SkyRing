"""Macos-side rehearsal of the owner-dump ingress path, using synthetic bytes only.

Covers what the owner relies on when the Windows dump arrives: the bundle is
validated, imported into a SkyRing-shaped checkout with a provenance receipt,
and every refusal path leaves the checkout exactly as it was.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

TESTS = Path(__file__).resolve().parent
sys.path.insert(0, str(TESTS))
sys.path.insert(0, str(TESTS.parent))
import owner_data  # noqa: E402
from test_owner_data import OwnerDataTests  # noqa: E402

BUILD_ID = "12345678"


def make_fixture(root: Path) -> tuple[Path, Path, Path]:
    # Reuse the synthetic dump builder rather than inventing a second one.
    return OwnerDataTests("test_nuxe_filter_is_exact_and_sound_is_opt_in").make_fixture(root)


def make_checkout(root: Path, *, with_core_sim: bool = True) -> Path:
    repo = root / "repo"
    (repo / "verification").mkdir(parents=True)
    (repo / "verification" / "contract.json").write_text("{}\n", encoding="utf-8")
    if with_core_sim:
        (repo / "core" / "src" / "sim").mkdir(parents=True)
    return repo


def rewrite_member(source: Path, destination: Path, member: str, rewrite) -> None:
    with zipfile.ZipFile(source) as original, zipfile.ZipFile(destination, "w") as output:
        for info in original.infolist():
            payload = original.read(info.filename)
            if info.filename == member:
                payload = rewrite(payload)
            output.writestr(info, payload)


def snapshot(repo: Path) -> dict[str, bytes]:
    return {
        path.relative_to(repo).as_posix(): path.read_bytes()
        for path in sorted(repo.rglob("*"))
        if path.is_file()
    }


class IngressRehearsalTests(unittest.TestCase):
    def build_bundle(self, root: Path) -> Path:
        source, upstream, game = make_fixture(root)
        bundle = root / "owner-data.zip"
        owner_data.create_bundle(source, upstream, game, False, bundle, False, BUILD_ID)
        return bundle

    def test_validate_returns_manifest_for_elden_ring(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            bundle = self.build_bundle(Path(directory))
            manifest = owner_data.validate_bundle(bundle)
            self.assertEqual(manifest["schema_version"], owner_data.SCHEMA_VERSION)
            self.assertEqual(manifest["source_game"]["game_id"], "elden-ring-pc")
            self.assertEqual(manifest["source_game"]["title"], "ELDEN RING")
            self.assertEqual(manifest["source_game"]["steam_app_id"], "1245620")

    def test_import_installs_payload_receipt_and_action_table(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = self.build_bundle(root)
            manifest = owner_data.validate_bundle(bundle)
            repo = make_checkout(root)

            private_root = owner_data.import_bundle(bundle, repo, False)

            self.assertEqual(private_root, repo / "owner_data" / manifest["source_fingerprint"][:16])
            self.assertTrue((private_root / "generated" / "src" / "sim" / "extracted.rs").is_file())
            self.assertEqual(
                (repo / "core" / "src" / "sim" / "extracted.rs").read_bytes(),
                (private_root / "generated" / "src" / "sim" / "extracted.rs").read_bytes(),
            )
            receipt = json.loads((repo / owner_data.RECEIPT_PATH).read_text(encoding="utf-8"))
            self.assertEqual(receipt["source_fingerprint"], manifest["source_fingerprint"])
            self.assertEqual(receipt["bundle_sha256"], owner_data.sha256(bundle))
            self.assertEqual(
                receipt["action_table_sha256"],
                manifest["files"]["generated/src/sim/extracted.rs"]["sha256"],
            )
            self.assertEqual(receipt["source_game"]["game_id"], "elden-ring-pc")
            self.assertEqual(receipt["source_game"]["steam_build_id"], BUILD_ID)
            self.assertEqual(receipt["private_root"], private_root.relative_to(repo).as_posix())

    def test_fresh_clone_imports_without_force(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = self.build_bundle(root)
            repo = make_checkout(root)
            self.assertFalse((repo / "core" / "src" / "sim" / "extracted.rs").exists())
            owner_data.import_bundle(bundle, repo, False)
            self.assertTrue((repo / "core" / "src" / "sim" / "extracted.rs").is_file())

    def test_reimport_is_refused_and_changes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = self.build_bundle(root)
            repo = make_checkout(root)
            owner_data.import_bundle(bundle, repo, False)
            before = snapshot(repo)
            with self.assertRaisesRegex(owner_data.OwnerDataError, "already imported"):
                owner_data.import_bundle(bundle, repo, False)
            self.assertEqual(snapshot(repo), before)

    def test_tampered_member_is_refused_and_leaves_no_residue(self) -> None:
        for member in ("source/regulation.bin", "generated/src/sim/extracted.rs"):
            for label, rewrite in (
                ("hash drift", lambda payload: bytes([payload[0] ^ 1]) + payload[1:]),
                ("size drift", lambda payload: payload + b"x"),
            ):
                with self.subTest(member=member, drift=label), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    bundle = self.build_bundle(root)
                    tampered = root / "tampered.zip"
                    rewrite_member(bundle, tampered, member, rewrite)
                    repo = make_checkout(root)
                    before = snapshot(repo)
                    with self.assertRaisesRegex(
                        owner_data.OwnerDataError, f"size/hash verification: {member}"
                    ):
                        owner_data.import_bundle(tampered, repo, False)
                    self.assertEqual(snapshot(repo), before)
                    self.assertFalse((repo / "owner_data").exists())

    def test_other_game_identity_is_refused_even_when_hashes_agree(self) -> None:
        for game_id, title, app_id in (
            ("cyberpunk-2077", "Cyberpunk 2077", "1091500"),
            ("skyrim-se", "The Elder Scrolls V: Skyrim Special Edition", "489830"),
        ):
            with self.subTest(game=game_id), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                bundle = self.build_bundle(root)

                def relabel(payload: bytes) -> bytes:
                    manifest = json.loads(payload)
                    manifest["source_game"].update(game_id=game_id, title=title, steam_app_id=app_id)
                    # Keep the manifest internally consistent: only identity is wrong.
                    manifest["source_fingerprint"] = owner_data.calculate_source_fingerprint(manifest)
                    return (json.dumps(manifest, indent=2) + "\n").encode()

                relabelled = root / "relabelled.zip"
                rewrite_member(bundle, relabelled, "manifest.json", relabel)
                repo = make_checkout(root)
                before = snapshot(repo)
                with self.assertRaisesRegex(owner_data.OwnerDataError, "unsupported source game"):
                    owner_data.import_bundle(relabelled, repo, False)
                self.assertEqual(snapshot(repo), before)

    def test_non_skyring_destination_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = self.build_bundle(root)
            elsewhere = root / "elsewhere"
            elsewhere.mkdir()
            with self.assertRaisesRegex(owner_data.OwnerDataError, "not a SkyRing checkout"):
                owner_data.import_bundle(bundle, elsewhere, False)
            self.assertEqual(snapshot(elsewhere), {})


if __name__ == "__main__":
    unittest.main()
