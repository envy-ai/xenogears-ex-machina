"""Keep the Studio's generated opcode metadata tied to the Python source tables."""

from __future__ import annotations

import hashlib
import json
import unittest
from dataclasses import asdict
from pathlib import Path

from tools.analysis import event_access, events
from tools.studio import field_access
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

    def test_field_access_metadata_tracks_source_proofs_and_operand_coverage(self):
        committed = field_access.OUTPUT.read_bytes()
        self.assertEqual(committed, field_access.metadata_bytes())
        decoded = json.loads(committed)
        expected_sources = {
            path: hashlib.sha256(
                (Path(__file__).resolve().parents[1] / path).read_bytes()
            ).hexdigest()
            for path in decoded["sources"]
        }
        self.assertEqual(decoded["sources"], expected_sources)
        self.assertEqual(decoded["generator"], "tools/studio/field_access.py")
        self.assertIn("no execution order", decoded["scope"])

        facts = decoded["facts"]
        self.assertEqual(len(facts), 658)
        self.assertEqual(
            {direction: sum(fact["direction"] == direction for fact in facts)
             for direction in ("read", "write", "read_write", "unknown")},
            {"read": 550, "write": 86, "read_write": 17, "unknown": 5},
        )
        self.assertTrue(
            all(fact["provenance"] for fact in facts if fact["direction"] != "unknown")
        )
        form_operands = {
            (form.handler, operand.kind, operand.offset, operand.flags, operand.bit)
            for form in (
                form
                for forms in (*events.PRIMARY.values(), *events.EXTENDED.values())
                for form in forms
            )
            for operand in form.operands
            if operand.kind in {"var", "iv", "sel", "bit", "data"}
        }
        fact_operands = {
            (
                fact["handler"],
                fact["operand"]["kind"],
                fact["operand"]["offset"],
                fact["operand"].get("flags"),
                fact["operand"].get("bit"),
            )
            for fact in facts
        }
        self.assertEqual(fact_operands, form_operands)
        self.assertEqual(field_access.event_access.digest(Path(event_access.__file__).read_bytes()),
                         decoded["sources"]["tools/analysis/event_access.py"])


if __name__ == "__main__":
    unittest.main()
