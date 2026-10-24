"""Check the isolation and input guards of the disposable VM driver."""
import argparse
import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

SPEC = importlib.util.spec_from_file_location(
    "clean_install", Path(__file__).resolve().parents[1] / "scripts/verify-clean-install.py")
vm = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(vm)


class CleanInstallTests(unittest.TestCase):
    def test_hash_mismatch_creates_no_vm_or_credentials(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "image"
            image.write_bytes(b"untrusted cloud image")
            args = argparse.Namespace(image=image, output=root / "vm", sha256="0" * 64)
            with patch.object(vm, "command") as run, self.assertRaisesRegex(ValueError, "SHA-256"):
                vm.prepare(args)
            run.assert_not_called()
            self.assertFalse(args.output.exists())

    def test_prepare_uses_private_disks_network_and_no_host_passthrough(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "image"
            image.write_bytes(b"verified fixture")
            digest = vm.hashlib.sha256(image.read_bytes()).hexdigest()
            args = argparse.Namespace(image=image, output=root / "vm", sha256=digest, distro="ubuntu")
            allocator = MagicMock()
            allocator.__enter__.return_value.getsockname.return_value = ("127.0.0.1", 12345)
            def record(command, **_kwargs):
                if command[0] == "ssh-keygen":
                    (args.output / "identity.pub").write_text("ssh-ed25519 fixture")
            with patch.object(vm, "command", side_effect=record) as run, patch.object(vm.socket, "socket", return_value=allocator), \
                 patch.object(vm.subprocess, "check_output", return_value="$6$hash\n"), \
                 patch.object(vm.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)):
                vm.prepare(args)
            qemu = next(call.args[0] for call in run.call_args_list if call.args[0][0] == "qemu-system-x86_64")
            self.assertIn("user,id=net,hostfwd=tcp:127.0.0.1:12345-:22", qemu)
            self.assertIn("usb-storage,drive=usbtest,serial=usbguard-clean-test,id=usbtest-device", qemu)
            self.assertFalse(any("usb-host" in argument or "virtfs" in argument or "/dev/bus/usb" in argument for argument in qemu))
            self.assertEqual((args.output.stat().st_mode & 0o777), 0o700)
            self.assertEqual(((args.output / "credentials.json").stat().st_mode & 0o777), 0o600)
            config = json.loads((args.output / "user-data").read_text().removeprefix("#cloud-config\n"))
            self.assertFalse(config["ssh_pwauth"])
            self.assertEqual(config["write_files"][0]["path"], "/etc/usbguard-clean-install-vm")

    def test_wrong_package_format_is_rejected_before_upload(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "vm.json").write_text(json.dumps({"distro": "fedora"}))
            with patch.object(vm, "command") as run, self.assertRaisesRegex(ValueError, "format"):
                vm.validate(argparse.Namespace(output=root, package=root / "candidate.deb"))
            run.assert_not_called()

    def test_copy_cannot_write_outside_guest_home(self):
        for destination in ("/etc/sudoers", "/home/usbguard-test/../../etc/sudoers"):
            with self.subTest(destination=destination), patch.object(vm, "command") as run, self.assertRaises(SystemExit):
                vm.main(["copy", "--output", "/tmp/fixture", __file__, destination])
            run.assert_not_called()

    def test_ssh_pins_identity_and_known_hosts_inside_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "vm.json").write_text(json.dumps({"port": 12345}))
            args = vm.ssh_args(root)
            self.assertIn("IdentitiesOnly=yes", args)
            self.assertIn(f"UserKnownHostsFile={root / 'known_hosts'}", args)
            self.assertEqual(args[-1], "usbguard-test@127.0.0.1")

    def test_stop_uses_only_the_private_qmp_monitor(self):
        monitor = MagicMock()
        stream = monitor.__enter__.return_value.makefile.return_value
        with patch.object(vm.socket, "socket", return_value=monitor):
            vm.stop(Path("/tmp/vm-fixture"))
        monitor.__enter__.return_value.connect.assert_called_once_with("/tmp/vm-fixture/qmp.sock")
        stream.write.assert_any_call(b'{"execute":"qmp_capabilities"}\n')
        stream.write.assert_any_call(b'{"execute":"quit"}\n')


if __name__ == "__main__":
    unittest.main()
