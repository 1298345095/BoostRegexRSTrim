"""Public CLI regressions for preflight validation and reproducible corpora."""

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import character_differential
import differential
from tool_paths import PROJECT_ROOT


class ValidationCliTests(unittest.TestCase):
    def setUp(self):
        (PROJECT_ROOT / "target").mkdir(exist_ok=True)

    def test_functional_corpus_is_unchanged(self):
        cases = list(differential.corpus(1298345095, 1000))
        wire = "".join("\t".join((op, pattern.hex(), data.hex(), template.hex())) + "\n"
                       for op, pattern, data, template in cases)
        self.assertEqual(len(cases), 17772)
        self.assertEqual(hashlib.sha256(wire.encode()).hexdigest(),
                         "225d01286e3b441b0c1310e4fba88c6ab6e420731db087552be6df0dea62ba10")

    def test_character_corpus_is_unchanged(self):
        cases = character_differential.cases(2000)
        keys = ("operation", "pattern_hex", "input_hex", "template_hex")
        wire = "".join("\t".join(case[key] for key in keys) + "\n" for case in cases)
        self.assertEqual(len(cases), 2930)
        self.assertEqual(hashlib.sha256(wire.encode()).hexdigest(),
                         "573269dee1aa8fdc64a85799f5fe63f2fdb254d92714939157916fa30627f338")

    def test_invalid_report_is_rejected_before_starting_any_driver(self):
        suffix = ".exe" if os.name == "nt" else ""
        with tempfile.TemporaryDirectory(dir=PROJECT_ROOT / "target") as directory:
            reference = Path(directory) / ("reference" + suffix)
            rust = Path(directory) / ("regex_driver" + suffix)
            # These are deliberately not executables: a launch would fail and
            # expose validation that happened after the process boundary.
            reference.write_text("not an executable", encoding="utf-8")
            rust.write_text("not an executable", encoding="utf-8")
            forbidden_report = Path(directory) / "must-not-write.json"
            for script in ("differential.py", "accounting_differential.py",
                           "character_differential.py", "benchmark.py"):
                with self.subTest(script=script):
                    result = subprocess.run(
                        [sys.executable, str(PROJECT_ROOT / "tools" / script),
                         "--reference", str(reference), "--rust", str(rust),
                         "--report", str(forbidden_report)],
                        cwd=PROJECT_ROOT, capture_output=True, text=True, timeout=10,
                        check=False)
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertIn("reports", result.stderr)
                    self.assertNotIn("Traceback", result.stderr)
                    self.assertFalse(forbidden_report.exists())

    def test_character_options_are_checked_at_each_process_boundary(self):
        suffix = ".exe" if os.name == "nt" else ""
        with tempfile.TemporaryDirectory(dir=PROJECT_ROOT / "target") as directory:
            reference = Path(directory) / ("reference" + suffix)
            reference.write_text("fixture", encoding="utf-8")
            with mock.patch.object(subprocess, "run") as run, \
                    mock.patch.object(subprocess, "check_output") as environment:
                with self.assertRaises(ValueError):
                    differential.run_driver(reference, [], locale="--benchmark")
                with self.assertRaises(ValueError):
                    differential.driver_environment(reference, lcid="-1")
            run.assert_not_called()
            environment.assert_not_called()

    def test_lcid_scenarios_forward_the_same_numeric_identifier(self):
        scenarios = dict(character_differential.character_scenarios([], ["01041", "0X411"], False))
        self.assertEqual(scenarios["lcid:01041"], ["--lcid", "1041"])
        self.assertEqual(scenarios["lcid:0X411"], ["--lcid", "1041"])


if __name__ == "__main__":
    unittest.main()
