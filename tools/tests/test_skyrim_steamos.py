from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
sys.path.insert(0, str(TOOLS / "tests"))
import skyrim_steamos as steamos  # noqa: E402
import test_skyrim_re as re_tests  # noqa: E402

APPMANIFEST = """"AppState"
{{
\t"appid"\t\t"489830"
\t"name"\t\t"Skyrim Special Edition"
\t"StateFlags"\t\t"{flags}"
\t"installdir"\t\t"Skyrim Special Edition"
\t"LastUpdated"\t\t"1700000000"
\t"SizeOnDisk"\t\t"1"
\t"buildid"\t\t"12345678"
\t"TargetBuildID"\t\t"{target}"
}}
"""
LIBRARYFOLDERS = """"libraryfolders"
{{
\t"0"
\t{{
\t\t"path"\t\t"{root}"
\t\t"apps"
\t\t{{
\t\t\t"489830"\t\t"1"
\t\t}}
\t}}
}}
"""
CONFIG = """"InstallConfigStore"
{
\t"Software"
\t{
\t\t"Valve"
\t\t{
\t\t\t"Steam"
\t\t\t{
\t\t\t\t"CompatToolMapping"
\t\t\t\t{
\t\t\t\t\t"489830"
\t\t\t\t\t{
\t\t\t\t\t\t"name"\t\t"proton_experimental"
\t\t\t\t\t}
\t\t\t\t}
\t\t\t}
\t\t}
\t}
}
"""


def fake_exe(version: tuple[int, int, int, int], machine: int = 0x8664, sections: tuple[str, ...] = (".text", ".bind")) -> bytes:
    """A minimal PE with a section table and a VS_FIXEDFILEINFO record."""
    optional = b"\0" * 240
    coff = b"PE\0\0" + struct.pack("<HHIIIHH", machine, len(sections), 1700000000, 0, 0, len(optional), 0x22)
    table = b"".join(name.encode().ljust(8, b"\0") + b"\0" * 32 for name in sections)
    ms = (version[0] << 16) | version[1]
    ls = (version[2] << 16) | version[3]
    fixed = struct.pack("<IIII", 0xFEEF04BD, 0x00010000, ms, ls)
    header = b"MZ" + b"\0" * 0x3A + struct.pack("<I", 0x40)
    return header + coff + optional + table + b"\0" * 64 + b"junk\xbd\x04\xef\xfe" + fixed + b"\0" * 64


def build_install(
    root: Path,
    *,
    version: tuple[int, int, int, int] = (1, 6, 1170, 0),
    flags: int = 4,
    target: str = "12345678",
    skse: bool = True,
    address_library: bool = True,
    prefix: bool = True,
    exe_name: str = "SkyrimSE.exe",
    data_name: str = "Data",
) -> Path:
    steam = root / "Steam"
    steamapps = steam / "steamapps"
    game = steamapps / "common" / "Skyrim Special Edition"
    (game / data_name / "SKSE" / "Plugins").mkdir(parents=True)
    steamapps.mkdir(parents=True, exist_ok=True)
    (steam / "config").mkdir()
    (steam / "config" / "config.vdf").write_text(CONFIG, encoding="utf-8")
    (steamapps / "libraryfolders.vdf").write_text(LIBRARYFOLDERS.format(root=steam), encoding="utf-8")
    (steamapps / "appmanifest_489830.acf").write_text(APPMANIFEST.format(flags=flags, target=target), encoding="utf-8")
    (game / exe_name).write_bytes(fake_exe(version))
    for master in steamos.BASE_MASTERS:
        (game / data_name / master).write_bytes(master.encode())
    (game / data_name / "Skyrim - Textures0.bsa").write_bytes(b"bsa")
    (game / data_name / "ccBGSSSE001-Fish.esm").write_bytes(b"cc")
    dotted = ".".join(str(part) for part in version[:3])
    if skse:
        (game / "skse64_loader.exe").write_bytes(b"loader")
        (game / f"skse64_{dotted.replace('.', '_')}.dll").write_bytes(b"skse")
    if address_library:
        name = steamos.expected_address_library(".".join(str(part) for part in version))
        (game / data_name / "SKSE" / "Plugins" / name).write_bytes(b"addr")
    if prefix:
        base = steamapps / "compatdata" / "489830"
        logs = base / "pfx" / "drive_c" / "users" / "steamuser" / "Documents" / "My Games" / "Skyrim Special Edition" / "SKSE"
        logs.mkdir(parents=True)
        (logs / "skse64.log").write_text("log", encoding="utf-8")
        (base / "version").write_text("1700000000 proton-9.0-3\n", encoding="utf-8")
    return steam


