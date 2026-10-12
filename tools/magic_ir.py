#!/usr/bin/env python3
"""Catalyst and spell records for the combat IR (additive to v1).

A document may carry optional top-level `catalysts` and `spells` arrays. They enter the
system through the same validator as actions, so the Charter's rules are enforced at the
boundary rather than trusted downstream:

- spell power is a ratio of the casting catalyst's quality, never a damage number;
- a catalyst has a quality and at most one passive, and nothing else that adds power;
- no attribute, skill, level, perk or stat requirement exists anywhere (no Intelligence,
  no Faith): a level-one character may use every catalyst and spell its family allows;
- numeric bounds mirror core/src/magic.rs, and tests/test_magic_ir.py compares them with
  the Rust constants so the two cannot drift.

Records use `quality` and `power_ratio` deliberately: the IR forbids the keys `scaling` and
`damage` so source-game absolute values cannot enter by name.
"""

from __future__ import annotations

import math
from typing import Any, Dict, List

# Mirrors core/src/magic.rs. A test parses the Rust file and fails if these differ.
MIN_QUALITY = 0.5
MAX_QUALITY = 1.5
MAX_UPGRADE_LEVEL = 10
MAX_PASSIVE_BONUS = 0.5
MIN_COST_MULTIPLIER = 1.0
MAX_COST_MULTIPLIER = 2.0
MAX_POWER_RATIO = 1.5
MAX_CHARGE_POWER = 2.0
MAX_SPELL_MODIFIERS = 4

SCHOOLS = {"destruction", "alteration", "illusion", "restoration", "conjuration"}
CATALYST_KINDS = {"staff", "ring"}
DELIVERIES = {"instant", "projectile", "area", "self"}
ELEMENTS = {"physical", "magic", "fire", "lightning", "holy"}
STATUSES = {"bleed", "frostbite", "poison", "rot", "sleep", "madness"}
STATS = {
    "damage_dealt", "damage_taken", "stamina_cost", "magicka_cost", "cast_speed",
    "movement_speed", "poise", "stance_damage", "status_buildup_dealt", "status_buildup_taken",
}
OPERATIONS = {"add", "multiply", "override"}
STACK_RULES = {"replace", "refresh", "strongest", "by_source"}
MODIFIER_TARGETS = {"caster", "target"}

# Anything that would make a catalyst or spell unavailable to a character, or import a source
# game's stat model. The Charter allows none of them.
FORBIDDEN_KEYS = {
    "requires", "requirement", "requirements", "required", "prerequisite", "prerequisites",
    "skill_required", "required_skill", "min_skill", "minimum_skill", "skill_level",
    "level_required", "required_level", "min_level", "minimum_level",
    "perk_required", "required_perk", "requires_perk", "attribute_required",
    "intelligence", "faith", "arcane", "strength", "dexterity", "vigor", "mind", "endurance",
}


def _number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def _check_forbidden(obj: Any, path: str, errors: List[str]) -> None:
    if isinstance(obj, dict):
        for key, value in obj.items():
            if isinstance(key, str) and key.lower() in FORBIDDEN_KEYS:
                errors.append(f"{path}.{key}: requirements and source-game stats are forbidden")
            _check_forbidden(value, f"{path}.{key}", errors)
    elif isinstance(obj, list):
        for index, item in enumerate(obj):
            _check_forbidden(item, f"{path}[{index}]", errors)


def _ranged(errors: List[str], where: str, value: Any, low: float, high: float) -> bool:
    if not _number(value):
        errors.append(f"{where} must be a finite number")
        return False
    if not (low <= value <= high):
        errors.append(f"{where} must be within [{low}, {high}] (got {value})")
        return False
    return True


def _validate_catalyst(index: int, record: Any, errors: List[str]) -> None:
    path = f"catalysts[{index}]"
    if not isinstance(record, dict):
        errors.append(f"{path} must be an object")
        return
    allowed = {"id", "kind", "school", "quality", "upgrade_level", "cost_multiplier", "passive"}
    for key in record:
        if key not in allowed and key.lower() not in FORBIDDEN_KEYS:
            errors.append(f"{path}.{key} is not a catalyst property: exactly quality and at most one passive add power")
    if not isinstance(record.get("id"), str) or not record.get("id"):
        errors.append(f"{path}.id must be a non-empty string")
    if record.get("kind") not in CATALYST_KINDS:
        errors.append(f"{path}.kind must be one of {sorted(CATALYST_KINDS)}")
    if record.get("school") not in SCHOOLS:
        errors.append(f"{path}.school must be one of {sorted(SCHOOLS)}")
    _ranged(errors, f"{path}.quality", record.get("quality"), MIN_QUALITY, MAX_QUALITY)
    level = record.get("upgrade_level", 0)
    if isinstance(level, bool) or not isinstance(level, int) or not (0 <= level <= MAX_UPGRADE_LEVEL):
        errors.append(f"{path}.upgrade_level must be an integer within [0, {MAX_UPGRADE_LEVEL}]")
    _ranged(errors, f"{path}.cost_multiplier", record.get("cost_multiplier", 1.0), MIN_COST_MULTIPLIER, MAX_COST_MULTIPLIER)
    passive = record.get("passive")
    if passive is not None:
        if not isinstance(passive, dict):
            errors.append(f"{path}.passive must be an object or null (at most one passive)")
        else:
            for key in passive:
                if key not in {"family", "bonus"}:
                    errors.append(f"{path}.passive.{key} is not allowed: a passive is one family and one percentage")
            if not isinstance(passive.get("family"), str) or not passive.get("family"):
                errors.append(f"{path}.passive.family must be a non-empty string")
            bonus = passive.get("bonus")
            if _ranged(errors, f"{path}.passive.bonus", bonus, 0.0, MAX_PASSIVE_BONUS) and bonus == 0:
                errors.append(f"{path}.passive.bonus must be greater than 0 (use null for no passive)")


