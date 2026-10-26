"""Reject incomplete Wayland launches without inspecting a live desktop."""
import contextlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location(
    'guest_wayland', Path(__file__).resolve().parents[1] / 'scripts/clean-install-guest-wayland.py')
wayland = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(wayland)

# Shape of WAYLAND_DEBUG=client output from the disposable Fedora guest.
FRAME = '''[21:56:56.678179] -> xdg_wm_base#16.get_xdg_surface(new id xdg_surface#20, wl_surface#18)
[21:56:56.681959] -> xdg_surface#20.get_toplevel(new id xdg_toplevel#21)
[21:56:56.683122] -> xdg_toplevel#21.set_title("USBGuardGUI")
[21:56:56.683754] -> wl_surface#18.commit()
[21:56:57.003077] -> xdg_surface#20.ack_configure(2)
[21:56:57.889373] -> wl_surface#18.attach(wl_buffer#23, 0, 0)
[21:56:57.889732] -> wl_surface#18.frame(new id wl_callback#26)
[21:56:57.890019] -> wl_surface#18.commit()
[21:56:57.937215] -> wl_surface#14.attach(wl_buffer#24, 0, 0)
[21:56:57.938357] -> wl_surface#14.commit()
[21:56:57.939195] {Default Queue} wl_callback#26.done(805192)
'''


class GuestWaylandTests(unittest.TestCase):
    def test_complete_main_window_frame_passes_despite_cursor_traffic(self) -> None:
        self.assertEqual(wayland.mapped_frame(FRAME),
                         {'xdg_surface': 20, 'surface': 18, 'toplevel': 21, 'frame_callback': 26})
        # A frame request may precede the buffer attachment within one commit.
        lines = FRAME.splitlines()
        lines[5], lines[6] = lines[6], lines[5]
        self.assertIsNotNone(wayland.mapped_frame('\n'.join(lines)))

    def test_registry_or_unconfigured_empty_window_cannot_pass(self) -> None:
        for protocol in ('', 'wl_registry#2.global(20, "xdg_wm_base", 5)',
                         '\n'.join(FRAME.splitlines()[:4]),
                         FRAME.replace('ack_configure(2)', 'configure(2)'),
                         FRAME.replace('set_title("USBGuardGUI")', 'set_title("other")')):
            with self.subTest(protocol=protocol):
                self.assertIsNone(wayland.mapped_frame(protocol))

    def test_wrong_surface_callback_or_missing_commit_cannot_pass(self) -> None:
        for protocol in (
            FRAME.replace('wl_surface#18.attach', 'wl_surface#99.attach'),
            FRAME.replace('wl_surface#18.frame', 'wl_surface#99.frame'),
            FRAME.replace('wl_callback#26.done', 'wl_callback#99.done'),
            FRAME.replace('wl_surface#18.commit()', ''),
            FRAME.replace('wl_callback#26.done(805192)', ''),
            FRAME.replace('attach(wl_buffer#23,', 'attach(nil,'),
        ):
            with self.subTest(protocol=protocol):
                self.assertIsNone(wayland.mapped_frame(protocol))

    def test_separate_incomplete_commits_cannot_be_combined(self) -> None:
        protocol = FRAME.replace('wl_surface#18.frame', 'wl_surface#18.commit()\nwl_surface#18.frame')
        self.assertIsNone(wayland.mapped_frame(protocol))
        # A callback delivered before the buffer commit proves no later frame.
        protocol = FRAME.replace('wl_callback#26.done(805192)', '')
        protocol = protocol.replace('wl_surface#18.frame', 'wl_callback#26.done(805192)\nwl_surface#18.frame')
        self.assertIsNone(wayland.mapped_frame(protocol))

    def test_guest_guard_refuses_missing_marker_root_and_wrong_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            marker = Path(temporary) / 'marker'
            with self.assertRaisesRegex(RuntimeError, 'marked disposable VM'):
                wayland.require_guest(marker)
            marker.write_text(json.dumps({'serial': 'usbguard-clean-test'}))
            with patch.object(wayland.os, 'getuid', return_value=0), \
                    self.assertRaisesRegex(RuntimeError, 'unprivileged'):
                wayland.require_guest(marker)
            marker.write_text(json.dumps({'serial': 'physical-device'}))
            with patch.object(wayland.os, 'getuid', return_value=1000), \
                    self.assertRaisesRegex(RuntimeError, 'synthetic USB fixture'):
                wayland.require_guest(marker)

    def test_exited_process_overrides_apparently_successful_checkpoint(self) -> None:
        process = Mock(args=['usbguard-gui'], returncode=-11)
        process.poll.return_value = -11
        with self.assertRaisesRegex(RuntimeError, 'prematurely.*-11'):
            wayland.wait_for(lambda: True, [process], 1)
        with self.assertRaises(TimeoutError):
            wayland.wait_for(lambda: False, [], 0)

    def test_failed_preflight_removes_stale_passing_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, contextlib.chdir(temporary):
            receipt = Path('wayland-report.json')
            receipt.write_text('{"passed": true}')
            with patch.object(wayland, 'require_guest'), patch.object(wayland.shutil, 'which', return_value=None), \
                    self.assertRaisesRegex(RuntimeError, 'Install weston'):
                wayland.main()
            self.assertFalse(receipt.exists())


if __name__ == '__main__':
    unittest.main()
