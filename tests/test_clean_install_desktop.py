"""The desktop fixture must stop before GUI imports or process launch on a host."""
import contextlib
import importlib.util
import io
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    'guest_desktop', Path(__file__).resolve().parents[1] / 'scripts/clean-install-guest-desktop.py')
desktop = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(desktop)


class GuestDesktopGuardTests(unittest.TestCase):
    def test_completed_window_traffic_does_not_hide_a_toolkit_critical(self) -> None:
        protocol = 'wl_surface#18.commit()\nwl_callback#26.done(42)\n'
        desktop.require_clean_app_log(protocol + 'Gtk-WARNING: test environment\n')
        for failure in ('GLib-GObject-CRITICAL', 'Gtk-CRITICAL', 'Gdk-ERROR', 'Gtk:ERROR'):
            with self.subTest(failure=failure), self.assertRaisesRegex(RuntimeError, failure):
                desktop.require_clean_app_log(protocol + failure + ': invalid object')

    def test_unmarked_host_cannot_import_gui_or_launch_a_desktop(self) -> None:
        with patch.object(desktop.helper, 'require_guest', side_effect=RuntimeError('unmarked VM')), \
                patch.object(desktop.subprocess, 'Popen') as launch, \
                patch.dict(sys.modules, {'gi': None, 'pyatspi': None}), \
                self.assertRaisesRegex(RuntimeError, 'unmarked VM'):
            desktop.main()
        launch.assert_not_called()

    def test_invalid_cycle_count_removes_stale_receipt_before_launch(self) -> None:
        for cycles in ('0', '-1', '101'):
            with self.subTest(cycles=cycles), tempfile.TemporaryDirectory() as temporary, \
                    contextlib.chdir(temporary), \
                    patch.object(desktop.helper, 'require_guest'), \
                    patch.object(sys, 'argv', ['desktop-test', '--desktop', 'gnome', '--cycles', cycles]), \
                    patch.object(desktop.subprocess, 'Popen') as launch, \
                    patch.dict(sys.modules, {'gi': None, 'pyatspi': None}):
                receipt = Path('desktop-gnome-report.json')
                receipt.write_text('{"passed": true}')
                with contextlib.redirect_stderr(io.StringIO()), \
                        self.assertRaises(SystemExit):
                    desktop.main()
                self.assertFalse(receipt.exists())
                launch.assert_not_called()

    def test_nonexecutable_binary_fails_before_gui_import_or_launch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, contextlib.chdir(temporary), \
                patch.object(desktop.helper, 'require_guest'), \
                patch.object(desktop.subprocess, 'Popen') as launch, \
                patch.dict(sys.modules, {'gi': None, 'pyatspi': None}):
            binary = Path('candidate')
            binary.write_text('not an executable')
            binary.chmod(0o600)
            receipt = Path('desktop-kde-report.json')
            receipt.write_text('{"passed": true}')
            with patch.object(sys, 'argv', ['desktop-test', '--desktop', 'kde', '--binary', str(binary)]), \
                    contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                desktop.main()
            self.assertFalse(receipt.exists())
            launch.assert_not_called()


if __name__ == '__main__':
    unittest.main()
