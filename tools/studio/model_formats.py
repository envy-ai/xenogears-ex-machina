"""Export public original model dispatch facts from the existing C-table reader."""
from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import asdict
from pathlib import Path

from tools.analysis.dispatch_tables import primitive_table

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "src/runtime/crates/xem-studio/data/field-models.json"
SOURCES = ("tools/analysis/dispatch_tables.py", "src/decomp/src/resident/model_renderer.c")


def metadata() -> dict:
    table = primitive_table(ROOT)
    return {
        "schema_version": 1,
        "generator": "tools/studio/model_formats.py",
        "sources": {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in SOURCES},
        "types": [asdict(row) for row in table.types],
        "override_codes": sorted(table.override_codes),
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
            parser.exit(1, "Model metadata stale; run python3 -m tools.studio.model_formats\n")
        print("Model metadata matches the authoritative C-table reader.")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(expected)
        print(f"Wrote public model format metadata: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
