#!/usr/bin/env python3
"""
Tests for combat_ir.py
"""

from __future__ import annotations

import json
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
        doc["provenance"]["data_fingerprint"] = "a1b2c3d4e5f6"
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


if __name__ == "__main__":
    unittest.main()