def _validate_modifier(path: str, record: Any, errors: List[str]) -> str | None:
    if not isinstance(record, dict):
        errors.append(f"{path} must be an object")
        return None
    if record.get("on") not in MODIFIER_TARGETS:
        errors.append(f"{path}.on must be one of {sorted(MODIFIER_TARGETS)}")
    if record.get("stat") not in STATS:
        errors.append(f"{path}.stat must be one of {sorted(STATS)}")
    operation = record.get("operation")
    if operation not in OPERATIONS:
        errors.append(f"{path}.operation must be one of {sorted(OPERATIONS)}")
    magnitude = record.get("magnitude")
    if not _number(magnitude):
        errors.append(f"{path}.magnitude must be a finite number")
    elif operation == "multiply" and magnitude < 0:
        errors.append(f"{path}.magnitude must not be negative for a multiply")
    duration = record.get("duration_s")
    if not _number(duration) or duration <= 0:
        errors.append(f"{path}.duration_s must be a number > 0")
    if record.get("stack_rule") not in STACK_RULES:
        errors.append(f"{path}.stack_rule must be one of {sorted(STACK_RULES)}; stacking is data, never an accident")
    for key in ("group", "priority"):
        value = record.get(key, 0)
        if isinstance(value, bool) or not isinstance(value, int) or (key == "group" and value < 0):
            errors.append(f"{path}.{key} must be an integer" + (" >= 0" if key == "group" else ""))
    return record.get("on") if record.get("on") in MODIFIER_TARGETS else None


def _validate_payload(path: str, payload: Any, errors: List[str]) -> Dict[str, Any]:
    facts = {"damages": False, "status": False, "caster": False, "target": False, "empty": True}
    if not isinstance(payload, dict):
        errors.append(f"{path} must be an object")
        return facts
    elements = payload.get("elements", {})
    if not isinstance(elements, dict):
        errors.append(f"{path}.elements must be an object")
    else:
        total = 0.0
        for name, share in elements.items():
            if name not in ELEMENTS:
                errors.append(f'{path}.elements has unknown element "{name}"')
            if not _number(share) or share < 0 or share > 1:
                errors.append(f'{path}.elements["{name}"] must be a share within [0, 1]')
            elif name in ELEMENTS:
                total += share
        if total > 1.0 + 1e-6:
            errors.append(f"{path}.elements shares must sum to at most 1.0 (got {total})")
        facts["damages"] = total > 0
    for key in ("poise_pressure", "stance_pressure"):
        value = payload.get(key, 0)
        if not _number(value) or value < 0:
            errors.append(f"{path}.{key} must be a number >= 0")
    buildup = payload.get("status_buildup", {})
    if not isinstance(buildup, dict):
        errors.append(f"{path}.status_buildup must be an object")
    else:
        for name, amount in buildup.items():
            if name not in STATUSES:
                errors.append(f'{path}.status_buildup has unknown status "{name}"')
            if not _number(amount) or amount < 0:
                errors.append(f'{path}.status_buildup["{name}"] must be a number >= 0')
            elif amount > 0:
                facts["status"] = True
    modifiers = payload.get("modifiers", [])
    if not isinstance(modifiers, list):
        errors.append(f"{path}.modifiers must be a list")
    else:
        if len(modifiers) > MAX_SPELL_MODIFIERS:
            errors.append(f"{path}.modifiers may hold at most {MAX_SPELL_MODIFIERS}: spells are shapes, not buff piles")
        for index, modifier in enumerate(modifiers):
            side = _validate_modifier(f"{path}.modifiers[{index}]", modifier, errors)
            if side:
                facts[side] = True
    facts["empty"] = not (facts["damages"] or facts["status"] or modifiers)
    return facts


