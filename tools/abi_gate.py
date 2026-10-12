#!/usr/bin/env python3
"""Gate the Rust/C ABI: layout, enum values and exported signatures.

Three views of the ABI must agree exactly: the Rust types (`abi_layout` bin),
the C header (`plugin/tests/AbiLayout.cpp` compiled against it), and the pinned
snapshot in `verification/abi-layout.json`. Any drift fails here, before
packaging. An intentional ABI change is recorded with `--update`, which bumps
the snapshot's revision so the change is a visible diff.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from typing import Any

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
ROOT = TOOLS.parent
SNAPSHOT_PATH = ROOT / "verification" / "abi-layout.json"
HEADER_PATH = ROOT / "core" / "include" / "er_combat.h"
FFI_PATH = ROOT / "core" / "src" / "ffi.rs"
PROBE_PATH = ROOT / "plugin" / "tests" / "AbiLayout.cpp"

SNAPSHOT_SCHEMA = 1


class AbiError(RuntimeError):
    pass


def normalize_name(name: str) -> str:
    return re.sub(r"[^a-z0-9]", "", name.lower())


def normalize_enum_values(values: dict[str, int]) -> dict[str, int]:
    return {normalize_name(name): value for name, value in values.items()}


def normalize_layout(raw: dict[str, Any], *, origin: str) -> dict[str, Any]:
    """Return a layout with canonical enum names and no sentinel `COUNT`."""
    structs = raw.get("structs")
    enums = raw.get("enums")
    if not isinstance(structs, dict) or not isinstance(enums, dict) or not structs or not enums:
        raise AbiError(f"{origin} layout has no structs or enums")
    normalized_enums: dict[str, Any] = {}
    for name, entry in enums.items():
        values = dict(entry["values"])
        if "COUNT" in values:
            count = values.pop("COUNT")
            if count != max(values.values()) + 1:
                raise AbiError(f"{origin} enum {name}: COUNT={count} is not last value + 1")
        normalized_enums[name] = {"size": entry["size"], "values": normalize_enum_values(values)}
    return {"structs": structs, "enums": normalized_enums}


def diff_layouts(left: dict[str, Any], right: dict[str, Any], left_name: str, right_name: str) -> list[str]:
    problems: list[str] = []
    for kind in ("structs", "enums"):
        for name in sorted(set(left[kind]) | set(right[kind])):
            if name not in left[kind]:
                problems.append(f"{kind[:-1]} {name} exists in {right_name} but not {left_name}")
            elif name not in right[kind]:
                problems.append(f"{kind[:-1]} {name} exists in {left_name} but not {right_name}")
            elif left[kind][name] != right[kind][name]:
                problems.append(
                    f"{kind[:-1]} {name} differs: {left_name}={json.dumps(left[kind][name], sort_keys=True)} "
                    f"{right_name}={json.dumps(right[kind][name], sort_keys=True)}"
                )
    return problems


# --- exported function signatures ------------------------------------------------


def _scalar(token: str) -> str:
    token = token.strip()
    return {
        "uint8_t": "u8",
        "uint32_t": "u32",
        "int32_t": "i32",
        "float": "f32",
        "void": "unit",
    }.get(token, token)


def canonical_c_type(declaration: str, *, keep_name: bool) -> str:
    text = re.sub(r"\s+", " ", declaration.strip())
    text = text.replace("*", " * ")
    tokens = text.split()
    if keep_name and len(tokens) > 1 and tokens[-1] != "*":
        tokens = tokens[:-1]  # drop the parameter name
    const = "const" in tokens
    tokens = [token for token in tokens if token != "const"]
    pointer = "*" in tokens
    base = _scalar(" ".join(token for token in tokens if token != "*"))
    if tokens.count("*") > 1:
        raise AbiError(f"unsupported multi-level pointer in C declaration: {declaration!r}")
    return (f"ptr_const:{base}" if const else f"ptr:{base}") if pointer else base


def canonical_rust_type(declaration: str) -> str:
    text = re.sub(r"\s+", " ", declaration.strip())
    pointer = re.fullmatch(r"\*(mut|const) (\w+)", text)
    if pointer:
        return ("ptr_const:" if pointer.group(1) == "const" else "ptr:") + _scalar(RUST_SCALARS.get(pointer.group(2), pointer.group(2)))
    return _scalar(RUST_SCALARS.get(text, text))


RUST_SCALARS = {"u8": "u8", "u32": "u32", "i32": "i32", "f32": "f32"}


def parse_header_functions(header_text: str) -> dict[str, dict[str, Any]]:
    text = re.sub(r"/\*.*?\*/", " ", header_text, flags=re.DOTALL)
    text = re.sub(r"//[^\n]*", " ", text)
    functions: dict[str, dict[str, Any]] = {}
    for match in re.finditer(r"([A-Za-z_][\w\s\*]*?)\b(er_\w+)\s*\(([^)]*)\)\s*;", text):
        returns = canonical_c_type(match.group(1), keep_name=False)
        raw_params = match.group(3).strip()
        params = [] if raw_params in ("", "void") else [
            canonical_c_type(param, keep_name=True) for param in raw_params.split(",")
        ]
        functions[match.group(2)] = {"returns": returns, "params": params}
    if not functions:
        raise AbiError("no er_* prototypes found in the C header")
    return functions


def parse_rust_exports(source: str) -> dict[str, dict[str, Any]]:
    """Exported functions from ffi.rs, rejecting anything not plainly `extern "C"`."""
    marked = len(re.findall(r"#\[(?:unsafe\()?(?:no_mangle|export_name)", source))
    pattern = re.compile(
        r"#\[no_mangle\]\s*pub\s+(?:unsafe\s+)?extern\s+\"C\"\s+fn\s+(\w+)\s*\((.*?)\)\s*(?:->\s*([\w\s\*]+?))?\s*\{",
        flags=re.DOTALL,
    )
    functions: dict[str, dict[str, Any]] = {}
    for match in pattern.finditer(source):
        raw_params = re.sub(r"//[^\n]*", "", match.group(2)).strip().rstrip(",")
        params = []
        if raw_params:
            for param in re.split(r",(?![^<(]*[>)])", raw_params):
                params.append(canonical_rust_type(param.split(":", 1)[1]))
        returns = canonical_rust_type(match.group(3)) if match.group(3) else "unit"
        functions[match.group(1)] = {"returns": returns, "params": params}
    if marked != len(functions):
        raise AbiError(
            f"{marked} exported Rust symbols but only {len(functions)} are plain `pub extern \"C\"` functions; "
            "the calling convention must be spelled extern \"C\""
        )
    return functions


def diff_functions(rust: dict[str, Any], header: dict[str, Any]) -> list[str]:
    problems = []
    for name in sorted(set(rust) | set(header)):
        if name not in rust:
            problems.append(f"{name} is declared in the C header but not exported by Rust")
        elif name not in header:
            problems.append(f"{name} is exported by Rust but not declared in the C header")
        elif rust[name] != header[name]:
            problems.append(f"{name} signature differs: rust={rust[name]} header={header[name]}")
    return problems


# --- running the three views -----------------------------------------------------


def run_rust_layout(cargo: Path, env: dict[str, str], root: Path = ROOT) -> dict[str, Any]:
    completed = subprocess.run(
        [str(cargo), "run", "--quiet", "--locked", "--bin", "abi_layout"],
        cwd=root / "core",
        env=env,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode != 0:
        raise AbiError(f"cargo run --bin abi_layout failed:\n{completed.stderr}")
    return json.loads(completed.stdout)


def run_c_layout(cxx: str, temporary: Path, root: Path = ROOT) -> dict[str, Any]:
    executable = temporary / "abi-layout"
    compiled = subprocess.run(
        [
            cxx, "-std=c++2b", "-Wall", "-Wextra", "-Werror",
            "-I", str(root / "core" / "include"),
            str(root / "plugin" / "tests" / "AbiLayout.cpp"),
            "-o", str(executable),
        ],
        check=False, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    if compiled.returncode != 0:
        raise AbiError(f"C ABI probe does not compile against the header:\n{compiled.stdout}")
    ran = subprocess.run([str(executable)], check=False, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if ran.returncode != 0:
        raise AbiError(f"C ABI probe failed:\n{ran.stderr}")
    return json.loads(ran.stdout)


def load_snapshot(path: Path) -> dict[str, Any]:
    try:
        snapshot = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AbiError(f"cannot read ABI snapshot {path}: {error}") from error
    if snapshot.get("schema_version") != SNAPSHOT_SCHEMA or not isinstance(snapshot.get("revision"), int):
        raise AbiError("ABI snapshot has an unsupported schema")
    return snapshot


def current_abi(cargo: Path, env: dict[str, str], cxx: str, temporary: Path, root: Path = ROOT) -> dict[str, Any]:
    """Cross-check Rust against C and return the agreed ABI, or raise."""
    rust = normalize_layout(run_rust_layout(cargo, env, root), origin="rust")
    c_side = normalize_layout(run_c_layout(cxx, temporary, root), origin="C")
    problems = diff_layouts(rust, c_side, "rust", "C")
    functions_rust = parse_rust_exports((root / "core" / "src" / "ffi.rs").read_text(encoding="utf-8"))
    functions_c = parse_header_functions((root / "core" / "include" / "er_combat.h").read_text(encoding="utf-8"))
    problems += diff_functions(functions_rust, functions_c)
    if problems:
        raise AbiError("Rust and C ABI disagree:\n  " + "\n  ".join(problems))
    return {**rust, "functions": functions_c}


def check(cargo: Path, env: dict[str, str], cxx: str, temporary: Path, root: Path = ROOT) -> str:
    agreed = current_abi(cargo, env, cxx, temporary, root)
    snapshot = load_snapshot(root / "verification" / "abi-layout.json")
    if snapshot.get("layout") != agreed:
        pinned = snapshot["layout"]
        details = diff_layouts(pinned, agreed, "snapshot", "current") + diff_functions(
            pinned.get("functions", {}), agreed["functions"]
        )
        raise AbiError(
            f"ABI drifted from revision {snapshot['revision']} of verification/abi-layout.json:\n  "
            + "\n  ".join(details or ["(layout differs)"])
            + "\nIf the change is intentional run `python3 tools/abi_gate.py --update` and review the diff."
        )
    structs, enums, functions = len(agreed["structs"]), len(agreed["enums"]), len(agreed["functions"])
    return (
        f"ABI revision {snapshot['revision']}: {structs} structs, {enums} enums and {functions} functions "
        "agree across Rust, C, and the pinned snapshot"
    )


def update(cargo: Path, env: dict[str, str], cxx: str, temporary: Path, root: Path = ROOT) -> str:
    agreed = current_abi(cargo, env, cxx, temporary, root)
    path = root / "verification" / "abi-layout.json"
    revision = 1
    if path.exists():
        previous = load_snapshot(path)
        if previous["layout"] == agreed:
            return f"ABI revision {previous['revision']} is already current"
        revision = previous["revision"] + 1
    snapshot = {"schema_version": SNAPSHOT_SCHEMA, "revision": revision, "layout": agreed}
    path.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return f"wrote ABI revision {revision} to {path.relative_to(root)}"


def main() -> int:
    from verify import cargo_environment, find_cargo, find_cxx

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true", help="record an intentional ABI change")
    args = parser.parse_args()
    cargo = find_cargo()
    env = cargo_environment(cargo)
    try:
        with tempfile.TemporaryDirectory() as directory:
            action = update if args.update else check
            print(action(cargo, env, find_cxx(), Path(directory)))
    except AbiError as error:
        print(f"FAIL  abi: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
