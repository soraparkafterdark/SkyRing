from __future__ import annotations

from pathlib import Path
import struct
import sys
import tempfile
import unittest
import zlib

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import skyrim_re as re_tools  # noqa: E402

SECTION_RVA = 0x1000
SECTION_RAW = 0x400


def rva(offset: int) -> int:
    return SECTION_RVA + offset


def build_pe(section: str = ".rdata", version: tuple[int, int, int, int] = (1, 0, 0, 0)) -> bytes:
    """A small but structurally real PE32+ DLL: imports, exports, CodeView debug entry, version info."""
    body = bytearray(0x400)

    def put(offset: int, data: bytes) -> None:
        body[offset:offset + len(data)] = data

    put(0x00, struct.pack("<IIIII", rva(0x40), 0, 0, rva(0xA0), rva(0x40)))  # import descriptor, then a null one
    put(0x40, struct.pack("<QQQ", rva(0x60), rva(0x80), 0))
    put(0x60, b"\0\0CreateFileW\0")
    put(0x80, b"\0\0ExitProcess\0")
    put(0xA0, b"KERNEL32.dll\0")
    put(0xC0, struct.pack("<IIHHIIIIIII", 0, 0, 0, 0, rva(0x100), 1, 2, 2, rva(0x120), rva(0x130), rva(0x140)))
    put(0x100, b"test.dll\0")
    put(0x120, struct.pack("<II", rva(0x200), rva(0x210)))
    put(0x130, struct.pack("<II", rva(0x150), rva(0x170)))
    put(0x140, struct.pack("<HH", 0, 1))
    put(0x150, b"SKSEPlugin_Version\0")
    put(0x170, b"SKSEPlugin_Load\0")
    pdb = b"C:\\build\\ercombat.pdb\0"
    put(0x190, struct.pack("<IIHHIIII", 0, 1700000000, 0, 0, 2, 24 + len(pdb), rva(0x1C0), SECTION_RAW + 0x1C0))
    put(0x1C0, b"RSDS" + bytes(range(16)) + struct.pack("<I", 3) + pdb)
    ms, ls = (version[0] << 16) | version[1], (version[2] << 16) | version[3]
    put(0x300, struct.pack("<IIII", 0xFEEF04BD, 0x00010000, ms, ls))

    optional = bytearray(240)
    struct.pack_into("<H", optional, 0, 0x20B)
    struct.pack_into("<I", optional, 16, rva(0x200))
    struct.pack_into("<Q", optional, 24, 0x180000000)
    struct.pack_into("<I", optional, 56, 0x2000)
    struct.pack_into("<H", optional, 68, 3)
    struct.pack_into("<H", optional, 70, 0x8160)
    struct.pack_into("<I", optional, 108, 16)
    struct.pack_into("<II", optional, 112 + 0 * 8, rva(0xC0), 0x80)  # export
    struct.pack_into("<II", optional, 112 + 1 * 8, rva(0x00), 0x28)  # import
    struct.pack_into("<II", optional, 112 + 6 * 8, rva(0x190), 28)  # debug

    dos = b"MZ" + b"\0" * 0x3A + struct.pack("<I", 0x40)
    coff = b"PE\0\0" + struct.pack("<HHIIIHH", 0x8664, 1, 1700000000, 0, 0, len(optional), 0x2022)
    table = section.encode().ljust(8, b"\0") + struct.pack("<IIIIIIHHI", 0x400, rva(0), 0x400, SECTION_RAW, 0, 0, 0, 0,
                                                           0x60000020)
    image = bytearray(SECTION_RAW)
    image[:len(dos + coff + bytes(optional) + table)] = dos + coff + bytes(optional) + table
    return bytes(image + body)


def build_address_library(version: tuple[int, int, int, int] = (1, 6, 1170, 0)) -> bytes:
    name = b"SkyrimSE.exe"
    header = struct.pack("<5i", 2, *version) + struct.pack("<i", len(name)) + name + struct.pack("<ii", 8, 4)
    entries = (
        bytes([0x00]) + struct.pack("<QQ", 100, 0x1000)  # absolute ID and RVA
        + bytes([0x21, 0x10])  # id+1, rva+0x10
        + bytes([0xA2, 4, 3])  # id+4, ((previous rva / 8) + 3) * 8
        + bytes([0x67]) + struct.pack("<IH", 7000, 0x20)  # absolute u32 ID, u16 RVA
    )
    return header + entries


def record(kind: bytes, form_id: int, payload: bytes, flags: int = 0) -> bytes:
    return struct.pack("<4sIIIIHH", kind, len(payload), flags, form_id, 0, 44, 0) + payload


def sub(kind: bytes, data: bytes) -> bytes:
    return struct.pack("<4sH", kind, len(data)) + data


def group(label: bytes, kind: int, children: bytes) -> bytes:
    return struct.pack("<4sI4sIHHHH", b"GRUP", 24 + len(children), label, kind, 0, 0, 44, 0) + children


