#!/usr/bin/env python3
"""
Combat IR v1 validator.

Usage:
    python3 tools/combat_ir.py validate FILE

The validate function returns a list of error strings; an empty list means valid.
"""

import enum
import json
import math
import sys
from typing import Any, Dict, List


class AttackKind(str, enum.Enum):
    """Attack kinds used by the bridge. Mirrors `AttackKind` in core/src/data.rs /
    role() and the envelope table in core/src/balance.rs."""

    # Light attacks
    Light1 = "Light1"
    Light2 = "Light2"
    Light3 = "Light3"
    Light4 = "Light4"
    Light5 = "Light5"
    Light6 = "Light6"
    LeftLight1 = "LeftLight1"
    LeftLight2 = "LeftLight2"
    LeftLight3 = "LeftLight3"
    LeftLight4 = "LeftLight4"
    LeftLight5 = "LeftLight5"
    LeftLight6 = "LeftLight6"
    # Heavy attacks
    Heavy1 = "Heavy1"
    Heavy2 = "Heavy2"
    RunHeavy = "RunHeavy"
    Heavy1Charge = "Heavy1Charge"
    Heavy2Charge = "Heavy2Charge"
    # Special actions
    CrouchAttack = "CrouchAttack"
    RunLight = "RunLight"
    RollAttack = "RollAttack"
    BackstepAttack = "BackstepAttack"
    JumpLightLand = "JumpLightLand"
    JumpLightLandShort = "JumpLightLandShort"
    JumpHeavyLand = "JumpHeavyLand"
    JumpHeavyLandShort = "JumpHeavyLandShort"
    GuardCounter = "GuardCounter"
    # Paired attacks
    PairedLight1 = "PairedLight1"
    PairedLight2 = "PairedLight2"
    PairedLight3 = "PairedLight3"
    PairedLight4 = "PairedLight4"
    PairedLight5 = "PairedLight5"
    PairedLight6 = "PairedLight6"
    PairedRun = "PairedRun"
    PairedRoll = "PairedRoll"
    PairedBackstep = "PairedBackstep"
    PairedJumpLand = "PairedJumpLand"
    PairedJumpLandShort = "PairedJumpLandShort"


