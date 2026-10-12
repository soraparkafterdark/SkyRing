from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import package_mod  # noqa: E402


def make_dist(root: Path, extra: dict[str, bytes] | None = None, omit: tuple[str, ...] = ()) -> Path:
    dist = root / "dist"
    files = {
        "SKSE/Plugins/ERCombat.dll": b"MZ synthetic plugin",
        "SKSE/Plugins/er_combat_core.dll": b"MZ synthetic core",
        "SKSE/Plugins/ERCombat.ini": b"[General]\nCoreHitDetection=0\n",
    }
    files.update(extra or {})
    for name in omit:
        files.pop(name)
    for name, data in files.items():
        path = dist / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    return dist


class PackageTests(unittest.TestCase):
    def package(self, root: Path, dist: Path, **kwargs):
        kwargs.setdefault("extracted", root / "absent-extracted.rs")
        return package_mod.package(dist, root / "out", **kwargs)

    def test_synthetic_package_lists_only_allowed_members(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive, inventory = self.package(root, make_dist(root))
            self.assertTrue(inventory["synthetic"])
            self.assertFalse(inventory["contains_owner_data"])
            self.assertEqual(
                [entry["path"] for entry in inventory["files"]], sorted(package_mod.ALLOWED_MEMBERS)
            )
            with zipfile.ZipFile(archive) as bundle:
                self.assertEqual(bundle.namelist(), sorted(package_mod.ALLOWED_MEMBERS))
            written = json.loads((root / "out" / f"{archive.name}.inventory.json").read_text(encoding="utf-8"))
            self.assertEqual(written, inventory)

    def test_archive_is_byte_reproducible(self) -> None:
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            _, one = self.package(Path(first), make_dist(Path(first)))
            _, two = self.package(Path(second), make_dist(Path(second)))
            self.assertEqual(one["archive"]["sha256"], two["archive"]["sha256"])

    def test_forbidden_files_fail_closed(self) -> None:
        forbidden = {
            "SKSE/Plugins/player_anims.bin": "generated animation or audio",
            "SKSE/Plugins/cs_main.bnk": "generated animation or audio",
            "SKSE/Plugins/c0000.anibnd.dcx": "owner data",
            "SKSE/Plugins/extracted.rs": "owner data",
            "SKSE/Plugins/regulation.bin": "owner data",
            "SKSE/Plugins/ERCombat.log": "log or runtime evidence",
            "SKSE/Plugins/ERCombat.pdb": "log or runtime evidence",
            "owner_data/current.json": "owner data",
        }
        for name, reason in forbidden.items():
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with self.assertRaises(package_mod.PackageError) as context:
                    self.package(root, make_dist(root, {name: b"x"}))
                self.assertIn(name, str(context.exception))
                self.assertIn(reason, str(context.exception))
                self.assertFalse((root / "out").exists(), "nothing may be written when the gate fails")

    def test_secret_names_are_forbidden(self) -> None:
        # Checked by name, not by writing the files: the swarm's sandbox refuses to create a
        # file called .env even inside a temporary directory.
        for name in ("SKSE/Plugins/.env", ".env.local", "SKSE/Plugins/github_token.txt", "keys/id_rsa",
                     "keys/id_ed25519.pub", "certs/server.pem", "certs/server.key", "credentials.json"):
            with self.subTest(name=name):
                self.assertIn("secret", package_mod.forbidden_reason(name) or "", name)
        for name in ("SKSE/Plugins/ERCombat.dll", "SKSE/Plugins/ERCombat.ini"):
            self.assertIsNone(package_mod.forbidden_reason(name))

    def test_unlisted_files_are_refused_not_skipped(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(package_mod.PackageError, "not in the allowed input list"):
                self.package(root, make_dist(root, {"SKSE/Plugins/notes.txt": b"hello"}))

    def test_missing_binaries_are_blocked_not_passed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dist = make_dist(root, omit=("SKSE/Plugins/ERCombat.dll",))
            with self.assertRaises(package_mod.Blocked) as context:
                self.package(root, dist)
            self.assertIn("ERCombat.dll", str(context.exception))
            with self.assertRaises(package_mod.Blocked):
                self.package(root, root / "no-dist")

    def test_secrets_and_home_paths_in_text_are_refused(self) -> None:
        for content in (b"token=ghp_" + b"a" * 30, b"-----BEGIN RSA PRIVATE KEY-----", b"log=/Users/someone/Desktop/x"):
            with self.subTest(content=content[:12]), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                dist = make_dist(root, {"SKSE/Plugins/ERCombat.ini": b"[General]\n" + content})
                with self.assertRaises(package_mod.PackageError):
                    self.package(root, dist)

    def test_owner_extracted_table_blocks_a_synthetic_package(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            extracted = root / "extracted.rs"
            extracted.write_text("// owner table\n", encoding="utf-8")
            dist = make_dist(root)
            with self.assertRaisesRegex(package_mod.PackageError, "embeds owner-derived data"):
                self.package(root, dist, extracted=extracted)
            archive, inventory = self.package(root, dist, extracted=extracted, owner_build=True)
            self.assertTrue(inventory["contains_owner_data"])
            self.assertIn("OWNER-PRIVATE", archive.name)

    def test_symbolic_links_are_not_followed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dist = make_dist(root)
            secret = root / "outside.txt"
            secret.write_text("outside", encoding="utf-8")
            (dist / "SKSE" / "Plugins" / "link.txt").symlink_to(secret)
            with self.assertRaisesRegex(package_mod.PackageError, "symbolic links"):
                self.package(root, dist)

    def test_repository_dist_contains_only_allowed_inputs(self) -> None:
        tracked = {
            path.relative_to(package_mod.DIST).as_posix()
            for path in package_mod.DIST.rglob("*")
            if path.is_file() and path.suffix not in (".dll", ".pdb")
        }
        self.assertLessEqual(tracked, set(package_mod.ALLOWED_MEMBERS))


if __name__ == "__main__":
    unittest.main()
