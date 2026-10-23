"""Keep public scene model metadata tied to the recovered C dispatch table."""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import tempfile
import unittest
from dataclasses import asdict
from pathlib import Path

from tools.analysis.dispatch_tables import primitive_table
from tools.studio import model_formats


class StudioModelFormatMetadataTests(unittest.TestCase):
    def test_generated_table_and_source_identity_match_the_authoritative_reader(self):
        committed = model_formats.OUTPUT.read_bytes()
        self.assertEqual(committed, model_formats.metadata_bytes())

        root = Path(__file__).resolve().parents[1]
        table = primitive_table(root)
        source_hashes = {
            path: hashlib.sha256((root / path).read_bytes()).hexdigest()
            for path in model_formats.SOURCES
        }
        expected = {
            "schema_version": 1,
            "generator": "tools/studio/model_formats.py",
            "sources": source_hashes,
            "types": [asdict(row) for row in table.types],
            "override_codes": sorted(table.override_codes),
        }
        expected = json.loads(json.dumps(expected))
        decoded = json.loads(committed)
        self.assertEqual(decoded, expected)
        self.assertEqual(decoded["sources"], source_hashes)
        self.assertEqual(decoded["generator"], "tools/studio/model_formats.py")
        self.assertEqual(len(decoded["types"]), len(table.types))

    def test_generator_reports_and_checks_an_external_output(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "field-models.json"
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                self.assertEqual(model_formats.main(["--output", str(output)]), 0)
            self.assertEqual(output.read_bytes(), model_formats.metadata_bytes())
            self.assertEqual(
                stdout.getvalue(), f"Wrote public model format metadata: {output}\n"
            )

            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                self.assertEqual(
                    model_formats.main(["--check", "--output", str(output)]), 0
                )
            self.assertEqual(
                stdout.getvalue(),
                "Model metadata matches the authoritative C-table reader.\n",
            )

            output.write_text("stale\n")
            with contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as raised:
                    model_formats.main(["--check", "--output", str(output)])
            self.assertEqual(raised.exception.code, 1)


if __name__ == "__main__":
    unittest.main()
