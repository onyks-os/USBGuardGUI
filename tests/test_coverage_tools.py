"""The documented Make entry points work without a custom interactive PATH."""
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class CoverageToolTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.tools = self.root / "tools"
        self.tools.mkdir()
        # Cargo searches CARGO_HOME/bin before PATH for external subcommands.
        # Keep a developer's (or CI's) installed plugins outside these fixtures.
        self.cargo_home = self.root / "cargo-home"
        self.cargo_home.mkdir()
        self.environment = dict(os.environ, PATH=str(self.bin),
                                USBGUARD_GUI_TEST_TOOLS_ROOT=str(self.root / "desktop"))
        (self.bin / "dirname").symlink_to(shutil.which("dirname"))

    def executable(self, path, body="exit 0"):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)

    def make(self, target):
        environment = dict(os.environ, PATH="/usr/bin:/bin", CARGO_HOME=str(self.cargo_home))
        return subprocess.run([shutil.which("make"), "--no-print-directory", target,
                               f"COVERAGE_TOOLS_DIR={self.tools}",
                               f"CARGO={shutil.which('cargo')}"],
                              cwd=ROOT, env=environment, text=True,
                              capture_output=True, check=False)

    def test_repo_local_cargo_plugin_is_found_outside_the_callers_path(self):
        self.executable(self.tools / "cargo-llvm-cov", "echo checkout-local-coverage")
        result = self.make("coverage-tools")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("checkout-local-coverage", result.stdout)

    def test_missing_plugin_stops_all_entry_points_with_install_instructions(self):
        for target in ("coverage", "coverage-app", "coverage-core"):
            with self.subTest(target=target):
                result = self.make(target)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("--version 0.9.1 --locked --root target/tools", result.stderr)
                self.assertIn("rustup component add llvm-tools-preview", result.stderr)
                self.assertNotIn("no such command", result.stderr)

    def test_gui_preflight_finds_an_unpacked_bundle_without_starting_a_display(self):
        for name in ("dbus-daemon", "dbus-run-session", "locale"):
            self.executable(self.bin / name)
        desktop = Path(self.environment["USBGUARD_GUI_TEST_TOOLS_ROOT"])
        for name in ("Xvfb", "xdotool"):
            self.executable(desktop / "usr/bin" / name, "exit 99")
        result = subprocess.run([str(ROOT / "scripts/test-gui.sh"), "--check"],
                                env=self.environment, text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_gui_tools_fail_before_creating_a_display(self):
        for name in ("dbus-daemon", "dbus-run-session", "locale"):
            self.executable(self.bin / name)
        result = subprocess.run([str(ROOT / "scripts/test-gui.sh"), "--check"],
                                env=self.environment, text=True, capture_output=True, check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing GTK test tools: Xvfb xdotool", result.stderr)
        self.assertIn("docs/testing.md", result.stderr)
