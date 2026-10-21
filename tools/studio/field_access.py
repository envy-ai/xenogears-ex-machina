"""Export public source-proven field operand access facts, without game bytes."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from tools.analysis import event_access

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "src/runtime/crates/xem-studio/data/field-access.json"


def metadata() -> dict:
    paths = [
        "tools/analysis/events.py",
        "tools/analysis/event_access.py",
        event_access.SOURCE,
        event_access.READER_SOURCE,
        event_access.SCREEN_SOURCE,
    ]
    return {
        "schema_version": 1,
        "generator": "tools/studio/field_access.py",
        "sources": {path: event_access.digest((ROOT / path).read_bytes()) for path in paths},
        "scope": (
            "Possible explicit operand accesses only; "
            "no execution order or implicit-state coverage."
        ),
        "facts": event_access.operand_facts(),
    }


def metadata_bytes() -> bytes:
    return (json.dumps(metadata(), sort_keys=True, indent=2) + "\n").encode()


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args(argv)
    expected = metadata_bytes()
    if args.check:
        if not args.output.is_file() or args.output.read_bytes() != expected:
            parser.exit(
                1, "Field access metadata is stale; run python3 -m tools.studio.field_access\n"
            )
        print("Field access metadata matches the authoritative forms and recovered source.")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(expected)
        print(f"Wrote public field access metadata: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