def validate(doc: Dict[str, Any]) -> List[str]:
    """Validate a Combat IR v1 document.

    Args:
        doc: The parsed JSON document as a dictionary.

    Returns:
        A list of error messages. Empty list means valid.
    """
    errors: List[str] = []

    # Top-level structure
    if not isinstance(doc, dict):
        return ["Document must be a JSON object"]

    # ir_version
    if doc.get("ir_version") != 1:
        errors.append("ir_version must be 1")

    # provenance
    provenance = doc.get("provenance")
    if not isinstance(provenance, dict):
        errors.append("provenance must be an object")
    else:
        source = provenance.get("source")
        if source not in ("synthetic", "owner"):
            errors.append('provenance.source must be either "synthetic" or "owner"')
        generator = provenance.get("generator")
        if not isinstance(generator, str) or not generator:
            errors.append('provenance.generator must be a non-empty string')
        data_fingerprint = provenance.get("data_fingerprint")
        if source == "owner":
            if not isinstance(data_fingerprint, str) or len(data_fingerprint) == 0:
                errors.append(
                    'provenance.data_fingerprint must be a non-null, non-empty string when source is "owner"'
                )
            else:
                # Validate it's a 64-character lowercase SHA-256 hex digest,
                # consistent with evidence.py.
                if len(data_fingerprint) != 64:
                    errors.append(
                        'provenance.data_fingerprint must be a 64-character lowercase hex string when source is "owner"'
                    )
                elif not all(c in "0123456789abcdef" for c in data_fingerprint):
                    errors.append(
                        'provenance.data_fingerprint must be a 64-character lowercase hex string when source is "owner"'
                    )
        # Note: data_fingerprint can be null (None) for synthetic, or a 64-char
        # lowercase hex string for owner (enforced above).

    # units
    units = doc.get("units")
    if not isinstance(units, dict):
        errors.append("units must be an object")
    else:
        expected = {"time": "s", "distance": "m", "angle": "deg"}
        for key, value in expected.items():
            if units.get(key) != value:
                errors.append(f'units.{key} must be "{value}"')
        # Ensure no extra keys? The spec doesn't forbid extra, but we can ignore.

    # actions
    actions = doc.get("actions")
    if not isinstance(actions, list):
        errors.append("actions must be a list")
    else:
        seen_ids = set()
        for i, action in enumerate(actions):

            # Type guard: action must be a dict before accessing its keys
            if not isinstance(action, dict):
                errors.append(f"actions[{i}] must be an object")
                continue

            # elements (ElementalProfile shares)
            elements = action.get("elements")
            if elements is not None:
                if not isinstance(elements, dict):
                    errors.append(f'actions[{i}].elements must be an object')
                else:
                    valid_keys = {"physical", "magic", "fire", "lightning", "holy"}
                    total = 0.0
                    for el_name, share in elements.items():
                        if el_name not in valid_keys:
                            errors.append(
                                f'actions[{i}].elements has unknown element "{el_name}"'
                            )
                        if not isinstance(share, (int, float)) or isinstance(share, bool):
                            errors.append(
                                f'actions[{i}].elements["{el_name}"] must be a number'
                            )
                            continue
                        s = float(share)
                        if s != s:  # NaN
                            errors.append(
                                f'actions[{i}].elements["{el_name}"] must be a finite number'
                            )
                            continue
                        if s < 0.0:
                            errors.append(
                                f'actions[{i}].elements["{el_name}"] share must not be negative'
                            )
                        if s > 1.0:
                            errors.append(
                                f'actions[{i}].elements["{el_name}"] share must not exceed 1.0'
                            )
                        total += s
                    epsilon = 1e-6
                    if total > 1.0 + epsilon:
                        errors.append(
                            f'actions[{i}].elements shares must sum to at most 1.0 (got {total})'
                        )
            # missing elements: treated as empty (all-zero) profile — valid

            # id
            action_id = action.get("id")
            if not isinstance(action_id, str) or not action_id:
                errors.append(f'actions[{i}].id must be a non-empty string')
            elif action_id in seen_ids:
                errors.append(f'actions[{i}].id "{action_id}" must be unique')
            else:
                seen_ids.add(action_id)

            # duration_s
            duration = action.get("duration_s")
            if not isinstance(duration, (int, float)) or duration <= 0:
                errors.append(f'actions[{i}].duration_s must be a number > 0')
                duration = None  # Mark as invalid so window validation can skip bounds check

            # stamina_cost
            stamina = action.get("stamina_cost")
            if not isinstance(stamina, (int, float)) or stamina < 0:
                errors.append(f'actions[{i}].stamina_cost must be a number >= 0')

            # Helper to validate windows
            def validate_window(name: str, window_list: List[Any]) -> None:
                if not isinstance(window_list, list):
                    errors.append(f'actions[{i}].{name} must be a list')
                    return
                for j, window in enumerate(window_list):
                    if not (isinstance(window, list) and len(window) == 2):
                        errors.append(f'actions[{i}].{name}[{j}] must be a list of two numbers [start, end]')
                        continue
                    start, end = window
                    if not (isinstance(start, (int, float)) and isinstance(end, (int, float))):
                        errors.append(f'actions[{i}].{name}[{j}] start and end must be numbers')
                        continue
                    if duration is not None and not (0 <= start < end <= duration):
                        errors.append(
                            f'actions[{i}].{name}[{j}] must satisfy 0 <= start < end <= duration_s ({duration})'
                        )

            # iframes_s
            iframes = action.get("iframes_s", [])
            validate_window("iframes_s", iframes)

            # hit_windows_s
            hit_windows = action.get("hit_windows_s", [])
            validate_window("hit_windows_s", hit_windows)

            # cancel_windows_s
            cancel_windows = action.get("cancel_windows_s", [])
            validate_window("cancel_windows_s", cancel_windows)

            # root_motion_m
            root_motion = action.get("root_motion_m", [])
            if not isinstance(root_motion, list):
                errors.append(f'actions[{i}].root_motion_m must be a list')
            else:
                prev_time = -1e-9  # allow zero at start
                for k, entry in enumerate(root_motion):
                    if not (isinstance(entry, list) and len(entry) == 3):
                        errors.append(f'actions[{i}].root_motion_m[{k}] must be a list of three numbers [t_s, dx, dy]')
                        continue
                    t_s, dx, dy = entry
                    if not (isinstance(t_s, (int, float)) and isinstance(dx, (int, float)) and isinstance(dy, (int, float))):
                        errors.append(f'actions[{i}].root_motion_m[{k}] all elements must be numbers')
                        continue
                    if duration is not None and not (0 <= t_s <= duration):
                        errors.append(f'actions[{i}].root_motion_m[{k}] t_s must be within [0, duration_s]')
                    if t_s <= prev_time:
                        errors.append(f'actions[{i}].root_motion_m[{k}] t_s must be strictly increasing')
                    prev_time = t_s

    # Check for forbidden keys (absolute damage etc.) anywhere in the document
    forbidden_keywords = {
        "damage", "attack_rating", "ar", "hp", "health",
        "defense", "defence", "scaling"
    }

    def check_forbidden(obj: Any, path: str = "") -> None:
        if isinstance(obj, dict):
            for key, value in obj.items():
                if isinstance(key, str) and key.lower() in forbidden_keywords:
                    errors.append(f"Absolute damage model detected: forbidden key '{key}' at {path}.{key}")
                check_forbidden(value, f"{path}.{key}" if path else key)
        elif isinstance(obj, list):
            for idx, item in enumerate(obj):
                check_forbidden(item, f"{path}[{idx}]")

    check_forbidden(doc)

    return errors


