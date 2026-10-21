"""Export public field-opcode facts from the authoritative analysis tables."""

from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import asdict
from pathlib import Path

from tools.analysis import events

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "src/runtime/crates/xem-studio/data/field-opcodes.json"


def metadata() -> dict:
    return {
        "schema_version": 1,
        "source": "tools/analysis/events.py",
        "source_sha256": hashlib.sha256(Path(events.__file__).read_bytes()).hexdigest(),
        "generator": "tools/studio/field_opcodes.py",
        "prefix": events.PREFIX,
        "prefix_handler": events.PREFIX_HANDLER,
        "jump": events.JUMP,
        "allocation_slack": events.ALLOCATION_SLACK,
        "widths": events.WIDTHS,
        "flows": events.FLOWS,
        "comparisons": events.COMPARISONS,
        "condition_text": events.CONDITION_TEXT,
        "primary": {
            code: [asdict(form) for form in forms] for code, forms in events.PRIMARY.items()
        },
        "extended": {
            code: [asdict(form) for form in forms] for code, forms in events.EXTENDED.items()
        },
    }


def metadata_bytes() -> bytes:
    return (json.dumps(metadata(), sort_keys=True, indent=2) + "\n").encode("utf-8")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args(argv)
    expected = metadata_bytes()
    if args.check:
        if not args.output.is_file() or args.output.read_bytes() != expected:
            parser.exit(
                1, "Field opcode metadata is stale; run python3 -m tools.studio.field_opcodes\n"
            )
        print("Field opcode metadata matches the authoritative Python tables.")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(expected)
        print(f"Wrote public field opcode metadata: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