class SkyrimSteamOSTests(unittest.TestCase):
    def setUp(self) -> None:
        self._directory = tempfile.TemporaryDirectory()
        self.addCleanup(self._directory.cleanup)
        self.root = Path(self._directory.name)

    def test_vdf_parser_reads_nested_blocks_escapes_and_comments(self) -> None:
        tree = steamos.parse_vdf('// note\n"A"\n{\n"p" "C:\\\\x \\"q\\""\n"B" { "k" "v" }\n}\n')
        self.assertEqual(steamos.vdf_get(tree, "a", "p"), 'C:\\x "q"')
        self.assertEqual(steamos.vdf_get(tree, "A", "B", "K"), "v")
        self.assertIsNone(steamos.vdf_get(tree, "A", "missing", "k"))

    def test_vdf_parser_rejects_malformed_files(self) -> None:
        for text in ('"A" {', '"A" { "k" } }', '{ "k" "v" }', '"A" { "k" "v" } }'):
            with self.assertRaises(steamos.SteamOSProbeError, msg=text):
                steamos.parse_vdf(text)

    def test_complete_install_is_ready(self) -> None:
        steam = build_install(self.root)
        report = steamos.probe(steam_root=steam)
        self.assertEqual(report["blockers"], [])
        self.assertEqual(report["status"], "ready")
        self.assertEqual(report["skyrim_version"], "1.6.1170.0")
        self.assertEqual(report["edition"], "AE")
        self.assertEqual(report["location"]["manifest"]["buildid"], "12345678")
        self.assertTrue(report["executable"]["is_x64"])
        self.assertTrue(report["executable"]["steam_drm_wrapper"])
        self.assertTrue(report["skse"]["matches_exe"])
        self.assertTrue(report["address_library"]["matches_exe"])
        self.assertEqual(report["proton"]["compat_tool_mapping"], "proton_experimental")
        self.assertIn("proton-9.0-3", report["proton"]["proton_build"])
        self.assertEqual(report["proton"]["logs"], ["skse64.log"])
        self.assertEqual(report["data"]["creation_club_plugins"], ["ccBGSSSE001-Fish.esm"])
        self.assertTrue(report["runtime_fingerprint_complete"])
        self.assertFalse(any(report["skyring_plugin"].values()))
        self.assertIn("not release evidence", report["claim_ceiling"])

    def test_missing_skse_address_library_and_prefix_are_blocked_not_passed(self) -> None:
        steam = build_install(self.root, skse=False, address_library=False, prefix=False)
        report = steamos.probe(steam_root=steam)
        self.assertEqual(report["status"], "blocked")
        joined = "\n".join(report["blockers"])
        self.assertIn("skse64_loader.exe", joined)
        self.assertIn("Address Library", joined)
        self.assertIn("Proton prefix", joined)

    def test_skse_build_for_another_exe_version_is_blocked(self) -> None:
        steam = build_install(self.root)
        game = steam / "steamapps" / "common" / "Skyrim Special Edition"
        (game / "skse64_1_6_1170.dll").rename(game / "skse64_1_6_640.dll")
        report = steamos.probe(steam_root=steam)
        self.assertFalse(report["skse"]["matches_exe"])
        self.assertTrue(any("skse64_1_6_1170.dll" in blocker for blocker in report["blockers"]))

    def test_se_uses_the_version_dash_address_library_name(self) -> None:
        self.assertEqual(steamos.expected_address_library("1.5.97.0"), "version-1-5-97-0.bin")
        self.assertEqual(steamos.expected_address_library("1.6.1170.0"), "versionlib-1-6-1170-0.bin")
        steam = build_install(self.root, version=(1, 5, 97, 0))
        report = steamos.probe(steam_root=steam)
        self.assertEqual(report["edition"], "SE")
        self.assertEqual(report["blockers"], [])
        self.assertTrue(any("targets 1.6.1170" in warning for warning in report["warnings"]))

    def test_case_mismatched_names_from_a_windows_era_install_are_found(self) -> None:
        steam = build_install(self.root, exe_name="skyrimse.exe", data_name="data")
        report = steamos.probe(steam_root=steam)
        self.assertEqual(report["blockers"], [])
        self.assertEqual(report["skyrim_version"], "1.6.1170.0")

    def test_partial_install_state_is_blocked_and_midupdate_is_warned(self) -> None:
        report = steamos.probe(steam_root=build_install(self.root, flags=1026))
        self.assertTrue(any("fully installed" in blocker for blocker in report["blockers"]))
        other = self.root / "second"
        other.mkdir()
        report = steamos.probe(steam_root=build_install(other, flags=4 | 256, target="999"))
        self.assertEqual(report["blockers"], [])
        self.assertTrue(any("mid-update" in warning for warning in report["warnings"]))

    def test_non_x64_or_unversioned_exe_is_blocked(self) -> None:
        steam = build_install(self.root)
        exe = steam / "steamapps" / "common" / "Skyrim Special Edition" / "SkyrimSE.exe"
        exe.write_bytes(fake_exe((1, 6, 1170, 0), machine=0x14C))
        self.assertTrue(any("not x64" in blocker for blocker in steamos.probe(steam_root=steam)["blockers"]))
        exe.write_bytes(fake_exe((1, 6, 1170, 0)).replace(b"\xbd\x04\xef\xfe", b"\0\0\0\0"))
        self.assertTrue(any("no readable file version" in blocker for blocker in steamos.probe(steam_root=steam)["blockers"]))

    def test_fingerprint_tracks_the_executable_and_masters(self) -> None:
        steam = build_install(self.root)
        first = steamos.probe(steam_root=steam)["runtime_fingerprint"]
        self.assertEqual(first, steamos.probe(steam_root=steam)["runtime_fingerprint"])
        master = steam / "steamapps" / "common" / "Skyrim Special Edition" / "Data" / "Skyrim.esm"
        master.write_bytes(b"patched")
        self.assertNotEqual(first, steamos.probe(steam_root=steam)["runtime_fingerprint"])

    def test_skipping_data_hashes_marks_the_fingerprint_incomplete(self) -> None:
        report = steamos.probe(steam_root=build_install(self.root), hash_data=False)
        self.assertFalse(report["runtime_fingerprint_complete"])
        self.assertTrue(any("hashing skipped" in warning for warning in report["warnings"]))

    def test_not_installed_is_blocked_and_names_unsupported_legendary_edition(self) -> None:
        steam = self.root / "Steam"
        (steam / "steamapps").mkdir(parents=True)
        with self.assertRaisesRegex(steamos.SteamOSProbeError, "BLOCKED: Skyrim SE/AE .* not found"):
            steamos.locate(steam_root=steam)
        (steam / "steamapps" / "appmanifest_72850.acf").write_text("x", encoding="utf-8")
        with self.assertRaisesRegex(steamos.SteamOSProbeError, "Legendary Edition"):
            steamos.locate(steam_root=steam)

    def test_game_on_a_second_library_is_found_through_libraryfolders(self) -> None:
        steam = self.root / "Steam"
        (steam / "steamapps").mkdir(parents=True)
        sd = build_install(self.root / "sd")  # a full library that Steam only knows about by path
        (steam / "steamapps" / "libraryfolders.vdf").write_text(LIBRARYFOLDERS.format(root=sd), encoding="utf-8")
        location = steamos.locate(steam_root=steam)
        self.assertEqual(Path(location["library"]), sd.resolve())
        self.assertTrue(location["game_dir"].endswith("Skyrim Special Edition"))

    def test_cli_writes_report_and_exit_code_reflects_blockers(self) -> None:
        out = self.root / "report.json"
        steam = build_install(self.root)
        with contextlib.redirect_stdout(io.StringIO()) as stdout:
            code = steamos.main(["probe", "--steam-root", str(steam), "--out", str(out)])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out.read_text(encoding="utf-8"))["status"], "ready")
        self.assertIn("Skyrim AE 1.6.1170.0", stdout.getvalue())
        self.assertIn("READY (SteamOS/Proton preflight only)", stdout.getvalue())

        other = self.root / "blocked"
        other.mkdir()
        steam = build_install(other, skse=False)
        with contextlib.redirect_stdout(io.StringIO()) as stdout:
            code = steamos.main(["probe", "--steam-root", str(steam), "--out", str(out)])
        self.assertEqual(code, 1)
        self.assertEqual(json.loads(out.read_text(encoding="utf-8"))["status"], "blocked")
        self.assertIn("BLOCKED: skse64_loader.exe", stdout.getvalue())

    def test_missing_install_exits_nonzero_with_a_blocked_message(self) -> None:
        empty = self.root / "Steam"
        (empty / "steamapps").mkdir(parents=True)
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            self.assertEqual(steamos.main(["probe", "--steam-root", str(empty), "--out", str(self.root / "r.json")]), 1)
        self.assertIn("BLOCKED", stderr.getvalue())
        self.assertFalse((self.root / "r.json").exists())


