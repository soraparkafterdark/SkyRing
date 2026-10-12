#!/usr/bin/env python3
"""Pure parsers for the Skyrim formats SkyRing needs to inspect.

Nothing here touches Steam, Proton, or the platform: every function takes bytes
(or a path) and returns plain data, so it runs and is tested natively on macOS
as well as on SteamOS. `skyrim_steamos.py` supplies the SteamOS-specific
discovery and the command line.

Covered formats:

* PE32+ images: sections, entropy, imports, exports, CodeView/PDB identity,
  version resource (SkyrimSE.exe, SKSE and plugin DLLs).
* Address Library `version-*.bin` / `versionlib-*.bin`: ID <-> RVA tables.
* Bethesda plugin (ESM/ESP/ESL) headers, top-level group counts, and WEAP
  records.
* Printable ASCII / UTF-16LE strings in a binary.

Layouts for Address Library and WEAP DNAM follow community documentation and
are checked structurally (the Address Library parser must consume the file
exactly). Cross-check decoded values against the game before trusting them.
"""

from __future__ import annotations

from collections import Counter
from datetime import datetime, timezone
import math
import mmap
from pathlib import Path
import re
import struct
import zlib
from typing import Any, Iterator

PE32_PLUS_MAGIC = 0x20B
VS_FIXEDFILEINFO_SIGNATURE = b"\xbd\x04\xef\xfe"
VS_FIXEDFILEINFO_STRUC_VERSION = 0x00010000
DATA_DIRECTORY_NAMES = ("export", "import", "resource", "exception", "security", "reloc", "debug", "architecture",
                        "global_ptr", "tls", "load_config", "bound_import", "iat", "delay_import", "clr", "reserved")
SECTION_FLAGS = ((0x20, "code"), (0x40, "initialized"), (0x80, "uninitialized"), (0x20000000, "exec"),
                 (0x40000000, "read"), (0x80000000, "write"))
MAX_ENTROPY_SAMPLE = 8 * 1024 * 1024
MAX_IMPORTS = 5000


class ReError(RuntimeError):
    """A file does not parse as the format it claims to be."""


def _unpack(fmt: str, buf: Any, offset: int) -> tuple[Any, ...]:
    try:
        return struct.unpack_from(fmt, buf, offset)
    except struct.error as error:
        raise ReError(f"truncated data at 0x{offset:x}") from error


def _cstring(buf: Any, offset: int, limit: int = 512) -> str:
    end = buf.find(b"\0", offset, offset + limit)
    if offset < 0 or end < 0:
        raise ReError(f"unterminated string at 0x{offset:x}")
    return bytes(buf[offset:end]).decode("ascii", "replace")


def _timestamp(stamp: int) -> str | None:
    return datetime.fromtimestamp(stamp, timezone.utc).isoformat() if stamp else None


def shannon_entropy(data: bytes | memoryview) -> float:
    if not data:
        return 0.0
    counts = Counter(bytes(data))
    total = len(data)
    return -sum(n / total * math.log2(n / total) for n in counts.values())


# --- PE -----------------------------------------------------------------------

def find_file_version(buf: Any) -> str | None:
    """File version from the first valid VS_FIXEDFILEINFO record, as a.b.c.d."""
    offset = buf.find(VS_FIXEDFILEINFO_SIGNATURE)
    while offset != -1:
        if offset + 16 <= len(buf):
            _, struc_version, ms, ls = struct.unpack_from("<IIII", buf, offset)
            if struc_version == VS_FIXEDFILEINFO_STRUC_VERSION:
                return f"{ms >> 16}.{ms & 0xFFFF}.{ls >> 16}.{ls & 0xFFFF}"
        offset = buf.find(VS_FIXEDFILEINFO_SIGNATURE, offset + 1)
    return None


def analyze_pe(path: Path, deep: bool = True) -> dict[str, Any]:
    if path.stat().st_size == 0:
        raise ReError(f"{path.name} is empty")
    with path.open("rb") as handle, mmap.mmap(handle.fileno(), 0, access=mmap.ACCESS_READ) as view:
        try:
            return parse_pe(view, deep)
        except ReError as error:
            raise ReError(f"{path.name}: {error}") from error