def weapon(form_id: int, editor_id: str, animation: int, damage: int, speed: float, reach: float,
           compressed: bool = False) -> bytes:
    dnam = struct.pack("<B3xff", animation, speed, reach).ljust(100, b"\0")
    subs = (sub(b"EDID", editor_id.encode() + b"\0") + sub(b"DATA", struct.pack("<IfH", 25, 9.0, damage))
            + sub(b"DNAM", dnam) + sub(b"KWDA", struct.pack("<II", 0x1E711, 0x6D931)))
    if compressed:
        return record(b"WEAP", form_id, struct.pack("<I", len(subs)) + zlib.compress(subs), re_tools.FLAG_COMPRESSED)
    return record(b"WEAP", form_id, subs)


def build_plugin(master: bool = True) -> bytes:
    tes4 = record(b"TES4", 0, sub(b"HEDR", struct.pack("<fiI", 1.7, 5, 0x800)) + sub(b"CNAM", b"Bethesda\0")
                  + sub(b"MAST", b"Skyrim.esm\0") + sub(b"DATA", b"\0" * 8), 0x81 if master else 0x80)
    iron = weapon(0x12EB7, "IronSword", 1, 7, 1.0, 1.0)
    nested = group(b"\0\0\0\0", 1, weapon(0x1000, "NestedSword", 5, 23, 0.9, 1.3, compressed=True))
    armor = record(b"ARMO", 0x13920, sub(b"EDID", b"ArmorIronCuirass\0"))
    return tes4 + group(b"WEAP", 0, iron + nested) + group(b"ARMO", 0, armor)


class PeTests(unittest.TestCase):
    def test_parses_sections_imports_exports_and_debug_identity(self) -> None:
        pe = re_tools.parse_pe(build_pe(version=(1, 6, 1170, 0)))
        self.assertTrue(pe["is_x64"] and pe["pe32_plus"] and pe["is_dll"])
        self.assertEqual(pe["file_version"], "1.6.1170.0")
        self.assertEqual(pe["image_base"], "0x180000000")
        self.assertTrue(pe["aslr"] and pe["nx"])
        self.assertEqual(pe["section_names"], [".rdata"])
        self.assertEqual(pe["imports"], [{"dll": "KERNEL32.dll", "functions": ["CreateFileW", "ExitProcess"]}])
        self.assertEqual(pe["exports"]["name"], "test.dll")
        self.assertEqual([f["name"] for f in pe["exports"]["functions"]], ["SKSEPlugin_Version", "SKSEPlugin_Load"])
        pdb = pe["debug"][0]
        self.assertEqual(pdb["pdb_path"], "C:\\build\\ercombat.pdb")
        self.assertEqual(pdb["pdb_guid"], bytes(range(16)).hex())
        self.assertEqual(pdb["pdb_age"], 3)
        self.assertEqual(pe["errors"], [])

    def test_steamstub_section_is_flagged(self) -> None:
        self.assertTrue(re_tools.parse_pe(build_pe(section=".bind"))["steam_drm_wrapper"])
        self.assertFalse(re_tools.parse_pe(build_pe())["steam_drm_wrapper"])

    def test_corrupt_directories_are_reported_without_losing_the_rest(self) -> None:
        image = bytearray(build_pe())
        struct.pack_into("<I", image, SECTION_RAW + 0x00, 0x00FFFFFF)  # import descriptor points outside the image
        pe = re_tools.parse_pe(bytes(image))
        self.assertTrue(any(error.startswith("imports:") for error in pe["errors"]))
        self.assertEqual(pe["exports"]["name"], "test.dll")

    def test_garbage_and_truncated_files_raise_a_clear_error(self) -> None:
        for data in (b"", b"not a pe file at all" * 8, build_pe()[:0x90]):
            with self.assertRaises(re_tools.ReError):
                re_tools.parse_pe(data)

    def test_non_pe32_plus_gets_only_the_shallow_view(self) -> None:
        image = bytearray(build_pe())
        struct.pack_into("<H", image, 0x58, 0x10B)  # PE32 magic
        pe = re_tools.parse_pe(bytes(image))
        self.assertFalse(pe["pe32_plus"])
        self.assertNotIn("imports", pe)

    def test_entropy_separates_text_from_random_looking_data(self) -> None:
        self.assertEqual(re_tools.shannon_entropy(b"\0" * 1024), 0.0)
        self.assertAlmostEqual(re_tools.shannon_entropy(bytes(range(256)) * 4), 8.0, places=6)


