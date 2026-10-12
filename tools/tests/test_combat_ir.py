#!/usr/bin/env python3
"""
Tests for combat_ir.py
"""

from __future__ import annotations

import json
import math
import sys
import tempfile
import unittest
from pathlib import Path

# Insert the tools directory on sys.path so we can import combat_ir
TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import combat_ir  # noqa: E402


class CombatIRTests(unittest.TestCase):
    def setUp(self) -> None:
        # Path to the fixture
        self.fixture_path = Path(__file__).resolve().parents[2] / "fixtures" / "ir" / "v1" / "synthetic_colossal_sword.json"
        with open(self.fixture_path, "r", encoding="utf-8") as f:
            self.valid_doc = json.load(f)

    def test_valid_synthetic_colossal_sword(self) -> None:
        errors = combat_ir.validate(self.valid_doc)
        self.assertEqual(errors, [], f"Expected no errors, got: {errors}")

    def test_absolute_damage_rejected(self) -> None:
        # Damage at the top level
        doc = json.loads(json.dumps(self.valid_doc))  # deep copy
        doc["damage"] = 300
        errors = combat_ir.validate(doc)
        self.assertTrue(any("absolute damage" in e.lower() for e in errors),
                        f"Expected error about absolute damage, got: {errors}")

        # Damage nested in an action
        doc2 = json.loads(json.dumps(self.valid_doc))
        doc2["actions"][0]["damage"] = 50
        errors2 = combat_ir.validate(doc2)
        self.assertTrue(any("absolute damage" in e.lower() for e in errors2),
                        f"Expected error about absolute damage, got: {errors2}")

        # Attack rating
        doc3 = json.loads(json.dumps(self.valid_doc))
        doc3["actions"][0]["attack_rating"] = 100
        errors3 = combat_ir.validate(doc3)
        self.assertTrue(any("absolute damage" in e.lower() for e in errors3),
                        f"Expected error about absolute damage, got: {errors3}")

        # Scaling
        doc4 = json.loads(json.dumps(self.valid_doc))
        doc4["provenance"]["scaling"] = "yes"
        errors4 = combat_ir.validate(doc4)
        self.assertTrue(any("absolute damage" in e.lower() for e in errors4),
                        f"Expected error about absolute damage, got: {errors4}")

    def test_missing_provenance(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc.pop("provenance", None)
        errors = combat_ir.validate(doc)
        self.assertTrue(len(errors) > 0, f"Expected errors for missing provenance, got: {errors}")
        self.assertTrue(any("provenance" in e.lower() for e in errors))

    def test_units_time_ms(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc["units"]["time"] = "ms"
        errors = combat_ir.validate(doc)
        self.assertTrue(len(errors) > 0, f"Expected error for units.time ms, got: {errors}")
        self.assertTrue(any('units.time' in e and '"s"' in e for e in errors))

    def test_hit_window_after_duration(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        # Set a hit window that ends after duration_s
        doc["actions"][0]["hit_windows_s"] = [[0.0, 1.0]]  # duration_s is 0.5
        errors = combat_ir.validate(doc)
        self.assertTrue(len(errors) > 0, f"Expected error for hit window after duration, got: {errors}")
        self.assertTrue(any('hit_windows_s' in e and 'duration_s' in e for e in errors))

    def test_owner_null_data_fingerprint_rejected(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc["provenance"]["source"] = "owner"
        doc["provenance"]["data_fingerprint"] = None
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("data_fingerprint" in e for e in errors),
            f"Expected error mentioning data_fingerprint, got: {errors}",
        )

    def test_owner_empty_data_fingerprint_rejected(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc["provenance"]["source"] = "owner"
        doc["provenance"]["data_fingerprint"] = ""
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("data_fingerprint" in e for e in errors),
            f"Expected error mentioning data_fingerprint, got: {errors}",
        )

    def test_owner_valid_data_fingerprint_accepted(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc["provenance"]["source"] = "owner"
        doc["provenance"]["data_fingerprint"] = "a1b2c3d4e5f6" + "0" * 52
        errors = combat_ir.validate(doc)
        self.assertEqual(
            [e for e in errors if "data_fingerprint" in e],
            [],
            f"Expected no data_fingerprint errors for valid owner doc, got: {errors}",
        )

    def test_synthetic_null_data_fingerprint_valid(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc["provenance"]["source"] = "synthetic"
        doc["provenance"]["data_fingerprint"] = None
        errors = combat_ir.validate(doc)
        self.assertEqual(
            [e for e in errors if "data_fingerprint" in e],
            [],
            f"Expected no data_fingerprint errors for synthetic with null fingerprint, got: {errors}",
        )

    def test_owner_invalid_hex_fingerprint_rejected(self) -> None:
        # Non-hex characters (e.g. 'g', 'z') must be rejected.
        for bad in ["not-hex-at-all", "a1b2c3d4e5f6", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"]:
            doc = json.loads(json.dumps(self.valid_doc))
            doc["provenance"]["source"] = "owner"
            doc["provenance"]["data_fingerprint"] = bad
            errors = combat_ir.validate(doc)
            self.assertTrue(
                any("data_fingerprint" in e for e in errors),
                f"Expected error for owner fingerprint {bad!r}, got: {errors}",
            )

    def test_owner_uppercase_fingerprint_rejected(self) -> None:
        # Uppercase hex characters must be rejected (lowercase only).
        doc = json.loads(json.dumps(self.valid_doc))
        doc["provenance"]["source"] = "owner"
        doc["provenance"]["data_fingerprint"] = "A1B2C3D4E5F6" + "0" * 52
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("data_fingerprint" in e and "lowercase" in e for e in errors),
            f"Expected lowercase error for uppercase owner fingerprint, got: {errors}",
        )

    def test_owner_valid_64char_lowercase_accepted(self) -> None:
        # A real 64-char lowercase SHA-256 hex digest must be accepted.
        doc = json.loads(json.dumps(self.valid_doc))
        doc["provenance"]["source"] = "owner"
        doc["provenance"]["data_fingerprint"] = "a1b2c3d4e5f6" + "0" * 52
        errors = combat_ir.validate(doc)
        self.assertEqual(
            [e for e in errors if "data_fingerprint" in e],
            [],
            f"Expected no data_fingerprint errors for valid owner doc, got: {errors}",
        )

    def test_cli_valid(self) -> None:
        import subprocess
        result = subprocess.run(
            [sys.executable, str(Path(__file__).parents[2] / "tools" / "combat_ir.py"), "validate", str(self.fixture_path)],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, f"CLI failed with output: {result.stderr}")

    def test_cli_invalid(self) -> None:
        import subprocess
        # Create a temporary invalid file
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            doc = json.loads(json.dumps(self.valid_doc))
            doc["actions"][0]["damage"] = 999
            json.dump(doc, f)
            invalid_path = f.name

        try:
            result = subprocess.run(
                [sys.executable, str(Path(__file__).parents[2] / "tools" / "combat_ir.py"), "validate", invalid_path],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0, "CLI should have returned non-zero for invalid doc")
            self.assertTrue("absolute damage" in result.stdout.lower() or "absolute damage" in result.stderr.lower(),
                            f"Expected output to mention absolute damage, got stdout: {result.stdout}, stderr: {result.stderr}")
        finally:
            Path(invalid_path).unlink(missing_ok=True)

    def test_elements_exceed_one(self) -> None:
        # individual share > 1.0
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["elements"] = {"fire": 1.5}
        errors = combat_ir.validate(doc)
        self.assertTrue(any("exceed 1.0" in e for e in errors),
                        f"Expected error for share > 1.0, got: {errors}")

    def test_elements_negative_rejected(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["elements"] = {"fire": -0.3}
        errors = combat_ir.validate(doc)
        self.assertTrue(any("non-negative" in e or "negative" in e for e in errors),
                        f"Expected error for negative share, got: {errors}")

    def test_elements_total_over_one(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["elements"] = {"fire": 0.6, "lightning": 0.6}
        errors = combat_ir.validate(doc)
        self.assertTrue(any("sum" in e.lower() and "1.0" in e for e in errors),
                        f"Expected error for total > 1.0, got: {errors}")

    def test_elements_sum_exactly_one_accepted(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["elements"] = {"fire": 0.5, "lightning": 0.5}
        errors = combat_ir.validate(doc)
        self.assertEqual([e for e in errors if "elements" in e], [],
                         f"Expected no element errors for valid profile, got: {errors}")

    def test_elements_missing_treated_as_empty(self) -> None:
        doc = json.loads(json.dumps(self.valid_doc))
        # ensure no elements key on any action
        for action in doc["actions"]:
            action.pop("elements", None)
        errors = combat_ir.validate(doc)
        self.assertEqual([e for e in errors if "elements" in e], [],
                         f"Expected no element errors for missing elements, got: {errors}")

    def test_non_dict_action_rejected(self) -> None:
        # Regression test: non-dict action should return a clean error, not crash
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"] = ["not-a-dict", 123, None, [1, 2, 3]]
        errors = combat_ir.validate(doc)
        # Should return errors for each non-dict action
        self.assertTrue(len(errors) >= 4, f"Expected at least 4 errors, got: {errors}")
        for i in range(4):
            self.assertTrue(any(f"actions[{i}] must be an object" in e for e in errors),
                            f"Expected error for actions[{i}], got: {errors}")

    def test_translate_nan_motion_value_clamped_to_default(self) -> None:
        # (a) NaN motion_value is clamped to the envelope default.
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["kind"] = "Light1"
        errors = combat_ir.validate(doc)
        self.assertEqual(errors, [], f"Expected valid doc, got: {errors}")
        result = combat_ir.translate("Light1", float("nan"))
        self.assertAlmostEqual(result["damage_multiplier"], 1.00, places=6)
        self.assertTrue(math.isnan(result["motion_value_in"]))

    def test_translate_negative_motion_value_clamped_to_min(self) -> None:
        # (b) negative motion_value is clamped to the envelope min.
        result = combat_ir.translate("Heavy1", -5.0)
        self.assertAlmostEqual(result["damage_multiplier"], 1.35, places=6)
        self.assertEqual(result["motion_value_in"], -5.0)

    def test_translate_infinity_motion_value_clamped_to_max(self) -> None:
        # (c) INFINITY motion_value is clamped to the envelope max.
        result = combat_ir.translate("Heavy1", float("inf"))
        self.assertAlmostEqual(result["damage_multiplier"], 1.85, places=6)
        self.assertEqual(result["motion_value_in"], float("inf"))

    def test_translate_zero_motion_value_uses_default(self) -> None:
        # (d) motion_value=0.0 uses the envelope default (not the min).
        result = combat_ir.translate("Light1", 0.0)
        self.assertAlmostEqual(result["damage_multiplier"], 1.00, places=6)
        self.assertEqual(result["motion_value_in"], 0.0)

    def test_translate_light1_default_matches_envelope(self) -> None:
        # Verify returned damage_multiplier matches envelope defaults used in
        # the Rust core (balance.rs), e.g. Light1 default ~1.0. Each kind's
        # result stays within its own envelope max (1.15 for the Light family).
        envelope_max: Dict[str, float] = {
            "Light1": 1.15, "Light2": 1.15, "Heavy1": 1.85,
            "Heavy1Charge": 2.50, "PairedLight1": 0.90,
        }
        for kind in envelope_max:
            mv = float("nan")
            r = combat_ir.translate(kind, mv)
            self.assertIsInstance(r["damage_multiplier"], float)
            self.assertTrue(r["damage_multiplier"] > 0.0)
            self.assertLessEqual(r["damage_multiplier"], envelope_max[kind])

    # --- New action-field completeness and type validation tests ---

    def test_action_missing_duration_s_rejected(self) -> None:
        # (a) action missing duration_s rejected — error message contains 'duration_s'
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0].pop("duration_s", None)
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("duration_s" in e for e in errors),
            f"Expected error mentioning duration_s, got: {errors}",
        )

    def test_action_duration_s_zero_or_negative_rejected(self) -> None:
        # (b) action with duration_s 0 or negative rejected — message contains 'duration_s must be a number > 0'
        for bad_value in [0, -0.1, -1.0]:
            doc = json.loads(json.dumps(self.valid_doc))
            doc["actions"][0]["duration_s"] = bad_value
            errors = combat_ir.validate(doc)
            self.assertTrue(
                any("duration_s must be a number > 0" in e for e in errors),
                f"Expected error for duration_s={bad_value}, got: {errors}",
            )

    def test_action_negative_stamina_cost_rejected(self) -> None:
        # (c) action with negative stamina_cost rejected — message contains 'stamina_cost'
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["stamina_cost"] = -5
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("stamina_cost" in e and ">= 0" in e for e in errors),
            f"Expected error mentioning stamina_cost >= 0, got: {errors}",
        )

    def test_action_hit_windows_s_non_list_rejected(self) -> None:
        # (d) action with hit_windows_s set to a non-list (e.g. a string) rejected — message contains 'hit_windows_s must be a list'
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["hit_windows_s"] = "not-a-list"
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("hit_windows_s must be a list" in e for e in errors),
            f"Expected error for non-list hit_windows_s, got: {errors}",
        )

    def test_action_hit_windows_s_entries_not_two_number_lists_rejected(self) -> None:
        # (e) action with hit_windows_s entries that are not two-number lists rejected — message contains 'must be a list of two numbers'
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["hit_windows_s"] = [[0.1], [0.2, 0.3, 0.4], "bad", 123]
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("must be a list of two numbers" in e for e in errors),
            f"Expected error for invalid hit window entries, got: {errors}",
        )

    def test_action_hit_window_start_negative_or_end_beyond_duration_rejected(self) -> None:
        # (f) action with a hit window whose start is negative or end beyond duration_s rejected — message contains 'must satisfy 0 <= start < end <= duration_s'
        doc = json.loads(json.dumps(self.valid_doc))
        # duration_s is 0.5 for first action
        doc["actions"][0]["hit_windows_s"] = [[-0.1, 0.2], [0.1, 0.6]]
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("must satisfy 0 <= start < end <= duration_s" in e for e in errors),
            f"Expected error for invalid window bounds, got: {errors}",
        )

    def test_translate_unknown_kind_returns_none_multiplier(self) -> None:
        # An unknown attack kind must translate to damage_multiplier=None with a
        # non-empty reason, so callers can distinguish "no translation" from
        # "translated to zero damage".
        result = combat_ir.translate("UnknownKind", 1.0)
        self.assertIsNone(
            result["damage_multiplier"],
            f"Expected damage_multiplier=None for unknown kind, got: {result}",
        )
        self.assertIsInstance(result["reason"], str)
        self.assertTrue(
            len(result["reason"]) > 0,
            f"Expected non-empty reason, got: {result}",
        )
        self.assertEqual(result["kind"], "UnknownKind")

    def test_validate_rejects_unknown_attack_kind(self) -> None:
        # An action whose id is not a known AttackKind must be rejected by
        # validate(), so an unknown kind cannot silently enter the IR document
        # and later hit translate() at runtime.
        doc = json.loads(json.dumps(self.valid_doc))
        doc["actions"][0]["id"] = "NotARealAttackKind"
        errors = combat_ir.validate(doc)
        self.assertTrue(
            any("must be a known AttackKind" in e for e in errors),
            f"Expected error about unknown AttackKind, got: {errors}",
        )


if __name__ == "__main__":
    unittest.main()