def _validate_spell(index: int, record: Any, errors: List[str]) -> None:
    path = f"spells[{index}]"
    if not isinstance(record, dict):
        errors.append(f"{path} must be an object")
        return
    allowed = {"id", "school", "family", "delivery", "castable_from", "power_ratio", "cost_fraction",
               "timing_s", "charge", "area", "payload"}
    for key in record:
        if key not in allowed and key.lower() not in FORBIDDEN_KEYS:
            errors.append(f"{path}.{key} is not a spell property")
    for key in ("id", "family"):
        if not isinstance(record.get(key), str) or not record.get(key):
            errors.append(f"{path}.{key} must be a non-empty string")
    if record.get("school") not in SCHOOLS:
        errors.append(f"{path}.school must be one of {sorted(SCHOOLS)}")
    delivery = record.get("delivery")
    if delivery not in DELIVERIES:
        errors.append(f"{path}.delivery must be one of {sorted(DELIVERIES)}")
    castable = record.get("castable_from")
    if (not isinstance(castable, list) or not castable or len(set(castable)) != len(castable)
            or any(kind not in CATALYST_KINDS for kind in castable)):
        errors.append(f"{path}.castable_from must list each catalyst family it can be cast from (staff, ring)")
    _ranged(errors, f"{path}.power_ratio", record.get("power_ratio"), 0.0, MAX_POWER_RATIO)
    _ranged(errors, f"{path}.cost_fraction", record.get("cost_fraction"), 0.0, 1.0)

    timing = record.get("timing_s")
    if not isinstance(timing, dict):
        errors.append(f"{path}.timing_s must be an object")
    else:
        names = ("release_at", "cancel_until", "recovery")
        values = {name: timing.get(name) for name in names}
        if not all(_number(v) and v >= 0 for v in values.values()):
            errors.append(f"{path}.timing_s release_at, cancel_until and recovery must be numbers >= 0")
        else:
            if values["release_at"] <= 0:
                errors.append(f"{path}.timing_s.release_at must be > 0")
            if values["cancel_until"] > values["release_at"]:
                errors.append(f"{path}.timing_s.cancel_until must not be after release_at: the cancel window closes before the release")
            armour = timing.get("hyperarmour")
            if (not isinstance(armour, list) or len(armour) != 2 or not all(_number(v) and v >= 0 for v in armour)
                    or armour[0] > armour[1] or armour[1] > values["release_at"] + values["recovery"]):
                errors.append(f"{path}.timing_s.hyperarmour must be [from, to] within the cast")

    charge = record.get("charge")
    if charge is not None:
        if (not isinstance(charge, dict) or not _number(charge.get("max_s")) or charge.get("max_s") <= 0
                or not _number(charge.get("power_at_full"))
                or not (1.0 <= charge.get("power_at_full") <= MAX_CHARGE_POWER)):
            errors.append(f"{path}.charge needs max_s > 0 and power_at_full within [1, {MAX_CHARGE_POWER}]")
    area = record.get("area")
    if delivery == "area":
        if (not isinstance(area, dict) or not all(_number(area.get(k)) and area.get(k) > 0
                                                   for k in ("radius_m", "duration_s", "tick_interval_s"))
                or area.get("tick_interval_s") > area.get("duration_s")):
            errors.append(f"{path}.area needs radius_m, duration_s and tick_interval_s > 0, with a tick no longer than the area")
    elif area is not None:
        errors.append(f"{path}.area is only valid for an area delivery")

    facts = _validate_payload(f"{path}.payload", record.get("payload"), errors)
    if facts["empty"]:
        errors.append(f"{path}.payload does nothing: it needs damage, a status buildup or a modifier")
    elif delivery == "self":
        if facts["damages"] or facts["status"] or facts["target"]:
            errors.append(f"{path}: a self-cast cannot hit targets; it may only carry caster modifiers")
        elif not facts["caster"]:
            errors.append(f"{path}: a self-cast needs a caster modifier")
    elif delivery in DELIVERIES and not (facts["damages"] or facts["status"] or facts["target"]):
        errors.append(f"{path}: a targeted spell needs damage, a status buildup or a target modifier")


def validate_magic(doc: Dict[str, Any]) -> List[str]:
    """Errors for the optional `catalysts` and `spells` arrays; empty when absent or valid."""
    errors: List[str] = []
    for name in ("catalysts", "spells"):
        if name in doc and not isinstance(doc[name], list):
            errors.append(f"{name} must be a list")
    catalysts = doc.get("catalysts") if isinstance(doc.get("catalysts"), list) else []
    spells = doc.get("spells") if isinstance(doc.get("spells"), list) else []
    for name, records in (("catalysts", catalysts), ("spells", spells)):
        seen = set()
        for index, record in enumerate(records):
            identifier = record.get("id") if isinstance(record, dict) else None
            if isinstance(identifier, str):
                if identifier in seen:
                    errors.append(f'{name}[{index}].id "{identifier}" must be unique')
                seen.add(identifier)
    for index, record in enumerate(catalysts):
        _validate_catalyst(index, record, errors)
    for index, record in enumerate(spells):
        _validate_spell(index, record, errors)
    for name, records in (("catalysts", catalysts), ("spells", spells)):
        _check_forbidden(records, name, errors)
    return errors
