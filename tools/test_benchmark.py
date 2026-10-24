"""Regression tests for benchmark planning and validation; never collect measurements."""

import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import benchmark


class BenchmarkTests(unittest.TestCase):
    def setUp(self):
        # Platform metadata can launch system probes, such as `file` on macOS.
        # Isolate it so subprocess assertions observe only benchmark commands.
        metadata = mock.patch.multiple(
            benchmark.platform,
            platform=lambda: "TestOS-1.0",
            machine=lambda: "test-architecture",
            libc_ver=lambda: ("test-libc", "1.0"),
            processor=lambda: "test-processor",
        )
        metadata.start()
        self.addCleanup(metadata.stop)
        (benchmark.ROOT / "target").mkdir(exist_ok=True)
        (benchmark.ROOT / "reports").mkdir(exist_ok=True)
        self.artifacts = tempfile.TemporaryDirectory(dir=benchmark.ROOT / "target")
        self.reports = tempfile.TemporaryDirectory(dir=benchmark.ROOT / "reports")
        self.addCleanup(self.artifacts.cleanup)
        self.addCleanup(self.reports.cleanup)
        self.artifact_path = Path(self.artifacts.name)
        self.report = Path(self.reports.name) / "benchmark.json"
        self.reference = self.artifact_path / ("reference-baseline" + benchmark.SUFFIX)
        self.rust = self.artifact_path / ("regex_driver" + benchmark.SUFFIX)
        self.reference.write_bytes(b"reference fixture")
        self.rust.write_bytes(b"rust fixture")
        self.build_info = self.artifact_path / "build-info-baseline.json"
        self.build_info.write_text(json.dumps({
            "profile": "release", "raw_error_observer": None,
            "executable_sha256": benchmark.digest(self.reference),
        }), encoding="utf-8")

    def arguments(self, *extra):
        return ["benchmark.py", "--reference", str(self.reference),
                "--rust", str(self.rust), "--report", str(self.report), *extra]

    def run_main(self, *extra):
        with mock.patch.object(sys, "argv", self.arguments(*extra)), \
                contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()):
            return benchmark.main()

    def write_workloads(self, values):
        path = self.artifact_path / "workloads.json"
        path.write_text(json.dumps(values), encoding="utf-8")
        return path

    def test_default_workload_bytes_remain_identical(self):
        values = benchmark.load_workloads(None)
        self.assertEqual(len(values), 9)
        digest = hashlib.sha256(json.dumps(values, sort_keys=True).encode()).hexdigest()
        self.assertEqual(digest, "841640bd917470c9d6395df0e65a435e35e63df6ab5358fa00456c3c4b90adf3")

    def test_custom_workloads_reject_noncontiguous_or_invalid_hex(self):
        for value in ("0", "00 00", "00\n", "gg", "--run", None, 12):
            with self.subTest(value=value):
                item = {**benchmark.WORKLOADS[0], "pattern_hex": value}
                with self.assertRaises(ValueError):
                    benchmark.load_workloads(self.write_workloads([item]))

    def test_workloads_reject_duplicates_and_invalid_shapes(self):
        item = benchmark.WORKLOADS[0]
        for values in ({}, [], [item, item], [None], [{**item, "extra": "field"}]):
            with self.subTest(values=values), self.assertRaises(ValueError):
                benchmark.load_workloads(self.write_workloads(values))

    def test_run_without_designated_machine_starts_no_process(self):
        with mock.patch.object(subprocess, "run") as run, \
                mock.patch.object(subprocess, "check_output") as check, \
                self.assertRaises(SystemExit) as raised:
            self.run_main("--run")
        self.assertEqual(raised.exception.code, 2)
        run.assert_not_called()
        check.assert_not_called()

    def test_empty_build_command_is_a_reported_failure_before_execution(self):
        with mock.patch.object(subprocess, "run") as run, \
                mock.patch.object(subprocess, "check_output") as check:
            status = self.run_main("--run", "--designated-machine", "unit-test-fixture",
                                   "--rust-build-command", "")
        self.assertEqual(status, 1)
        report = json.loads(self.report.read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "failed")
        self.assertIn("--rust-build-command", report["error"])
        self.assertEqual(report["samples"], [])
        run.assert_not_called()
        check.assert_not_called()

    def test_dry_run_starts_no_process_and_needs_no_binaries(self):
        self.reference.unlink()
        self.rust.unlink()
        with mock.patch.object(subprocess, "run") as run, \
                mock.patch.object(subprocess, "check_output") as check:
            self.assertEqual(self.run_main("--dry-run"), 0)
        report = json.loads(self.report.read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "plan_only")
        self.assertEqual(report["samples"], [])
        self.assertEqual(len(report["planned_measurement_commands"]), 9)
        run.assert_not_called()
        check.assert_not_called()

    def test_check_mode_only_requests_measurement_free_checks(self):
        requests = []

        def respond(argv, **_kwargs):
            if argv[0] in ("rustc", "cargo"):
                return subprocess.CompletedProcess(argv, 0, "test toolchain", "")
            requests.append(argv)
            return subprocess.CompletedProcess(argv, 0, json.dumps({
                "ok": True, "operation": argv[2], "iterations": int(argv[-1]),
                "checksum": 1234, "measured": False,
            }), "")

        with mock.patch.object(subprocess, "run", side_effect=respond), \
                mock.patch.object(subprocess, "check_output", return_value="{}"):
            self.assertEqual(self.run_main("--check"), 0)
        report = json.loads(self.report.read_text(encoding="utf-8"))
        self.assertEqual(len(requests), 18)
        self.assertTrue(all(argv[1] == "--benchmark-check" for argv in requests))
        self.assertTrue(all(argv[-1] == "2" for argv in requests))
        self.assertEqual(report["status"], "passed")
        self.assertEqual(len(report["checks"]), 9)
        self.assertEqual(report["samples"], [])
        self.assertEqual(report["summary"], [])

    def test_check_rejects_timing_fields_in_checksum_response(self):
        response = {"ok": True, "measured": False, "checksum": 1234, "elapsed_ns": 0}
        with mock.patch.object(benchmark, "tool_version", return_value={}), \
                mock.patch.object(subprocess, "run", return_value=subprocess.CompletedProcess(
                    [], 0, json.dumps(response), "")), \
                mock.patch.object(subprocess, "check_output", return_value="{}"):
            self.assertEqual(self.run_main("--check"), 1)
        report = json.loads(self.report.read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "failed")
        self.assertIn("must not collect", report["error"])
        self.assertEqual(report["samples"], [])

    def test_cli_rejects_external_paths_before_reading_or_running(self):
        with tempfile.TemporaryDirectory(dir=benchmark.ROOT.parent) as directory:
            outside = Path(directory) / "outside.json"
            outside.write_text("[]", encoding="utf-8")
            for option in ("--workloads", "--report", "--reference-build-info", "--reference"):
                with self.subTest(option=option), \
                        mock.patch.object(subprocess, "run") as run, \
                        mock.patch.object(subprocess, "check_output") as check, \
                        mock.patch.object(benchmark, "load_workloads") as load, \
                        self.assertRaises(SystemExit) as raised:
                    self.run_main("--check", option, str(outside))
                self.assertEqual(raised.exception.code, 2)
                run.assert_not_called()
                check.assert_not_called()
                load.assert_not_called()
            self.assertEqual(outside.read_text(encoding="utf-8"), "[]")

    def test_workload_names_remain_data_and_never_choose_paths_or_arguments(self):
        name = "../../other.json; --benchmark"
        item = benchmark.workload(name, "search", b"a", b"ba")
        workloads = self.write_workloads([item])
        response = {"ok": True, "measured": False, "checksum": 1234}
        with mock.patch.object(benchmark, "tool_version", return_value={}), \
                mock.patch.object(benchmark, "invoke", return_value=response) as invoke, \
                mock.patch.object(subprocess, "check_output", return_value="{}"):
            self.assertEqual(self.run_main("--check", "--workloads", str(workloads)), 0)
        report = json.loads(self.report.read_text(encoding="utf-8"))
        self.assertEqual(report["workloads"][0]["name"], name)
        self.assertEqual(report["checks"][0]["workload"], name)
        self.assertEqual(list(self.report.parent.iterdir()), [self.report])
        self.assertEqual(invoke.call_count, 2)
        for call in invoke.call_args_list:
            self.assertEqual(call.args[0][1:], ["--benchmark-check", "search", "61", "6261", "", "2"])
        self.assertEqual(report["samples"], [])


if __name__ == "__main__":
    unittest.main()