def parse_pe(buf: Any, deep: bool = True) -> dict[str, Any]:
    if bytes(buf[:2]) != b"MZ" or len(buf) < 0x40:
        raise ReError("not a PE executable")
    pe_offset = _unpack("<I", buf, 0x3C)[0]
    if bytes(buf[pe_offset:pe_offset + 4]) != b"PE\0\0":
        raise ReError("no PE header")
    machine, section_count, stamp, _, _, optional_size, characteristics = _unpack("<HHIIIHH", buf, pe_offset + 4)
    optional = pe_offset + 24
    magic = _unpack("<H", buf, optional)[0] if optional_size >= 2 else 0
    table = optional + optional_size
    sections = []
    for index in range(min(section_count, 96)):
        name, vsize, vaddr, rawsize, rawptr, *_rest, flags = _unpack("<8sIIIIIIHHI", buf, table + index * 40)
        sections.append({
            "name": name.rstrip(b"\0").decode("ascii", "replace"),
            "virtual_address": vaddr, "virtual_size": vsize, "raw_size": rawsize, "raw_pointer": rawptr,
            "flags": [label for bit, label in SECTION_FLAGS if flags & bit], "characteristics": flags,
        })
    info: dict[str, Any] = {
        "machine": hex(machine), "is_x64": machine == 0x8664, "pe32_plus": magic == PE32_PLUS_MAGIC,
        "timestamp_utc": _timestamp(stamp), "is_dll": bool(characteristics & 0x2000),
        "sections": sections, "section_names": [s["name"] for s in sections],
        # SteamStub wraps the Steam copy in a `.bind` section; .text is encrypted on disk until it runs.
        "steam_drm_wrapper": any(s["name"] == ".bind" for s in sections),
        "file_version": find_file_version(buf),
    }
    if not deep or magic != PE32_PLUS_MAGIC:
        return info

    entry, = _unpack("<I", buf, optional + 16)
    image_base, = _unpack("<Q", buf, optional + 24)
    size_of_image, = _unpack("<I", buf, optional + 56)
    subsystem, = _unpack("<H", buf, optional + 68)
    dll_characteristics, = _unpack("<H", buf, optional + 70)
    directory_count, = _unpack("<I", buf, optional + 108)
    directories = {}
    for index in range(min(directory_count, 16)):
        rva, size = _unpack("<II", buf, optional + 112 + index * 8)
        if rva or size:
            directories[DATA_DIRECTORY_NAMES[index]] = {"rva": rva, "size": size}
    info.update({
        "image_base": hex(image_base), "entry_point_rva": hex(entry), "size_of_image": size_of_image,
        "subsystem": subsystem, "dll_characteristics": hex(dll_characteristics),
        "aslr": bool(dll_characteristics & 0x40), "nx": bool(dll_characteristics & 0x100),
        "data_directories": directories, "errors": [],
    })

    def rva_to_offset(rva: int) -> int:
        for section in sections:
            start = section["virtual_address"]
            span = max(section["virtual_size"], section["raw_size"])
            if start <= rva < start + span:
                delta = rva - start
                if delta >= section["raw_size"]:
                    raise ReError(f"RVA 0x{rva:x} is in uninitialised data")
                return section["raw_pointer"] + delta
        if sections and rva < sections[0]["virtual_address"]:
            return rva  # headers
        raise ReError(f"RVA 0x{rva:x} is outside every section")

    for section in sections:
        sample = memoryview(buf)[section["raw_pointer"]:section["raw_pointer"] + min(section["raw_size"], MAX_ENTROPY_SAMPLE)]
        section["entropy"] = round(shannon_entropy(sample), 3)
    info["packed_sections"] = [s["name"] for s in sections if "code" in s["flags"] and s["entropy"] > 7.0]

    for key, reader in (("imports", _read_imports), ("exports", _read_exports), ("debug", _read_debug)):
        directory = directories.get({"imports": "import", "exports": "export", "debug": "debug"}[key])
        info[key] = [] if key != "exports" else {"name": None, "functions": []}
        if directory:
            try:
                info[key] = reader(buf, directory, rva_to_offset)
            except ReError as error:
                info["errors"].append(f"{key}: {error}")
    return info


def _read_imports(buf: Any, directory: dict[str, int], rva_to_offset: Any) -> list[dict[str, Any]]:
    imports = []
    offset = rva_to_offset(directory["rva"])
    while True:
        original, _, _, name_rva, first = _unpack("<IIIII", buf, offset)
        if name_rva == 0 and first == 0:
            break
        functions: list[str] = []
        thunk = rva_to_offset(original or first)
        while len(functions) < MAX_IMPORTS:
            value, = _unpack("<Q", buf, thunk)
            if value == 0:
                break
            functions.append(f"#{value & 0xFFFF}" if value >> 63 else _cstring(buf, rva_to_offset(value & 0x7FFFFFFF) + 2))
            thunk += 8
        imports.append({"dll": _cstring(buf, rva_to_offset(name_rva)), "functions": functions})
        offset += 20
    return imports


