from __future__ import annotations

import copy
import json
from pathlib import Path
import re
import subprocess
import sys
import unittest

TOOLS = Path(__file__).resolve().parents[1]
ROOT = TOOLS.parent
sys.path.insert(0, str(TOOLS))
import combat_ir  # noqa: E402
import magic_ir  # noqa: E402

FIXTURE = ROOT / "fixtures" / "ir" / "v1" / "magic_matrix.json"


def load() -> dict:
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


class MagicIrTests(unittest.TestCase):
    def errors(self, mutate) -> list[str]:
        doc = load()
        mutate(doc)
        return combat_ir.validate(doc)

    def assertRejects(self, mutate, fragment: str) -> None:
        errors = self.errors(mutate)
        self.assertTrue(any(fragment in error for error in errors), f"{fragment!r} not in {errors}")

    def test_the_fixture_is_valid_and_covers_every_delivery(self) -> None:
        doc = load()
        self.assertEqual(combat_ir.validate(doc), [])
        self.assertEqual({spell["delivery"] for spell in doc["spells"]}, magic_ir.DELIVERIES)
        self.assertEqual(len(doc["spells"]), 7, "the seven cases of the first magic slice")
        self.assertTrue(any(spell["charge"] for spell in doc["spells"]))
        self.assertEqual({c["kind"] for c in doc["catalysts"]}, {"staff", "ring"})
        # Destruction straddles both families, with different spells on each.
        families = {kind for spell in doc["spells"] if spell["school"] == "destruction" for kind in spell["castable_from"]}
        self.assertEqual(families, {"staff", "ring"})

    def test_documents_without_magic_are_unaffected(self) -> None:
        doc = load()
        del doc["catalysts"], doc["spells"]
        self.assertEqual(combat_ir.validate(doc), [])

    def test_the_cli_validates_the_fixture(self) -> None:
        done = subprocess.run([sys.executable, str(TOOLS / "combat_ir.py"), "validate", str(FIXTURE)],
                              capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)

    def test_python_bounds_equal_the_rust_constants(self) -> None:
        rust = (ROOT / "core" / "src" / "magic.rs").read_text(encoding="utf-8")

        def constant(name: str) -> float:
            match = re.search(rf"pub const {name}: (?:f32|u8|usize) = ([0-9.]+);", rust)
            self.assertIsNotNone(match, f"{name} not found in core/src/magic.rs")
            return float(match.group(1))

        for name in ("MIN_QUALITY", "MAX_QUALITY", "MAX_UPGRADE_LEVEL", "MAX_PASSIVE_BONUS", "MIN_COST_MULTIPLIER",
                     "MAX_COST_MULTIPLIER", "MAX_POWER_RATIO", "MAX_CHARGE_POWER", "MAX_SPELL_MODIFIERS"):
            self.assertEqual(getattr(magic_ir, name), constant(name), name)

    def test_catalyst_rules(self) -> None:
        cases = [
            (lambda d: d["catalysts"][0].update(quality=0.4), "quality must be within"),
            (lambda d: d["catalysts"][0].update(quality=1.6), "quality must be within"),
            (lambda d: d["catalysts"][0].update(quality=float("nan")), "quality must be a finite number"),
            (lambda d: d["catalysts"][0].update(quality="high"), "quality must be a finite number"),
            (lambda d: d["catalysts"][0].update(upgrade_level=11), "upgrade_level"),
            (lambda d: d["catalysts"][0].update(upgrade_level=1.5), "upgrade_level"),
            (lambda d: d["catalysts"][0].update(cost_multiplier=0.8), "cost_multiplier must be within"),
            (lambda d: d["catalysts"][0].update(kind="wand"), "kind must be one of"),
            (lambda d: d["catalysts"][0].update(school="necromancy"), "school must be one of"),
            (lambda d: d["catalysts"][0].update(id=""), "id must be a non-empty string"),
            (lambda d: d["catalysts"][2]["passive"].update(bonus=0.6), "bonus must be within"),
            (lambda d: d["catalysts"][2]["passive"].update(bonus=0.0), "greater than 0"),
            (lambda d: d["catalysts"][2]["passive"].update(family=""), "family must be a non-empty string"),
            (lambda d: d["catalysts"][2].update(passive=[{"family": "a", "bonus": 0.1}]), "at most one passive"),
            (lambda d: d["catalysts"][2]["passive"].update(second_family="x"), "a passive is one family and one percentage"),
            (lambda d: d["catalysts"][0].update(extra_power=1.2), "exactly quality and at most one passive"),
            (lambda d: d["catalysts"].append(copy.deepcopy(d["catalysts"][0])), "must be unique"),
            (lambda d: d.update(catalysts={"a": 1}), "catalysts must be a list"),
            (lambda d: d["catalysts"].append("a ring"), "must be an object"),
        ]
        for mutate, fragment in cases:
            with self.subTest(fragment=fragment):
                self.assertRejects(mutate, fragment)

    def test_spell_rules(self) -> None:
        cases = [
            (lambda d: d["spells"][0].update(power_ratio=1.6), "power_ratio must be within"),
            (lambda d: d["spells"][0].update(power_ratio=-0.1), "power_ratio must be within"),
            (lambda d: d["spells"][0].update(power_ratio=float("inf")), "power_ratio must be a finite number"),
            (lambda d: d["spells"][0].update(cost_fraction=1.2), "cost_fraction must be within"),
            (lambda d: d["spells"][0].update(castable_from=[]), "castable_from must list"),
            (lambda d: d["spells"][0].update(castable_from=["staff", "staff"]), "castable_from must list"),
            (lambda d: d["spells"][0].update(castable_from=["wand"]), "castable_from must list"),
            (lambda d: d["spells"][0].update(delivery="beam"), "delivery must be one of"),
            (lambda d: d["spells"][0].update(school="smithing"), "school must be one of"),
            (lambda d: d["spells"][0]["timing_s"].update(release_at=0), "release_at must be > 0"),
            (lambda d: d["spells"][0]["timing_s"].update(cancel_until=0.9), "cancel window closes before the release"),
            (lambda d: d["spells"][0]["timing_s"].update(hyperarmour=[0.6, 0.3]), "hyperarmour must be"),
            (lambda d: d["spells"][0]["timing_s"].update(hyperarmour=[0.1, 9.0]), "hyperarmour must be"),
            (lambda d: d["spells"][0]["timing_s"].update(recovery=-1), "numbers >= 0"),
            (lambda d: d["spells"][6]["charge"].update(power_at_full=0.5), "charge needs"),
            (lambda d: d["spells"][6]["charge"].update(power_at_full=2.5), "charge needs"),
            (lambda d: d["spells"][6]["charge"].update(max_s=0), "charge needs"),
            (lambda d: d["spells"][0].update(area={"radius_m": 1, "duration_s": 1, "tick_interval_s": 1}), "only valid for an area"),
            (lambda d: d["spells"][2].update(area=None), "area needs"),
            (lambda d: d["spells"][2]["area"].update(tick_interval_s=5.0), "tick no longer than the area"),
            (lambda d: d["spells"][0]["payload"].update(elements={"fire": 0.7, "holy": 0.6}), "sum to at most 1.0"),
            (lambda d: d["spells"][0]["payload"].update(elements={"void": 0.5}), "unknown element"),
            (lambda d: d["spells"][0]["payload"].update(poise_pressure=-1), "poise_pressure"),
            (lambda d: d["spells"][5]["payload"]["status_buildup"].update(rust=5), "unknown status"),
            (lambda d: d["spells"][5]["payload"]["status_buildup"].update(frostbite=-5), "status_buildup"),
            (lambda d: d["spells"][0]["payload"].update(elements={}), "payload does nothing"),
            (lambda d: d["spells"][3]["payload"]["modifiers"].append(copy.deepcopy(d["spells"][3]["payload"]["modifiers"][0])) or d["spells"][3]["payload"]["modifiers"].extend(copy.deepcopy(d["spells"][3]["payload"]["modifiers"]) * 4), "at most 4"),
            (lambda d: d["spells"][3]["payload"].update(elements={"fire": 1.0}), "a self-cast cannot hit targets"),
            (lambda d: d["spells"][3]["payload"]["modifiers"][0].update(on="target"), "a self-cast cannot hit targets"),
            (lambda d: d["spells"][4]["payload"]["modifiers"][0].update(on="caster"), "a targeted spell needs"),
            (lambda d: d["spells"][4]["payload"]["modifiers"][0].update(stat="health"), "stat must be one of"),
            (lambda d: d["spells"][4]["payload"]["modifiers"][0].update(operation="set"), "operation must be one of"),
            (lambda d: d["spells"][4]["payload"]["modifiers"][0].update(magnitude=-1.0), "must not be negative for a multiply"),
            (lambda d: d["spells"][4]["payload"]["modifiers"][0].update(duration_s=0), "duration_s must be a number > 0"),
            (lambda d: d["spells"][4]["payload"]["modifiers"][0].pop("stack_rule"), "stack_rule must be one of"),
            (lambda d: d["spells"][0].update(unexpected=1), "is not a spell property"),
            (lambda d: d["spells"].append(copy.deepcopy(d["spells"][0])), "must be unique"),
        ]
        for mutate, fragment in cases:
            with self.subTest(fragment=fragment):
                self.assertRejects(mutate, fragment)

    def test_no_requirement_of_any_kind_is_accepted_at_any_depth(self) -> None:
        keys = ("requires", "min_level", "skill_required", "perk_required", "intelligence", "faith", "strength",
                "required_skill", "minimum_level", "attribute_required")
        for key in keys:
            with self.subTest(key=key):
                self.assertRejects(lambda d: d["catalysts"][0].update({key: 10}), "requirements and source-game stats are forbidden")
                self.assertRejects(lambda d: d["spells"][0].update({key: 10}), "requirements and source-game stats are forbidden")
                self.assertRejects(lambda d: d["spells"][0]["payload"].update({key: 10}), "requirements and source-game stats are forbidden")
                self.assertRejects(lambda d: d["catalysts"][2]["passive"].update({key: 10}), "requirements and source-game stats are forbidden")

    def test_absolute_damage_and_scaling_keys_are_still_rejected_inside_magic(self) -> None:
        self.assertRejects(lambda d: d["spells"][0].update(damage=300), "forbidden key 'damage'")
        self.assertRejects(lambda d: d["catalysts"][0].update(scaling=413), "forbidden key 'scaling'")

    def test_every_error_is_a_string_and_malformed_documents_do_not_crash(self) -> None:
        for bad in (None, 5, "x", [], {"catalysts": None, "spells": None}, {"spells": [None, 1, "s", [], {}]}):
            doc = load()
            if isinstance(bad, dict):
                doc.update(bad)
            elif bad is not None:
                doc["spells"] = bad
            errors = combat_ir.validate(doc)
            self.assertTrue(all(isinstance(e, str) for e in errors), bad)
        self.assertEqual(magic_ir.validate_magic({}), [])


if __name__ == "__main__":
    unittest.main()