class AddressLibraryTests(unittest.TestCase):
    def test_delta_encoding_round_trips_and_consumes_the_file(self) -> None:
        database = re_tools.parse_address_library(build_address_library())
        self.assertEqual(database["entries"], {100: 0x1000, 101: 0x1010, 105: 0x1028, 7000: 0x20})
        self.assertEqual(database["version"], "1.6.1170.0")
        self.assertEqual(database["module"], "SkyrimSE.exe")
        self.assertTrue(database["parsed_exactly"])

    def test_trailing_bytes_or_short_counts_are_not_accepted_as_exact(self) -> None:
        self.assertFalse(re_tools.parse_address_library(build_address_library() + b"\0\0")["parsed_exactly"])
        with self.assertRaises(re_tools.ReError):
            re_tools.parse_address_library(build_address_library()[:-3])

    def test_bad_headers_are_rejected(self) -> None:
        data = bytearray(build_address_library())
        struct.pack_into("<i", data, 0, 9)
        for blob in (bytes(data), b"\0" * 10):
            with self.assertRaises(re_tools.ReError):
                re_tools.parse_address_library(blob)

    def test_lookup_by_id_and_by_rva(self) -> None:
        database = re_tools.parse_address_library(build_address_library())
        self.assertEqual(re_tools.lookup_address(database, ident=105), {"id": 105, "rva": "0x1028"})
        self.assertEqual(re_tools.lookup_address(database, rva=0x1010), {"id": 101, "rva": "0x1010"})
        self.assertIsNone(re_tools.lookup_address(database, ident=1))

    def test_read_at_rva_maps_through_the_section_table(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "x.dll"
            path.write_bytes(build_pe())
            pe = re_tools.analyze_pe(path)
            self.assertEqual(re_tools.read_at_rva(path, pe, rva(0x100), 8), b"test.dll")
            self.assertIsNone(re_tools.read_at_rva(path, pe, 0x10, 4))


class PluginTests(unittest.TestCase):
    def test_header_reports_flags_masters_and_declared_count(self) -> None:
        header = re_tools.read_plugin_header(build_plugin())
        self.assertTrue(header["master"])
        self.assertFalse(header["light"])
        self.assertEqual(header["masters"], ["Skyrim.esm"])
        self.assertEqual(header["author"], "Bethesda")
        self.assertEqual(header["record_count"], 5)
        self.assertFalse(re_tools.read_plugin_header(build_plugin(master=False))["master"])

    def test_group_summary_descends_into_nested_groups(self) -> None:
        self.assertEqual(re_tools.summarize_plugin(build_plugin())["groups"], {"ARMO": 1, "WEAP": 2})

    def test_weapons_decode_including_compressed_records_inside_subgroups(self) -> None:
        weapons = re_tools.extract_weapons(build_plugin())
        self.assertEqual([w["editor_id"] for w in weapons], ["IronSword", "NestedSword"])
        iron, nested = weapons
        self.assertEqual((iron["animation_type"], iron["damage"], iron["value"], iron["weight"]),
                         ("OneHandSword", 7, 25, 9.0))
        self.assertEqual((iron["speed"], iron["reach"]), (1.0, 1.0))
        self.assertEqual(iron["keywords"], ["0001E711", "0006D931"])
        self.assertEqual((nested["animation_type"], nested["damage"], nested["speed"]), ("TwoHandSword", 23, 0.9))
        self.assertEqual(iron["form_id"], "00012EB7")

    def test_non_plugins_and_overrunning_groups_are_rejected(self) -> None:
        with self.assertRaises(re_tools.ReError):
            re_tools.read_plugin_header(b"NOPE" + b"\0" * 40)
        broken = bytearray(build_plugin())
        struct.pack_into("<I", broken, broken.index(b"GRUP") + 4, 10_000_000)
        with self.assertRaises(re_tools.ReError):
            re_tools.summarize_plugin(bytes(broken))

    def test_xxxx_extended_subrecord_sizes_are_honoured(self) -> None:
        big = b"A" * 70000
        data = struct.pack("<4sHI", b"XXXX", 4, len(big)) + struct.pack("<4sH", b"DATA", 0) + big + sub(b"EDID", b"x\0")
        self.assertEqual([(k, len(v)) for k, v in re_tools.subrecords(data)], [("DATA", 70000), ("EDID", 2)])


class StringTests(unittest.TestCase):
    def test_ascii_and_wide_strings_with_offsets_and_filter(self) -> None:
        blob = b"\0\0SkyrimSE\0\0\0" + "wide text".encode("utf-16-le") + b"\0\0ab\0"
        self.assertEqual(re_tools.extract_strings(blob)[0], {"offset": 2, "text": "SkyrimSE"})
        self.assertEqual([s["text"] for s in re_tools.extract_strings(blob, wide=True)], ["wide text"])
        self.assertEqual(re_tools.extract_strings(blob, pattern="^Sky")[0]["text"], "SkyrimSE")
        self.assertEqual(len(re_tools.extract_strings(b"aaaaaa\0bbbbbb\0cccccc", limit=2)), 2)


if __name__ == "__main__":
    unittest.main()
