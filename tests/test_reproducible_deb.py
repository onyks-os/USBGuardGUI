"""DEB payload and dependency metadata regression tests, without containers."""

import hashlib
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "reproducible_deb", Path(__file__).resolve().parents[1] / "packaging/reproducible/deb.py")
deb = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(deb)

MANIFEST = '''
[package]
name = "usbguard-gui"
version = "0.1.1"
repository = "https://example.org"
description = "Test GUI"
[package.metadata.deb]
maintainer = "Test <test@example.org>"
section = "admin"
priority = "optional"
depends = "$auto, libgtk-4-1 (>= 4.14), usbguard (>= 1.1.0)"
recommends = "polkitd | policykit-1"
extended-description = "First paragraph.\\n\\nSecond paragraph."
assets = [
 ["target/release/usbguard-gui", "usr/bin/", "755"],
 ["README.md", "usr/share/doc/usbguard-gui/README.md", "644"]]
'''


class DebTests(unittest.TestCase):
    def fixture(self, root):
        source = root / "source"
        source.mkdir()
        (source / "Cargo.toml").write_text(MANIFEST)
        (source / "README.md").write_text("documentation")
        (source / "LICENSE").write_text("MIT license text")
        output = root / "output"
        output.mkdir()
        (output / "usbguard-gui").write_bytes(b"verified ELF")
        return source, output

    def test_stage_preserves_bytes_modes_and_installed_checksums(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, output = self.fixture(root)
            payload = root / "payload"
            package = deb.stage(source, output / "usbguard-gui", payload)
            self.assertEqual(package["name"], "usbguard-gui")
            binary = payload / "usr/bin/usbguard-gui"
            self.assertEqual(binary.read_bytes(), b"verified ELF")
            self.assertEqual((payload / "usr/share/doc/usbguard-gui/copyright").read_text(), "MIT license text")
            self.assertEqual(binary.stat().st_mode & 0o777, 0o755)
            self.assertEqual((payload / "usr/share/doc/usbguard-gui/README.md").stat().st_mode & 0o777, 0o644)
            for line in (payload / "DEBIAN/md5sums").read_text().splitlines():
                digest, name = line.split("  ")
                self.assertEqual(digest, hashlib.md5((payload / name).read_bytes(), usedforsecurity=False).hexdigest())
            control = deb.control_text(package, "shlibs:Depends=libc6 (>= 2.39)")
            for expected in ["Version: 0.1.1-1", "Architecture: amd64",
                             "Depends: libc6 (>= 2.39), libgtk-4-1 (>= 4.14), usbguard (>= 1.1.0)",
                             "Recommends: polkitd | policykit-1", " First paragraph.\n .\n Second paragraph."]:
                self.assertIn(expected, control)
            self.assertNotIn("$auto", control)
            for bad in ["", "unexpected=libc6", "shlibs:Depends= "]:
                with self.assertRaises(ValueError):
                    deb.control_text(package, bad)

    def test_invalid_destination_is_rejected(self):
        for destination in ["/usr/bin/", "usr/../etc/", "etc/"]:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source, output = self.fixture(root)
                (source / "Cargo.toml").write_text(MANIFEST.replace("usr/bin/", destination))
                with self.assertRaises(ValueError):
                    deb.stage(source, output / "usbguard-gui", root / "payload")

    def test_main_normalizes_all_metadata_and_records_queries(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, output = self.fixture(root)
            calls = []

            def run(command, **kwargs):
                calls.append(command)
                if "--build" in command:
                    payload = Path(command[-2])
                    for path in [payload, *payload.rglob("*")]:
                        self.assertEqual(path.stat().st_mtime, 1234)
                    self.assertEqual(sorted(p.name for p in (payload / "DEBIAN").iterdir()), ["control", "md5sums"])
                    self.assertIn("--root-owner-group", command)
                    self.assertIn("--threads-max=1", command)
                    Path(command[-1]).write_bytes(b"DEB")
                else:
                    kwargs["stdout"].write("query output")

            with patch.object(deb.Path, "cwd", return_value=source), \
                    patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "1234"}), \
                    patch.object(deb.subprocess, "check_output", return_value="shlibs:Depends=libc6 (>= 2.39)\n") as auto, \
                    patch.object(deb.subprocess, "run", side_effect=run):
                deb.main(output)
                self.assertEqual(auto.call_args.kwargs["cwd"], root / "deb")
                self.assertEqual(len(calls), 3)
                self.assertEqual((output / "usbguard-gui_0.1.1-1_amd64.deb").read_bytes(), b"DEB")
                self.assertTrue((output / "deb-control.txt").exists())


if __name__ == "__main__":
    unittest.main()