class DeepCommandTests(unittest.TestCase):
    """The reverse-engineering subcommands, run end to end against a synthetic install."""

    def setUp(self) -> None:
        self._directory = tempfile.TemporaryDirectory()
        self.addCleanup(self._directory.cleanup)
        self.root = Path(self._directory.name)
        self.steam = build_install(self.root)
        self.game = self.steam / "steamapps" / "common" / "Skyrim Special Edition"
        self.data = self.game / "Data"
        (self.game / "SkyrimSE.exe").write_bytes(re_tests.build_pe(section=".bind", version=(1, 6, 1170, 0)))
        (self.data / "Skyrim.esm").write_bytes(re_tests.build_plugin())
        plugins = self.data / "SKSE" / "Plugins"
        (plugins / "versionlib-1-6-1170-0.bin").write_bytes(re_tests.build_address_library())
        (plugins / "ERCombat.dll").write_bytes(re_tests.build_pe())
        (plugins / "oldplugin.dll").write_bytes(re_tests.build_pe().replace(b"SKSEPlugin_Version", b"SKSEPlugin_Query\0\0"))
        (plugins / "helper.dll").write_bytes(b"MZ")

    def run_cli(self, *args: str) -> tuple[int, str]:
        out = self.root / "out.json"
        with contextlib.redirect_stdout(io.StringIO()) as stdout, contextlib.redirect_stderr(io.StringIO()) as stderr:
            code = steamos.main([args[0], "--steam-root", str(self.steam), *args[1:], "--out", str(out)])
        return code, stdout.getvalue() + stderr.getvalue()

    def report(self) -> dict | list:
        return json.loads((self.root / "out.json").read_text(encoding="utf-8"))

    def test_exe_reports_drm_wrapper_and_identity(self) -> None:
        code, text = self.run_cli("exe")
        self.assertEqual(code, 0)
        self.assertIn("version 1.6.1170.0", text)
        self.assertIn("SteamStub", text)
        report = self.report()
        self.assertTrue(report["steam_drm_wrapper"])
        self.assertEqual(len(report["sha256"]), 64)

    def test_address_library_lookup_and_exactness(self) -> None:
        code, text = self.run_cli("address-library", "--id", "105", "--rva", "0x1010", "--id", "0x1B58")
        self.assertEqual(code, 0)
        self.assertIn("4/4 entries", text)
        self.assertEqual([(h["id"], h["rva"]) for h in self.report()["lookups"]],
                         [(105, "0x1028"), (7000, "0x20"), (101, "0x1010")])

    def test_address_library_peek_warns_about_encrypted_code_and_missing_library_is_blocked(self) -> None:
        _, text = self.run_cli("address-library", "--id", "100", "--peek")
        self.assertIn("encrypted on disk", text)
        (self.data / "SKSE" / "Plugins" / "versionlib-1-6-1170-0.bin").unlink()
        code, text = self.run_cli("address-library")
        self.assertEqual(code, 1)
        self.assertIn("BLOCKED: Address Library", text)

    def test_plugins_audit_separates_ae_and_se_only_plugins(self) -> None:
        code, text = self.run_cli("plugins")
        self.assertEqual(code, 0)
        by_name = {entry["name"]: entry for entry in self.report()}
        self.assertTrue(by_name["ERCombat.dll"]["loads_on_ae"])
        self.assertIs(by_name["oldplugin.dll"]["loads_on_ae"], False)
        self.assertIn("error", by_name["helper.dll"])
        self.assertIn("MISSING: er_combat_core.dll", text)

    def test_esm_and_weapons(self) -> None:
        code, text = self.run_cli("esm")
        self.assertEqual(code, 0)
        self.assertEqual(self.report()["groups"], {"ARMO": 1, "WEAP": 2})
        code, text = self.run_cli("weapons", "--filter", "TwoHand")
        self.assertEqual(code, 0)
        self.assertEqual([w["editor_id"] for w in self.report()], ["NestedSword"])
        self.assertIn("1 WEAP record(s)", text)

    def test_strings_filters_and_reports(self) -> None:
        code, text = self.run_cli("strings", "--pattern", "^SKSEPlugin_L")
        self.assertEqual(code, 0)
        self.assertEqual(len(self.report()), 1)
        self.assertEqual(self.report()[0]["text"], "SKSEPlugin_Load")

    def test_logs_lists_notable_lines_and_blocks_without_a_prefix(self) -> None:
        log = (self.steam / "steamapps" / "compatdata" / "489830" / "pfx" / "drive_c" / "users" / "steamuser"
               / "Documents" / "My Games" / "Skyrim Special Edition" / "SKSE" / "skse64.log")
        log.write_text("skse64 loaded\nplugin ERCombat.dll loaded correctly\nunrelated\n", encoding="utf-8")
        code, text = self.run_cli("logs", "--tail", "1")
        self.assertEqual(code, 0)
        self.assertIn("plugin ERCombat.dll loaded correctly", text)
        self.assertEqual(self.report()["skse64.log"]["tail"], ["unrelated"])
        log.unlink()
        for parent in list(log.parents)[:1]:
            parent.rmdir()
        code, text = self.run_cli("logs")
        self.assertEqual(code, 1)
        self.assertIn("BLOCKED", text)

    def test_launch_help_is_static_and_states_the_claim_ceiling(self) -> None:
        with contextlib.redirect_stdout(io.StringIO()) as stdout:
            self.assertEqual(steamos.main(["launch-help"]), 0)
        self.assertIn("skse64_loader.exe", stdout.getvalue())
        self.assertIn("not release evidence", stdout.getvalue())


if __name__ == "__main__":
    unittest.main()
