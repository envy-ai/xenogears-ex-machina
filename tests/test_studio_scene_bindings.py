"""Keep scene actor and zone references tied to source-proven event handlers."""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import re
import tempfile
import unittest
from dataclasses import asdict
from pathlib import Path
from unittest.mock import patch

from tools.analysis import event_access, events
from tools.studio import scene_bindings


class StudioSceneBindingMetadataTests(unittest.TestCase):
    def test_generated_bindings_cover_canonical_actor_operands_and_source_proofs(self):
        committed = scene_bindings.OUTPUT.read_bytes()
        self.assertEqual(committed, scene_bindings.metadata_bytes())
        decoded = json.loads(committed)
        root = Path(__file__).resolve().parents[1]
        source = (root / scene_bindings.SOURCE).read_text()
        forms = [
            form
            for table in (*events.PRIMARY.values(), *events.EXTENDED.values())
            for form in table
        ]

        expected_sources = {
            path: hashlib.sha256((root / path).read_bytes()).hexdigest()
            for path in (
                scene_bindings.SOURCE,
                "tools/analysis/events.py",
                "tools/analysis/event_access.py",
            )
        }
        self.assertEqual(decoded["sources"], expected_sources)
        self.assertEqual(decoded["generator"], "tools/studio/scene_bindings.py")
        self.assertIn("No runtime variable", decoded["scope"])

        reader = event_access.function(source, "field_event_read_actor_index")
        self.assertEqual(
            decoded["actor_reader_sha256"], hashlib.sha256(reader.encode()).hexdigest()
        )
        self.assertEqual(
            decoded["actor_readers"],
            {
                name: hashlib.sha256(
                    event_access.function(source, name).encode()
                ).hexdigest()
                for name in (
                    "field_event_read_actor_index",
                    "field_event_read_actor_index_or_leader",
                )
            },
        )

        expected_actors = {}
        for form in forms:
            for operand in form.operands:
                if operand.kind == "actor":
                    key = (form.handler, operand.offset)
                    if key in expected_actors:
                        self.assertEqual(expected_actors[key], asdict(operand))
                    expected_actors[key] = asdict(operand)

        actual_actors = {
            (row["handler"], row["operand"]["offset"]): row
            for row in decoded["actors"]
        }
        self.assertEqual(set(actual_actors), set(expected_actors))
        for key, operand in expected_actors.items():
            row = actual_actors[key]
            self.assertEqual(row["operand"], operand)
            self.assertIn(
                row["role"], {"actor_selector", "raw_and_actor_selector", "unknown"}
            )
            if row["role"] == "unknown":
                self.assertIsInstance(row["reason"], str)
                self.assertTrue(row["reason"])
                self.assertEqual(row["provenance"], [])
            else:
                self.assertIsNone(row["reason"])
                self.assertTrue(row["provenance"])
            for proof in row["provenance"]:
                body = event_access.function(source, proof["function"])
                self.assertEqual(proof["source"], scene_bindings.SOURCE)
                self.assertIn(proof["helper"], decoded["actor_readers"])
                self.assertIn(proof["helper"] + "(", body)
                call = re.compile(
                    re.escape(proof["helper"])
                    + rf"\(\s*{operand['offset']}\s*\)"
                )
                self.assertRegex(body, call)
                self.assertEqual(
                    proof["function_sha256"], hashlib.sha256(body.encode()).hexdigest()
                )

        store_form = next(
            form for form in forms if form.handler == "field_event_store_actor_flags"
        )
        expected_alias = next(
            asdict(operand)
            for operand in store_form.operands
            if operand.kind == "var" and operand.offset == 1
        )
        store_actor = actual_actors[("field_event_store_actor_flags", 1)]
        self.assertEqual(store_actor["aliases"], [expected_alias])

        move = actual_actors[("field_event_move_to_actor", 1)]
        self.assertEqual(move["role"], "raw_and_actor_selector")
        self.assertEqual(
            move["provenance"],
            [
                event_access.evidence(
                    scene_bindings.SOURCE,
                    "field_event_move",
                    event_access.function(source, "field_event_move"),
                    "field_event_read_actor_index",
                )
            ],
        )
        move_body = event_access.function(source, "field_event_move_to_actor")
        self.assertRegex(
            move_body,
            r"field_view\.components\.descriptors\[EVENT_OPERAND_BYTE\(1\)\]",
        )

        zone_pattern = re.compile(r"\b(?:EVENT_ZONE(?:_CORNER)?|CODE_ZONE(?:_CORNER)?)\b")
        expected_zones = {
            form.handler
            for form in forms
            if zone_pattern.search(event_access.function(source, form.handler))
        }
        actual_zones = {row["handler"] for row in decoded["zones"]}
        self.assertEqual(actual_zones, expected_zones)
        self.assertEqual(len(actual_zones), 4)
        macro_source = (
            "#define EVENT_ZONE (field_trigger_zones[EVENT_OPERAND_BYTE(1)])\n"
            "#define CODE_ZONE(code) (field_trigger_zones[(code)[1]])"
        )
        expected_macro_hash = hashlib.sha256(macro_source.encode()).hexdigest()
        for row in decoded["zones"]:
            self.assertEqual(row["role"], "zone_index")
            form = next(form for form in forms if form.handler == row["handler"])
            zone_operand = next(
                operand
                for operand in form.operands
                if operand.kind == "u8" and operand.offset == 1
            )
            self.assertEqual(row["operand"], asdict(zone_operand))
            self.assertEqual(row["macro_sha256"], expected_macro_hash)
            self.assertEqual(len(row["provenance"]), 1)
            proof = row["provenance"][0]
            body = event_access.function(source, row["handler"])
            self.assertEqual(
                proof,
                event_access.evidence(
                    scene_bindings.SOURCE,
                    row["handler"],
                    body,
                    "EVENT_ZONE/CODE_ZONE",
                ),
            )

    def test_missing_current_pc_reader_remains_unknown(self):
        original_function = event_access.function

        def without_request_event_reader(text: str, name: str) -> str:
            body = original_function(text, name)
            if name == "field_event_request_event":
                body = re.sub(
                    r"field_event_read_actor_index\(\s*1\s*\)",
                    "unproven_actor_reference(1)",
                    body,
                )
            return body

        with patch.object(
            scene_bindings.event_access,
            "function",
            side_effect=without_request_event_reader,
        ):
            decoded = json.loads(scene_bindings.metadata_bytes())

        unknown = next(
            row
            for row in decoded["actors"]
            if row["handler"] == "field_event_request_event"
            and row["operand"]["offset"] == 1
        )
        self.assertEqual(unknown["role"], "unknown")
        self.assertEqual(unknown["provenance"], [])
        self.assertTrue(unknown["reason"])

    def test_generator_external_output_and_stale_refusal(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "field-scene-bindings.json"
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                self.assertEqual(
                    scene_bindings.main(["--output", str(output)]), 0
                )
            self.assertEqual(output.read_bytes(), scene_bindings.metadata_bytes())
            self.assertEqual(
                stdout.getvalue(),
                f"Wrote public scene binding metadata: {output}\n",
            )

            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                self.assertEqual(
                    scene_bindings.main(["--check", "--output", str(output)]), 0
                )
            self.assertEqual(
                stdout.getvalue(),
                "Scene bindings match canonical forms and source reader proofs.\n",
            )

            output.write_text("stale\n")
            with contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as raised:
                    scene_bindings.main(["--check", "--output", str(output)])
            self.assertEqual(raised.exception.code, 1)


if __name__ == "__main__":
    unittest.main()
