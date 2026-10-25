"""The desktop fixture must stop before GUI imports or process launch on a host."""
import importlib.util
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    'guest_desktop', Path(__file__).resolve().parents[1] / 'scripts/clean-install-guest-desktop.py')
desktop = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(desktop)


class GuestDesktopGuardTests(unittest.TestCase):
    def test_unmarked_host_cannot_import_gui_or_launch_a_desktop(self) -> None:
        with patch.object(desktop.helper, 'require_guest', side_effect=RuntimeError('unmarked VM')), \
                patch.object(desktop.subprocess, 'Popen') as launch, \
                patch.dict(sys.modules, {'gi': None, 'pyatspi': None}), \
                self.assertRaisesRegex(RuntimeError, 'unmarked VM'):
            desktop.main()
        launch.assert_not_called()


if __name__ == '__main__':
    unittest.main()