class _Envelope:
    __slots__ = ("damage_min", "damage_default", "damage_max")

    def __init__(self, damage_min: float, damage_default: float, damage_max: float) -> None:
        self.damage_min = damage_min
        self.damage_default = damage_default
        self.damage_max = damage_max


# Damage envelope table. Mirrors `ActionRole.envelope()` in core/src/balance.rs
# so the Python translation and the Rust core disagree on nothing.
_ENVELOPES: Dict[AttackKind, _Envelope] = {
    AttackKind.Light1: _Envelope(0.90, 1.00, 1.15),
    AttackKind.Light2: _Envelope(0.90, 1.00, 1.15),
    AttackKind.Light3: _Envelope(0.90, 1.00, 1.15),
    AttackKind.Light4: _Envelope(0.90, 1.00, 1.15),
    AttackKind.Light5: _Envelope(0.90, 1.00, 1.15),
    AttackKind.Light6: _Envelope(0.90, 1.00, 1.15),
    AttackKind.LeftLight1: _Envelope(0.90, 1.00, 1.15),
    AttackKind.LeftLight2: _Envelope(0.90, 1.00, 1.15),
    AttackKind.LeftLight3: _Envelope(0.90, 1.00, 1.15),
    AttackKind.LeftLight4: _Envelope(0.90, 1.00, 1.15),
    AttackKind.LeftLight5: _Envelope(0.90, 1.00, 1.15),
    AttackKind.LeftLight6: _Envelope(0.90, 1.00, 1.15),
    AttackKind.Heavy1: _Envelope(1.35, 1.55, 1.85),
    AttackKind.Heavy2: _Envelope(1.35, 1.55, 1.85),
    AttackKind.RunHeavy: _Envelope(1.35, 1.55, 1.85),
    AttackKind.Heavy1Charge: _Envelope(1.80, 2.10, 2.50),
    AttackKind.Heavy2Charge: _Envelope(1.80, 2.10, 2.50),
    AttackKind.CrouchAttack: _Envelope(0.80, 0.90, 1.00),
    AttackKind.RunLight: _Envelope(1.00, 1.10, 1.25),
    AttackKind.RollAttack: _Envelope(0.90, 1.00, 1.10),
    AttackKind.BackstepAttack: _Envelope(0.90, 1.00, 1.10),
    AttackKind.JumpLightLand: _Envelope(1.15, 1.25, 1.45),
    AttackKind.JumpLightLandShort: _Envelope(1.15, 1.25, 1.45),
    AttackKind.JumpHeavyLand: _Envelope(1.65, 1.85, 2.20),
    AttackKind.JumpHeavyLandShort: _Envelope(1.65, 1.85, 2.20),
    AttackKind.GuardCounter: _Envelope(1.50, 1.75, 2.00),
    AttackKind.PairedLight1: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedLight2: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedLight3: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedLight4: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedLight5: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedLight6: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedRun: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedRoll: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedBackstep: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedJumpLand: _Envelope(0.65, 0.75, 0.90),
    AttackKind.PairedJumpLandShort: _Envelope(0.65, 0.75, 0.90),
}


