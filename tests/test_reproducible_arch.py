"""Exercise the Arch adapter and real PKGBUILD package() without a container."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ArchTests(unittest.TestCase):
    def test_adapter_uses_original_assets_and_preserves_verified_binary(self):
        for mismatch in (False, True):
            with self.subTest(mismatch=mismatch), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source, output, commands = root / "source", root / "output", root / "commands"
                source.mkdir()
                output.mkdir()
                commands.mkdir()
                for name in ("data", "po"):
                    shutil.copytree(ROOT / name, source / name)
                (source / "packaging/arch").mkdir(parents=True)
                for name in ("packaging/arch/PKGBUILD", "packaging/70-usbguard-gui.rules.example", "LICENSE"):
                    shutil.copyfile(ROOT / name, source / name)
                version = next(line.split("=", 1)[1].strip() for line in
                               (ROOT / "packaging/arch/PKGBUILD").read_text().splitlines() if line.startswith("pkgver="))
                (source / "Cargo.toml").write_text(f'version = "{"9.9.9" if mismatch else version}"\n')
                binary = output / "usbguard-gui"
                binary.write_bytes(b"verified ELF bytes")
                config = root / "base.conf"
                config.write_text("# fixture\n")
                scripts = {
                    "makepkg": '''#!/bin/bash
set -euo pipefail
source "$2"
source ./PKGBUILD
! declare -F build
! declare -F prepare
! declare -F check
[[ ${#source[@]} == 0 ]]
declare -p depends optdepends options > "$PKGDEST/adapter-metadata.txt"
cat "$2" > "$PKGDEST/adapter-config.txt"
pkgdir="$PWD/pkg/$pkgname"
srcdir="$PWD/src"
mkdir -p "$pkgdir"
cd "$srcdir"
package
printf package > "$PKGDEST/$pkgname-$pkgver-$pkgrel-x86_64.pkg.tar.zst"
''',
                    "msgfmt": '''#!/bin/bash
set -euo pipefail
[[ $1 == -o ]]
printf catalogue > "$2"
''',
                    "bsdtar": '''#!/bin/bash
printf 'archive metadata\\n'
''',
                }
                for name, content in scripts.items():
                    path = commands / name
                    path.write_text(content)
                    path.chmod(0o755)
                env = {**os.environ, "PATH": f"{commands}:{os.environ['PATH']}",
                       "ARCH_WORK_DIR": str(root / "work"), "ARCH_OUTPUT_DIR": str(output),
                       "ARCH_MAKEPKG_CONFIG": str(config)}
                result = subprocess.run(["bash", str(ROOT / "packaging/reproducible/arch.sh")],
                                        cwd=source, env=env, capture_output=True, text=True, check=False)
                if mismatch:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("versions differ", result.stderr)
                    self.assertFalse((output / "adapter-metadata.txt").exists())
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    payload = root / "work/pkg/usbguard-gui"
                    installed = payload / "usr/bin/usbguard-gui"
                    self.assertEqual(installed.read_bytes(), binary.read_bytes())
                    self.assertEqual(installed.stat().st_mode & 0o777, 0o755)
                    self.assertTrue((payload / "usr/share/licenses/usbguard-gui/LICENSE").is_file())
                    self.assertEqual((payload / "usr/share/doc/usbguard-gui/70-usbguard-gui.rules.example").stat().st_mode & 0o777, 0o644)
                    self.assertFalse((payload / "etc").exists())
                    metadata = (output / "adapter-metadata.txt").read_text()
                    for expected in ("usbguard", "polkit", "!strip", "!debug"):
                        self.assertIn(expected, metadata)
                    self.assertIn("zstd -c -T1 -19", (output / "adapter-config.txt").read_text())
                    self.assertTrue((output / "arch-BUILDINFO.txt").is_file())
