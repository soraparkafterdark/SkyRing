#!/usr/bin/env python3
"""
Combat IR v1 validator.

Usage:
    python3 tools/combat_ir.py validate FILE

The validate function returns a list of error strings; an empty list means valid.
"""

import json
import sys
from typing import Any, Dict, List


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
        # Note: data_fingerprint can be null (None) for synthetic, or a non-empty
        # string for owner (enforced above).

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
                    if not (0 <= start < end <= duration):
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
                    if not (0 <= t_s <= duration):
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