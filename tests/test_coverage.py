"""Coverage gating must reject incomplete reports and rounding shortcuts."""
import contextlib
import importlib.util
import io
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("coverage_gate", ROOT / "scripts/check-coverage.py")
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


class CoverageGateTests(unittest.TestCase):
    def report(self, covered=9800):
        return {"data": [{"files": [
            {"filename": str(p), "summary": {"lines": {"count": 10000, "covered": covered}}}
            for p in gate.required_files(ROOT, False)
        ]}]}

    def check(self, report, core=False):
        with contextlib.redirect_stdout(io.StringIO()):
            return gate.check(report, ROOT, core)

    def test_exactly_98_percent_passes(self):
        self.assertEqual(self.check(self.report()), 0)

    def test_rounded_98_percent_fails(self):
        report = self.report()
        report["data"][0]["files"][0]["summary"]["lines"]["covered"] -= 1
        self.assertEqual(self.check(report), 1)

    def test_missing_or_empty_gui_file_fails_even_with_high_total(self):
        for empty in (False, True):
            report = self.report(10000)
            files = report["data"][0]["files"]
            item = next(f for f in files if f["filename"].endswith("src/ui/window.rs"))
            if empty:
                item["summary"]["lines"] = {"count": 0, "covered": 0}
            else:
                files.remove(item)
            self.assertEqual(self.check(report), 1)

    def test_counters_must_be_real_nonnegative_integers(self):
        for counters in [(10, 11), (10, -1), (True, 1), (10, "9")]:
            report = self.report()
            report["data"][0]["files"][0]["summary"]["lines"] = dict(zip(("count", "covered"), counters))
            with self.assertRaises(ValueError):
                self.check(report)

    def test_duplicate_files_cannot_inflate_total(self):
        report = self.report()
        report["data"][0]["files"] *= 2
        with self.assertRaises(ValueError):
            self.check(report)

    def test_external_test_code_cannot_inflate_total(self):
        report = self.report(9700)
        report["data"][0]["files"].append({"filename": str(ROOT / "tests/extra.rs"),
            "summary": {"lines": {"count": 10000000, "covered": 10000000}}})
        self.assertEqual(self.check(report), 1)

    def test_empty_report_fails(self):
        self.assertEqual(self.check({"data": []}), 1)

    def test_legacy_core_gate_remains_strict(self):
        self.assertEqual(self.check(self.report(9000), True), 1)
        self.assertEqual(self.check(self.report(9001), True), 0)

    def test_actual_command_handles_help_valid_and_malformed_reports(self):
        import json
        import subprocess
        import sys
        import tempfile
        command = [sys.executable, str(ROOT / "scripts/check-coverage.py")]
        help_result = subprocess.run(command + ["--help"], capture_output=True, text=True, check=False)
        self.assertEqual(help_result.returncode, 0)
        self.assertIn(">90%", help_result.stdout)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            path.write_text(json.dumps(self.report()))
            result = subprocess.run(command + [str(path)], capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stdout)
            path.write_text("not JSON")
            result = subprocess.run(command + [str(path)], capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 1)
            self.assertIn("invalid coverage report", result.stdout)
