"""Hash and count the Python walk facts; write each decoded private map only locally."""

from __future__ import annotations

import hashlib
import json
import os
import struct
import sys
from dataclasses import asdict
from pathlib import Path

from tools.analysis import events


def digest(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def record(disc: int, map_id: int, package: events.EventPackage | None, output: Path) -> dict:
    if package is None:
        return {"disc": disc, "map": map_id, "placeholder": True}

    rows = b"".join(struct.pack("<32H", *row) for row in package.entries)
    component = (
        package.variable_unsigned_bits
        + struct.pack("<I", len(package.entries))
        + rows
        + package.bytecode
    )
    declared_size = package.bytecode_offset + len(package.declared)
    starts, skipped = events.script_entries(package)
    walked = events.walk(package.bytecode, [pc for _, _, pc in starts])
    instruction_hashes = []
    for pc, instruction in sorted(walked.instructions.items()):
        semantics = {
            "pc": instruction.pc,
            "opcode": instruction.opcode,
            "extended": instruction.extended,
            "size": instruction.size,
            "spec": asdict(instruction.spec),
            "operands": list(instruction.operands),
            "immediate": list(instruction.immediate),
            "successors": list(instruction.successors),
            "skips": list(instruction.skips),
            "inner": list(instruction.inner),
        }
        instruction_hashes.append({"pc": pc, "sha256": digest(semantics)})
    entries = [{"actor": actor, "event": event, "pc": pc} for actor, event, pc in starts]
    diagnostics = {
        "overlaps": walked.overlaps,
        "unpaired": walked.unpaired,
        "inner": walked.inner,
        "tables": walked.tables,
        "into": walked.into,
        "outside": walked.outside,
        "beyond_declared": [
            pc
            for pc, instruction in walked.instructions.items()
            if pc + instruction.size > len(package.declared)
        ],
        "followed_skips": walked.skips,
    }
    filename = f"disc{disc}-map{map_id:03}.bin"
    path = output / filename
    path.write_bytes(component)
    header = {
        "variable_unsigned_bits": list(package.variable_unsigned_bits),
        "actors": [list(row) for row in package.entries],
        "entries": entries,
        "skipped_zero_entries": skipped,
        "stream_size": len(component),
        "bytecode_offset": package.bytecode_offset,
        "declared_size": declared_size,
    }
    return {
        "disc": disc,
        "map": map_id,
        "placeholder": False,
        "path": str(path),
        "bytecode_stream_size": len(package.bytecode),
        "declared_size": declared_size,
        "component_size": len(component),
        "source_sha256": hashlib.sha256(component).hexdigest(),
        "declared_sha256": hashlib.sha256(component[:declared_size]).hexdigest(),
        "header_sha256": digest(header),
        "instruction_count": len(instruction_hashes),
        "instruction_hashes": instruction_hashes,
        "diagnostic_counts": {
            name: len(value) if isinstance(value, list) else value
            for name, value in diagnostics.items()
        },
        "diagnostics_sha256": digest(diagnostics),
    }


def main() -> int:
    root_value = os.environ.get("XEM_STUDIO_EXTRACT_ROOT")
    output_value = os.environ.get("XEM_STUDIO_ORACLE_DIR")
    if not root_value or not output_value:
        print("oracle requires XEM_STUDIO_EXTRACT_ROOT and XEM_STUDIO_ORACLE_DIR", file=sys.stderr)
        return 2
    root = Path(root_value).resolve()
    output = Path(output_value).resolve()
    repository = Path(__file__).resolve().parents[3]
    if not output.is_relative_to(repository / "tmp/studio") or output.exists():
        print("oracle output must be a fresh directory under ignored tmp/studio", file=sys.stderr)
        return 2
    output.mkdir(parents=True)
    for disc in (1, 2):
        if not (root / "extract" / f"disc{disc}" / "manifest.json").is_file():
            print(f"missing extract/disc{disc}/manifest.json", file=sys.stderr)
            return 2
        if not (root / "discs" / f"disc{disc}.bin").is_file():
            print(f"missing discs/disc{disc}.bin", file=sys.stderr)
            return 2

    for disc in (1, 2):
        extract = root / "extract" / f"disc{disc}"
        raw_disc = root / "discs" / f"disc{disc}.bin"
        for map_id, path in events.map_files(extract, raw_disc):
            package = events.map_events(path.read_bytes())
            record_json = record(disc, map_id, package, output)
            serialized = json.dumps(record_json, separators=(",", ":"))
            print(serialized, flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
