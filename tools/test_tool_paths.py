"""Regression checks for test-tool file and executable boundaries."""

import os
from pathlib import Path
import tempfile
import unittest

from tool_paths import PROJECT_ROOT, driver_path, project_file, report_path


class ToolPathTests(unittest.TestCase):
    def setUp(self):
        (PROJECT_ROOT / "target").mkdir(exist_ok=True)
        (PROJECT_ROOT / "reports").mkdir(exist_ok=True)
        self.build_temp = tempfile.TemporaryDirectory(dir=PROJECT_ROOT / "target")
        self.report_temp = tempfile.TemporaryDirectory(dir=PROJECT_ROOT / "reports")
        self.outside_temp = tempfile.TemporaryDirectory(dir=PROJECT_ROOT.parent)
        self.addCleanup(self.build_temp.cleanup)
        self.addCleanup(self.report_temp.cleanup)
        self.addCleanup(self.outside_temp.cleanup)
        self.build = Path(self.build_temp.name)
        self.reports = Path(self.report_temp.name)
        self.outside = Path(self.outside_temp.name)

    def make_file(self, path):
        path.write_text("fixture", encoding="utf-8")
        return path

    def symlink(self, path, destination):
        try:
            path.symlink_to(destination, target_is_directory=destination.is_dir())
        except OSError as error:
            self.skipTest(f"Creating symlinks is unavailable: {error}")
        return path

    def test_project_inputs_accept_files_and_reject_directories(self):
        path = self.make_file(self.build / "workloads.json")
        self.assertEqual(project_file(path), path.resolve())
        with self.assertRaises(ValueError):
            project_file(self.build)

    def test_inputs_cannot_read_outside_the_checkout(self):
        path = self.make_file(self.outside / "workloads.json")
        with self.assertRaises(ValueError):
            project_file(path)
        with self.assertRaises(ValueError):
            project_file(self.build / ".." / ".." / ".." / path.parent.name / path.name)

    def test_inputs_cannot_escape_through_symlinks(self):
        path = self.make_file(self.outside / "workloads.json")
        link = self.symlink(self.build / "workloads.json", path)
        with self.assertRaises(ValueError):
            project_file(link)

    def test_only_named_build_artifacts_are_driver_candidates(self):
        suffix = ".exe" if os.name == "nt" else ""
        for name in ("reference", "reference-baseline", "regex_driver"):
            path = self.make_file(self.build / (name + suffix))
            self.assertEqual(driver_path(path), path.resolve())
        for path in (self.make_file(self.build / "shell"),
                     self.make_file(self.outside / ("reference" + suffix))):
            with self.assertRaises(ValueError):
                driver_path(path)

    def test_driver_symlink_cannot_select_an_external_program(self):
        name = "regex_driver.exe" if os.name == "nt" else "regex_driver"
        destination = self.make_file(self.outside / name)
        link = self.symlink(self.build / name, destination)
        with self.assertRaises(ValueError):
            driver_path(link)

    def test_dry_run_can_name_a_future_driver_without_relaxing_boundaries(self):
        name = "regex_driver.exe" if os.name == "nt" else "regex_driver"
        path = self.build / name
        self.assertEqual(driver_path(path, must_exist=False), path.resolve())
        with self.assertRaises(ValueError):
            driver_path(path)
        with self.assertRaises(ValueError):
            driver_path(self.outside / name, must_exist=False)

    def test_reports_allow_new_subdirectories_but_only_json(self):
        path = self.reports / "new" / "comparison.json"
        self.assertEqual(report_path(path), path.resolve())
        self.assertFalse(path.parent.exists())
        for path in (self.reports / "output.py", PROJECT_ROOT / "Cargo.toml",
                     self.outside / "result.json", self.reports):
            with self.assertRaises(ValueError):
                report_path(path)

    def test_report_parent_symlink_cannot_escape(self):
        link = self.symlink(self.reports / "linked", self.outside)
        with self.assertRaises(ValueError):
            report_path(link / "result.json")

    def test_existing_report_symlink_cannot_overwrite_external_file(self):
        destination = self.make_file(self.outside / "result.json")
        link = self.symlink(self.reports / "result.json", destination)
        with self.assertRaises(ValueError):
            report_path(link)
        self.assertEqual(destination.read_text(encoding="utf-8"), "fixture")


if __name__ == "__main__":
    unittest.main()