def _read_exports(buf: Any, directory: dict[str, int], rva_to_offset: Any) -> dict[str, Any]:
    offset = rva_to_offset(directory["rva"])
    _, _, _, _, name_rva, base, function_count, name_count, functions_rva, names_rva, ordinals_rva = _unpack(
        "<IIHHIIIIIII", buf, offset)
    names = {}
    for index in range(min(name_count, 20000)):
        name_ptr, = _unpack("<I", buf, rva_to_offset(names_rva) + index * 4)
        ordinal, = _unpack("<H", buf, rva_to_offset(ordinals_rva) + index * 2)
        names[ordinal] = _cstring(buf, rva_to_offset(name_ptr))
    exported = []
    for ordinal in range(min(function_count, 20000)):
        rva, = _unpack("<I", buf, rva_to_offset(functions_rva) + ordinal * 4)
        if rva:
            forwarded = directory["rva"] <= rva < directory["rva"] + directory["size"]
            exported.append({"ordinal": base + ordinal, "name": names.get(ordinal), "rva": hex(rva),
                             "forwarder": _cstring(buf, rva_to_offset(rva)) if forwarded else None})
    return {"name": _cstring(buf, rva_to_offset(name_rva)), "functions": exported}


def _read_debug(buf: Any, directory: dict[str, int], rva_to_offset: Any) -> list[dict[str, Any]]:
    entries = []
    offset = rva_to_offset(directory["rva"])
    for index in range(directory["size"] // 28):
        _, stamp, _, _, kind, size, _, pointer = _unpack("<IIHHIIII", buf, offset + index * 28)
        entry: dict[str, Any] = {"type": kind, "timestamp_utc": _timestamp(stamp)}
        if kind == 2 and size >= 24 and bytes(buf[pointer:pointer + 4]) == b"RSDS":  # CodeView
            guid = bytes(buf[pointer + 4:pointer + 20])
            entry.update({"pdb_guid": guid.hex(), "pdb_age": _unpack("<I", buf, pointer + 20)[0],
                          "pdb_path": _cstring(buf, pointer + 24, max(size - 24, 1))})
        entries.append(entry)
    return entries


# --- Address Library ----------------------------------------------------------

def parse_address_library(data: bytes) -> dict[str, Any]:
    """Decode an Address Library database into ID -> RVA entries.

    The delta encoding is the one CommonLibSSE reads. The parse is only accepted
    as correct when it produces exactly `address_count` entries and consumes the
    file to the last byte; `parsed_exactly` records that check.
    """
    if len(data) < 28:
        raise ReError("Address Library file is too short")
    format_version, *version = struct.unpack_from("<5i", data, 0)
    if format_version not in (1, 2):
        raise ReError(f"unknown Address Library format {format_version}")
    offset = 20
    name_length, = _unpack("<i", data, offset)
    offset += 4
    if not 0 <= name_length <= 256:
        raise ReError("implausible module name length")
    name = data[offset:offset + name_length].decode("ascii", "replace")
    offset += name_length
    pointer_size, count = _unpack("<ii", data, offset)
    offset += 8
    if pointer_size not in (4, 8) or not 0 <= count <= 50_000_000:
        raise ReError("implausible pointer size or address count")

    def take(fmt: str) -> int:
        nonlocal offset
        value, = _unpack(fmt, data, offset)
        offset += struct.calcsize(fmt)
        return value

    id_to_rva: dict[int, int] = {}
    previous_id = previous_offset = 0
    for _ in range(count):
        kind = take("<B")
        low, high = kind & 0xF, kind >> 4
        if low == 0:
            ident = take("<Q")
        elif low == 1:
            ident = previous_id + 1
        elif low == 2:
            ident = previous_id + take("<B")
        elif low == 3:
            ident = previous_id - take("<B")
        elif low == 4:
            ident = previous_id + take("<H")
        elif low == 5:
            ident = previous_id - take("<H")
        elif low == 6:
            ident = take("<H")
        elif low == 7:
            ident = take("<I")
        else:
            raise ReError(f"bad ID encoding {low}")
        base = previous_offset // pointer_size if high & 8 else previous_offset
        mode = high & 7
        if mode == 0:
            rva = take("<Q")
        elif mode == 1:
            rva = base + 1
        elif mode == 2:
            rva = base + take("<B")
        elif mode == 3:
            rva = base - take("<B")
        elif mode == 4:
            rva = base + take("<H")
        elif mode == 5:
            rva = base - take("<H")
        elif mode == 6:
            rva = take("<H")
        else:
            rva = take("<I")
        if high & 8:
            rva *= pointer_size
        id_to_rva[ident] = rva
        previous_id, previous_offset = ident, rva
    return {
        "format": format_version, "version": ".".join(str(part) for part in version), "module": name,
        "pointer_size": pointer_size, "declared_count": count, "entries": id_to_rva,
        "parsed_exactly": offset == len(data) and len(id_to_rva) == count,
        "bytes_unread": len(data) - offset,
    }


def lookup_address(database: dict[str, Any], *, ident: int | None = None, rva: int | None = None) -> dict[str, Any] | None:
    entries = database["entries"]
    if ident is not None:
        return {"id": ident, "rva": hex(entries[ident])} if ident in entries else None
    reverse = next(((i, r) for i, r in entries.items() if r == rva), None)
    return {"id": reverse[0], "rva": hex(reverse[1])} if reverse else None


def read_at_rva(pe_path: Path, pe: dict[str, Any], rva: int, length: int = 16) -> bytes | None:
    """Bytes at an RVA as stored on disk; meaningless in encrypted SteamStub sections."""
    for section in pe["sections"]:
        start = section["virtual_address"]
        if start <= rva < start + section["raw_size"]:
            with pe_path.open("rb") as handle:
                handle.seek(section["raw_pointer"] + rva - start)
                return handle.read(length)
    return None


# --- Bethesda plugins (ESM / ESP / ESL) ---------------------------------------

RECORD_HEADER = struct.Struct("<4sIIIIHH")  # type, data size, flags, form ID, VCS, version, unknown
GROUP_HEADER = struct.Struct("<4sI4sIHHHH")  # GRUP, size incl. header, label, type, stamp, unk, version, unk
FLAG_ESM, FLAG_LOCALIZED, FLAG_ESL, FLAG_COMPRESSED = 0x1, 0x80, 0x200, 0x40000
WEAPON_ANIMATION_TYPES = ("HandToHand", "OneHandSword", "OneHandDagger", "OneHandAxe", "OneHandMace",
                          "TwoHandSword", "TwoHandAxe", "Bow", "Staff", "Crossbow")


def subrecords(data: bytes) -> Iterator[tuple[str, bytes]]:
    offset = 0
    extended_size: int | None = None
    while offset + 6 <= len(data):
        kind, size = struct.unpack_from("<4sH", data, offset)
        offset += 6
        if kind == b"XXXX":  # next subrecord's size does not fit in 16 bits
            extended_size, = struct.unpack_from("<I", data, offset)
            offset += size
            continue
        if extended_size is not None:
            size, extended_size = extended_size, None
        yield kind.decode("ascii", "replace"), data[offset:offset + size]
        offset += size


def record_payload(buf: Any, offset: int, size: int, flags: int) -> bytes:
    payload = bytes(buf[offset:offset + size])
    if flags & FLAG_COMPRESSED:
        try:
            return zlib.decompress(payload[4:])
        except zlib.error as error:
            raise ReError(f"bad compressed record at 0x{offset:x}") from error
    return payload


def read_plugin_header(buf: Any) -> dict[str, Any]:
    if len(buf) < RECORD_HEADER.size:
        raise ReError("plugin is too short")
    kind, size, flags, _, _, version, _ = RECORD_HEADER.unpack_from(buf, 0)
    if kind != b"TES4":
        raise ReError("not a Bethesda plugin: first record is not TES4")
    header: dict[str, Any] = {"flags": hex(flags), "master": bool(flags & FLAG_ESM), "light": bool(flags & FLAG_ESL),
                              "localized": bool(flags & FLAG_LOCALIZED), "form_version": version, "masters": [],
                              "end_offset": RECORD_HEADER.size + size}
    for sub, payload in subrecords(record_payload(buf, RECORD_HEADER.size, size, flags)):
        if sub == "HEDR" and len(payload) >= 12:
            header["hedr_version"], header["record_count"], header["next_object_id"] = struct.unpack_from("<fiI", payload)
            header["hedr_version"] = round(header["hedr_version"], 3)
        elif sub == "MAST":
            header["masters"].append(payload.rstrip(b"\0").decode("utf-8", "replace"))
        elif sub in ("CNAM", "SNAM"):
            header["author" if sub == "CNAM" else "description"] = payload.rstrip(b"\0").decode("utf-8", "replace")
    return header


def iter_records(buf: Any, start: int, end: int, wanted: set[str] | None = None) -> Iterator[tuple[str, int, int, int, int]]:
    """Yield (type, form ID, flags, payload offset, payload size), descending into groups."""
    offset = start
    while offset + 24 <= end:
        kind = bytes(buf[offset:offset + 4])
        if kind == b"GRUP":
            _, group_size, *_ = GROUP_HEADER.unpack_from(buf, offset)
            if group_size < 24 or offset + group_size > end:
                raise ReError(f"group at 0x{offset:x} overruns its parent")
            yield from iter_records(buf, offset + 24, offset + group_size, wanted)
            offset += group_size
        else:
            _, size, flags, form_id, *_ = RECORD_HEADER.unpack_from(buf, offset)
            name = kind.decode("ascii", "replace")
            if offset + 24 + size > end:
                raise ReError(f"record {name} at 0x{offset:x} overruns its group")
            if wanted is None or name in wanted:
                yield name, form_id, flags, offset + 24, size
            offset += 24 + size


def summarize_plugin(buf: Any) -> dict[str, Any]:
    header = read_plugin_header(buf)
    groups: dict[str, int] = {}
    offset = header["end_offset"]
    while offset + 24 <= len(buf):
        kind, group_size, label, group_type, *_ = GROUP_HEADER.unpack_from(buf, offset)
        if kind != b"GRUP" or group_size < 24 or offset + group_size > len(buf):
            raise ReError(f"expected a top-level group at 0x{offset:x}")
        if group_type == 0:
            count = sum(1 for _ in iter_records(buf, offset + 24, offset + group_size))
            groups[label.decode("ascii", "replace")] = count
        offset += group_size
    header["groups"] = dict(sorted(groups.items()))
    header["records_walked"] = sum(groups.values())
    return header


def decode_weapon(buf: Any, form_id: int, flags: int, offset: int, size: int) -> dict[str, Any]:
    weapon: dict[str, Any] = {"form_id": f"{form_id:08X}", "editor_id": None, "keywords": []}
    for sub, payload in subrecords(record_payload(buf, offset, size, flags)):
        if sub == "EDID":
            weapon["editor_id"] = payload.rstrip(b"\0").decode("utf-8", "replace")
        elif sub == "DATA" and len(payload) >= 10:
            weapon["value"], weapon["weight"], weapon["damage"] = struct.unpack_from("<IfH", payload)
            weapon["weight"] = round(weapon["weight"], 3)
        elif sub == "DNAM" and len(payload) >= 12:
            animation, = struct.unpack_from("<B", payload)
            weapon["animation_type"] = (WEAPON_ANIMATION_TYPES[animation] if animation < len(WEAPON_ANIMATION_TYPES)
                                        else f"unknown({animation})")
            weapon["speed"], weapon["reach"] = (round(v, 4) for v in struct.unpack_from("<ff", payload, 4))
        elif sub == "KWDA":
            weapon["keywords"] = [f"{v:08X}" for v in struct.unpack_from(f"<{len(payload) // 4}I", payload)]
    return weapon


def extract_weapons(buf: Any) -> list[dict[str, Any]]:
    header = read_plugin_header(buf)
    weapons = []
    for kind, form_id, flags, offset, size in iter_records(buf, header["end_offset"], len(buf), {"WEAP"}):
        weapons.append(decode_weapon(buf, form_id, flags, offset, size))
    return weapons


# --- Strings ------------------------------------------------------------------

def extract_strings(buf: Any, min_length: int = 6, pattern: str | None = None, wide: bool = False,
                    limit: int = 100000) -> list[dict[str, Any]]:
    """Printable ASCII (or UTF-16LE) strings with their file offsets."""
    if wide:
        regex = re.compile(rb"(?:[\x20-\x7e]\x00){%d,}" % min_length)
    else:
        regex = re.compile(rb"[\x20-\x7e]{%d,}" % min_length)
    wanted = re.compile(pattern) if pattern else None
    found = []
    for match in regex.finditer(buf):
        text = match.group().decode("utf-16-le" if wide else "ascii")
        if wanted is None or wanted.search(text):
            found.append({"offset": match.start(), "text": text})
            if len(found) >= limit:
                break
    return found
