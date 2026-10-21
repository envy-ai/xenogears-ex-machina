"""Keep the Studio's generated opcode metadata tied to the Python source tables."""

from __future__ import annotations

import hashlib
import json
import unittest
from dataclasses import asdict
from pathlib import Path

from tools.analysis import events
from tools.studio.field_opcodes import OUTPUT, metadata, metadata_bytes


class StudioFieldOpcodeMetadataTests(unittest.TestCase):
    def test_generated_metadata_matches_authoritative_source_tables(self):
        committed = OUTPUT.read_bytes()
        self.assertEqual(committed, metadata_bytes())

        decoded = json.loads(committed)
        source_hash = hashlib.sha256(Path(events.__file__).read_bytes()).hexdigest()
        expected = {
            "schema_version": 1,
            "source": "tools/analysis/events.py",
            "source_sha256": source_hash,
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
                str(code): [asdict(form) for form in forms]
                for code, forms in events.PRIMARY.items()
            },
            "extended": {
                str(code): [asdict(form) for form in forms]
                for code, forms in events.EXTENDED.items()
            },
        }
        expected = json.loads(json.dumps(expected))
        self.assertEqual(decoded, expected)
        self.assertEqual(decoded["source"], "tools/analysis/events.py")
        self.assertEqual(decoded["source_sha256"], source_hash)
        self.assertEqual(decoded["prefix"], events.PREFIX)
        self.assertEqual(decoded["jump"], events.JUMP)
        self.assertEqual(decoded["allocation_slack"], events.ALLOCATION_SLACK)

        self.assertEqual(sum(map(len, decoded["primary"].values())), 261)
        self.assertEqual(sum(map(len, decoded["extended"].values())), 247)
        self.assertEqual(
            sum(map(len, decoded["primary"].values()))
            + sum(map(len, decoded["extended"].values())),
            508,
        )

        self.assertEqual(decoded["primary"]["16"][0]["mnemonic"], "walk_to")
        self.assertEqual(decoded["primary"]["16"][0]["select"], [1, 255, 0])
        self.assertEqual(decoded["primary"]["16"][1]["setup"], 9)
        self.assertEqual(decoded["primary"]["166"][0]["flow"], "table")
        self.assertEqual(decoded["primary"]["166"][0]["operands"][0]["kind"], "iv")

    def test_source_and_generated_paths_are_repository_relative(self):
        root = Path(__file__).resolve().parents[1]
        self.assertEqual(OUTPUT, root / "src/runtime/crates/xem-studio/data/field-opcodes.json")
        self.assertEqual(metadata()["generator"], "tools/studio/field_opcodes.py")


if __name__ == "__main__":
    unittest.main()