def translate(kind: str, motion_value: Any) -> Dict[str, Any]:
    """Translate one Elden Ring source motion value into the bounded multiplier.

    This mirrors `balance::damage_multiplier` exactly: invalid, non-positive or
    non-finite source values are treated as envelope default; the result is then
    clamped into the role's damage envelope. The returned multiplier is always a
    finite float inside [damage_min, damage_max].
    """
    envelope = _ENVELOPES.get(AttackKind(kind))
    if envelope is None:
        return {
            "kind": kind,
            "motion_value_in": motion_value,
            "damage_multiplier": None,
            "reason": f"unknown attack kind {kind!r}; must be one of {sorted(_ENVELOPES.keys())}",
        }

    mv: float
    if isinstance(motion_value, str):
        try:
            mv = float(motion_value)
        except ValueError:
            mv = float("nan")
    elif isinstance(motion_value, bool):
        mv = float("nan")
    elif isinstance(motion_value, (int, float)):
        mv = float(motion_value)
    else:
        mv = float("nan")

    if math.isnan(mv):
        # NaN is non-finite and carries no magnitude; use the envelope default.
        source = envelope.damage_default
        reason = "non-finite (NaN) motion value; using envelope default"
    elif mv == 0.0:
        # A zero motion value carries no force; use the envelope default.
        source = envelope.damage_default
        reason = "zero motion value; using envelope default"
    elif mv < 0.0:
        # Negative motion value is clamped to the envelope min.
        source = envelope.damage_min
        reason = "negative motion value; clamping to envelope min"
    elif not math.isfinite(mv):
        # Positive infinity is clamped to the envelope max.
        source = envelope.damage_max
        reason = "infinite motion value; clamping to envelope max"
    else:
        source = mv
        reason = "normal motion value within envelope"

    result = max(envelope.damage_min, min(envelope.damage_max, source))
    return {
        "kind": kind,
        "motion_value_in": motion_value,
        "damage_multiplier": result,
        "reason": reason,
    }


def _cli() -> None:
    import argparse
    parser = argparse.ArgumentParser(description="Validate Combat IR v1 document")
    parser.add_argument("command", choices=["validate"], help="Command to run")
    parser.add_argument("file", type=str, help="Path to JSON file")
    args = parser.parse_args()

    if args.command == "validate":
        try:
            with open(args.file, "r", encoding="utf-8") as f:
                doc = json.load(f)
        except Exception as e:
            print(f"Failed to load JSON: {e}", file=sys.stderr)
            sys.exit(1)

        errors = validate(doc)
        if errors:
            for err in errors:
                print(err)
            sys.exit(1)
        else:
            # Success: no output, exit 0
            sys.exit(0)


if __name__ == "__main__":
    _cli()