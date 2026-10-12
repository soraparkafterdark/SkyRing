from __future__ import annotations

import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import abi_gate  # noqa: E402
from verify import find_cxx  # noqa: E402


def real_layout() -> dict:
    return abi_gate.load_snapshot(abi_gate.SNAPSHOT_PATH)["layout"]


class SignatureParsingTests(unittest.TestCase):
    def test_header_and_rust_signatures_agree(self) -> None:
        header = abi_gate.parse_header_functions(abi_gate.HEADER_PATH.read_text(encoding="utf-8"))
        rust = abi_gate.parse_rust_exports(abi_gate.FFI_PATH.read_text(encoding="utf-8"))
        self.assertEqual(abi_gate.diff_functions(rust, header), [])
        self.assertEqual(set(header), {"er_create", "er_destroy", "er_update", "er_receive_hit", "er_confirm_hit"})

    def test_rust_export_must_spell_extern_c(self) -> None:
        for declaration in (
            '#[no_mangle]\npub extern "system" fn er_x(a: u8) {\n}\n',
            '#[no_mangle]\npub fn er_x(a: u8) {\n}\n',
            '#[export_name = "er_x"]\npub extern "C" fn y(a: u8) {\n}\n',
        ):
            with self.subTest(declaration=declaration), self.assertRaises(abi_gate.AbiError):
                abi_gate.parse_rust_exports(declaration)

    def test_signature_drift_is_reported(self) -> None:
        header = abi_gate.parse_header_functions("void er_x(float dt, const ErInput* input);")
        rust = abi_gate.parse_rust_exports(
            '#[no_mangle]\npub unsafe extern "C" fn er_x(dt: f64, input: *const ErInput) {\n}\n'
        )
        # f64 is not a scalar the C side can declare, so it stays distinct from `float`.
        problems = abi_gate.diff_functions(rust, header)
        self.assertEqual(len(problems), 1)
        self.assertIn("er_x signature differs", problems[0])

    def test_missing_and_extra_exports_are_reported(self) -> None:
        header = abi_gate.parse_header_functions("void er_a(void);\nvoid er_b(void);")
        rust = abi_gate.parse_rust_exports('#[no_mangle]\npub extern "C" fn er_a() {\n}\n')
        problems = abi_gate.diff_functions(rust, header)
        self.assertEqual(problems, ["er_b is declared in the C header but not exported by Rust"])


class LayoutComparisonTests(unittest.TestCase):
    def test_offset_size_and_enum_drift_are_detected(self) -> None:
        pinned = real_layout()
        mutated = copy.deepcopy(pinned)
        mutated["structs"]["ErOutput"]["fields"]["hit_stop"] += 4
        mutated["structs"]["ErInput"]["size"] += 4
        mutated["enums"]["ErAction"]["values"]["hurt"] = 99
        del mutated["structs"]["ErWorld"]["fields"]["in_combat"]
        problems = abi_gate.diff_layouts(pinned, mutated, "pinned", "mutated")
        self.assertEqual(len(problems), 4)
        self.assertTrue(any("ErOutput differs" in problem for problem in problems))
        self.assertTrue(any("ErAction differs" in problem for problem in problems))

    def test_enum_count_sentinel_must_follow_last_value(self) -> None:
        raw = {
            "structs": {"S": {"size": 1, "align": 1, "fields": {}}},
            "enums": {"E": {"size": 4, "values": {"A": 0, "B": 1, "COUNT": 5}}},
        }
        with self.assertRaisesRegex(abi_gate.AbiError, "COUNT=5"):
            abi_gate.normalize_layout(raw, origin="C")
        raw["enums"]["E"]["values"]["COUNT"] = 2
        self.assertEqual(abi_gate.normalize_layout(raw, origin="C")["enums"]["E"]["values"], {"a": 0, "b": 1})

    def test_enum_names_compare_across_rust_and_c_spellings(self) -> None:
        self.assertEqual(
            abi_gate.normalize_enum_values({"CrouchRoll": 2}),
            abi_gate.normalize_enum_values({"CROUCH_ROLL": 2}),
        )


class HeaderProbeTests(unittest.TestCase):
    """Compile the real probe against the real and against mutated headers."""

    def stage(self, root: Path, edit=None) -> Path:
        (root / "core" / "include").mkdir(parents=True)
        (root / "plugin" / "tests").mkdir(parents=True)
        header = abi_gate.HEADER_PATH.read_text(encoding="utf-8")
        if edit is not None:
            mutated = edit(header)
            self.assertNotEqual(mutated, header, "the mutation must change the header")
            header = mutated
        (root / "core" / "include" / "er_combat.h").write_text(header, encoding="utf-8")
        shutil.copy(abi_gate.PROBE_PATH, root / "plugin" / "tests" / "AbiLayout.cpp")
        return root

    def c_layout(self, edit=None) -> dict:
        with tempfile.TemporaryDirectory() as directory:
            root = self.stage(Path(directory) / "repo", edit)
            work = Path(directory) / "work"
            work.mkdir()
            return abi_gate.normalize_layout(abi_gate.run_c_layout(find_cxx(), work, root), origin="C")

    def test_real_header_matches_pinned_snapshot(self) -> None:
        pinned = real_layout()
        layout = self.c_layout()
        self.assertEqual(abi_gate.diff_layouts({k: pinned[k] for k in ("structs", "enums")}, layout, "snapshot", "header"), [])

    def test_added_field_changes_layout(self) -> None:
        layout = self.c_layout(lambda header: header.replace("uint8_t crouch, walk;", "uint8_t crouch, walk, sprint;"))
        pinned = real_layout()
        problems = abi_gate.diff_layouts({k: pinned[k] for k in ("structs", "enums")}, layout, "snapshot", "header")
        self.assertTrue(any("ErInput" in problem for problem in problems), problems)

    def test_reordered_fields_change_offsets(self) -> None:
        layout = self.c_layout(
            lambda header: header.replace(
                "float move_x, move_y; /* stick", "float move_y, move_x; /* stick"
            )
        )
        pinned = real_layout()
        self.assertNotEqual(layout["structs"]["ErInput"], pinned["structs"]["ErInput"])

    def test_changed_enum_value_is_visible(self) -> None:
        layout = self.c_layout(lambda header: header.replace("ER_HIT_BLOCKED = 3", "ER_HIT_BLOCKED = 13"))
        self.assertEqual(layout["enums"]["ErHitResult"]["values"]["blocked"], 13)

    def test_changed_prototype_stops_the_probe_compiling(self) -> None:
        with self.assertRaisesRegex(abi_gate.AbiError, "does not compile"):
            self.c_layout(lambda header: header.replace("void er_confirm_hit(ErCombat* combat);", "int er_confirm_hit(ErCombat* combat);"))


class SnapshotTests(unittest.TestCase):
    def test_snapshot_schema_is_enforced(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "abi.json"
            path.write_text(json.dumps({"schema_version": 9, "revision": 1, "layout": {}}), encoding="utf-8")
            with self.assertRaisesRegex(abi_gate.AbiError, "unsupported schema"):
                abi_gate.load_snapshot(path)
            with self.assertRaisesRegex(abi_gate.AbiError, "cannot read"):
                abi_gate.load_snapshot(Path(directory) / "missing.json")


if __name__ == "__main__":
    unittest.main()
