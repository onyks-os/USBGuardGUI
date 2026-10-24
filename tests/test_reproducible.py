"""Determinism and failure behavior of the reproducible-build driver (no containers)."""

import contextlib
import importlib.util
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "reproducible", Path(__file__).resolve().parents[1] / "scripts/reproducible-build.py"
)
reproducible = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(reproducible)

RPM_SPEC = importlib.util.spec_from_file_location(
    "reproducible_rpm", reproducible.ROOT / "packaging/reproducible/rpm.py"
)
rpm_builder = importlib.util.module_from_spec(RPM_SPEC)
RPM_SPEC.loader.exec_module(rpm_builder)


class ReproducibleTests(unittest.TestCase):
    def test_archive_normalizes_order_time_modes_and_ownership(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "z.rs").write_text("fn main() {}")
            (root / "a.sh").write_text("#!/bin/sh\n")
            (root / "a.sh").chmod(0o750)
            first, second = root / "first.tar", root / "second.tar"
            digest = reproducible.source_archive(root, [Path("z.rs"), Path("a.sh")], 1234, first)
            (root / "z.rs").touch()
            (root / "a.sh").chmod(0o711)
            self.assertEqual(digest, reproducible.source_archive(
                root, [Path("a.sh"), Path("z.rs")], 1234, second))
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with tarfile.open(first) as archive:
                self.assertEqual(archive.getnames(), ["a.sh", "z.rs"])
                for member in archive:
                    self.assertEqual((member.uid, member.gid, member.mtime), (0, 0, 1234))
                    self.assertEqual((member.uname, member.gname), ("", ""))
                self.assertEqual(archive.getmember("a.sh").mode, 0o755)
                self.assertEqual(archive.getmember("z.rs").mode, 0o644)

    def test_archive_rejects_symlinks_and_parent_traversal(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "file").write_text("data")
            (root / "link").symlink_to("file")
            for name in [Path("link"), Path("../file"), root / "file"]:
                with self.assertRaises(ValueError):
                    reproducible.source_archive(root, [name], 1, root / "source.tar")

    def test_changed_source_changes_snapshot_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "main.rs"
            source.write_text("before")
            before = reproducible.source_archive(root, [Path("main.rs")], 1, root / "one.tar")
            source.write_text("after")
            after = reproducible.source_archive(root, [Path("main.rs")], 1, root / "two.tar")
            self.assertNotEqual(before, after)

    def test_only_equal_nonempty_binaries_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "one", Path(directory) / "two"
            first.write_bytes(b"same binary")
            second.write_bytes(first.read_bytes())
            self.assertEqual(reproducible.verify_binaries(first, second), reproducible.sha256(first))
            stamp = first.stat().st_mtime_ns
            second.write_bytes(b"different!!")
            os.utime(second, ns=(stamp, stamp))
            with self.assertRaisesRegex(ValueError, "Binaries differ"):
                reproducible.verify_binaries(first, second)
            first.write_bytes(b"")
            second.write_bytes(b"")
            with self.assertRaisesRegex(ValueError, "non-empty"):
                reproducible.verify_binaries(first, second)
            second.unlink()
            with self.assertRaises(ValueError):
                reproducible.verify_binaries(first, second)

    def test_equal_binaries_with_build_path_leaks_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "one", Path(directory) / "two"
            for path in [b"/tmp/repro-one", b"/tmp/independent/repro-two", b"/dependencies/crate"]:
                first.write_bytes(b"ELF" + path)
                second.write_bytes(first.read_bytes())
                with self.assertRaisesRegex(ValueError, "Unnormalized"):
                    reproducible.verify_binaries(first, second)

    def test_rpm_comparison_rejects_missing_empty_changed_and_extra_packages(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "first", Path(directory) / "second"
            first.mkdir()
            second.mkdir()
            for data in [None, b"", b"changed", b"RPM"]:
                (first / "a.rpm").write_bytes(b"RPM")
                if data is not None:
                    (second / "a.rpm").write_bytes(data)
                if data == b"RPM":
                    self.assertEqual(reproducible.verify_packages(first, second),
                                     ("a.rpm", reproducible.sha256(first / "a.rpm")))
                else:
                    with self.assertRaises(ValueError):
                        reproducible.verify_packages(first, second)
            (second / "extra.rpm").write_bytes(b"RPM")
            with self.assertRaises(ValueError):
                reproducible.verify_packages(first, second)

    def test_deb_comparison_rejects_missing_empty_changed_and_extra_packages(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "first", Path(directory) / "second"
            first.mkdir()
            second.mkdir()
            for data in [None, b"", b"changed", b"RPM"]:
                (first / "a.deb").write_bytes(b"RPM")
                if data is not None:
                    (second / "a.deb").write_bytes(data)
                if data == b"RPM":
                    self.assertEqual(reproducible.verify_packages(first, second, "deb"),
                                     ("a.deb", reproducible.sha256(first / "a.deb")))
                else:
                    with self.assertRaises(ValueError):
                        reproducible.verify_packages(first, second, "deb")
            (second / "extra.deb").write_bytes(b"RPM")
            with self.assertRaises(ValueError):
                reproducible.verify_packages(first, second, "deb")

    def test_flatpak_comparison_rejects_missing_empty_changed_and_extra_packages(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "first", Path(directory) / "second"
            first.mkdir()
            second.mkdir()
            for data in [None, b"", b"changed", b"RPM"]:
                (first / "a.flatpak").write_bytes(b"RPM")
                if data is not None:
                    (second / "a.flatpak").write_bytes(data)
                if data == b"RPM":
                    self.assertEqual(reproducible.verify_packages(first, second, "flatpak"),
                                     ("a.flatpak", reproducible.sha256(first / "a.flatpak")))
                else:
                    with self.assertRaises(ValueError):
                        reproducible.verify_packages(first, second, "flatpak")
            (second / "extra.flatpak").write_bytes(b"RPM")
            with self.assertRaises(ValueError):
                reproducible.verify_packages(first, second, "flatpak")

    def test_arch_comparison_rejects_missing_empty_changed_and_extra_packages(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "first", Path(directory) / "second"
            first.mkdir()
            second.mkdir()
            for data in [None, b"", b"changed", b"RPM"]:
                (first / "a.pkg.tar.zst").write_bytes(b"RPM")
                if data is not None:
                    (second / "a.pkg.tar.zst").write_bytes(data)
                if data == b"RPM":
                    self.assertEqual(reproducible.verify_packages(first, second, "pkg.tar.zst"),
                                     ("a.pkg.tar.zst", reproducible.sha256(first / "a.pkg.tar.zst")))
                else:
                    with self.assertRaises(ValueError):
                        reproducible.verify_packages(first, second, "pkg.tar.zst")
            (second / "extra.pkg.tar.zst").write_bytes(b"RPM")
            with self.assertRaises(ValueError):
                reproducible.verify_packages(first, second, "pkg.tar.zst")

    def test_rpm_stage_preserves_payload_modes_epoch_and_declared_dependencies(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('''
[package]
name = "usbguard-gui"
version = "0.1.1"
license = "MIT"
repository = "https://example.org"
description = "Test GUI"
[package.metadata.generate-rpm]
summary = "Test"
assets = [
 {source="target/release/usbguard-gui", dest="/usr/bin/usbguard-gui", mode="755"},
 {source="README.md", dest="/usr/share/doc/usbguard-gui/README.md", mode="644", doc=true}]
[package.metadata.generate-rpm.requires]
gtk4 = ">= 4.14"
usbguard-dbus = "*"
[package.metadata.generate-rpm.recommends]
polkit = "*"
''')
            (root / "README.md").write_text("documentation")
            binary = root / "verified-binary"
            binary.write_bytes(b"original ELF bytes")
            payload = root / "payload"
            spec = rpm_builder.stage(root, binary, payload, 1234)
            installed = payload / "usr/bin/usbguard-gui"
            self.assertEqual(installed.read_bytes(), binary.read_bytes())
            self.assertEqual(installed.stat().st_mode & 0o777, 0o755)
            self.assertEqual(installed.stat().st_mtime, 1234)
            self.assertEqual((payload / "usr/share/doc/usbguard-gui/README.md").stat().st_mode & 0o777, 0o644)
            for line in ["Requires: gtk4 >= 4.14", "Requires: usbguard-dbus",
                         "Recommends: polkit", "%defattr(-,root,root,-)",
                         "%doc /usr/share/doc/usbguard-gui/README.md"]:
                self.assertIn(line, spec)
            (root / "Cargo.toml").write_text((root / "Cargo.toml").read_text().replace(
                "/usr/bin/usbguard-gui", "/usr/../etc/unsafe"))
            with self.assertRaises(ValueError):
                rpm_builder.stage(root, binary, payload, 1234)

    def test_rpm_build_verifies_digests_and_records_manifest_without_scriptlets(self):
        for package_count in [0, 1, 2]:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source, output = root / "source", root / "output"
                source.mkdir()
                output.mkdir()
                (root / "rpm").mkdir()
                calls = []

                def fake_run(command, *, calls=calls, root=root, package_count=package_count, **kwargs):
                    calls.append(command)
                    if command[0] == "rpmbuild":
                        packages = root / "rpm/RPMS/x86_64"
                        packages.mkdir(parents=True)
                        for index in range(package_count):
                            (packages / f"package-{index}.rpm").write_bytes(b"RPM")
                    elif "stdout" in kwargs:
                        kwargs["stdout"].write("query output")

                with patch.object(rpm_builder.Path, "cwd", return_value=source),                         patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "1234"}),                         patch.object(rpm_builder, "stage", return_value="spec text") as stage,                         patch.object(rpm_builder.subprocess, "run", side_effect=fake_run):
                    if package_count != 1:
                        with self.assertRaisesRegex(ValueError, "exactly one RPM"):
                            rpm_builder.main(output)
                    else:
                        rpm_builder.main(output)
                        self.assertEqual((output / "package-0.rpm").read_bytes(), b"RPM")
                        self.assertEqual((output / "usbguard-gui.spec").read_text(), "spec text")
                        self.assertIn(["rpm", "-K", str(output / "package-0.rpm")], calls)
                        self.assertIn("use_source_date_epoch_as_buildtime 1", calls[0])
                        self.assertEqual(stage.call_args.args[-1], 1234)
                        self.assertTrue((output / "rpm-scripts.txt").exists())

    def test_actual_compilation_is_offline_and_caches_are_not_mounted(self):
        for engine in ["podman", "docker"]:
            command = reproducible.container_command(
                engine, "builder", Path("/input"), Path("/output"), 1234,
                "first", Path("/vendor"))
            self.assertIn("--network=none", command)
            self.assertIn("--pull=never", command)
            self.assertIn("/vendor:/dependencies:ro,Z", command)
            self.assertNotIn("/target", " ".join(command))
            self.assertEqual("--userns=keep-id" in command, engine == "podman")
        prepare = reproducible.container_command(
            "podman", "builder", Path("/input"), Path("/output"), 1, "prepare")
        self.assertNotIn("--network=none", prepare)

    def test_git_selection_includes_modified_and_untracked_source_but_no_builds(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "src").mkdir()
            (root / "src/main.rs").write_text("original")
            subprocess.run(["git", "-C", str(root), "add", "src/main.rs"], check=True)
            (root / "src/main.rs").write_text("modified")
            (root / "src/new.rs").write_text("new")
            (root / "private.txt").write_text("not a build input")
            (root / ".cargo").mkdir()
            (root / ".cargo/config.toml").write_text("[build]\njobs = 1\n")
            (root / ".cargo/credentials.toml").write_text("must never be archived")
            (root / "target").mkdir()
            (root / "target/binary").write_text("not source")
            self.assertEqual(reproducible.source_files(root),
                             [Path(".cargo/config.toml"), Path("src/main.rs"), Path("src/new.rs")])
            reproducible.source_archive(root, reproducible.source_files(root), 1, root / "source.tar")
            with tarfile.open(root / "source.tar") as archive:
                self.assertEqual(archive.extractfile("src/main.rs").read(), b"modified")

    def test_snapshot_contains_embedded_packaging_resource(self):
        files = reproducible.source_files(reproducible.ROOT)
        self.assertIn(Path("packaging/70-usbguard-gui.rules.example"), files)
        self.assertIn(Path("packaging/arch/PKGBUILD"), files)
        self.assertIn(Path("tests/fixtures/rules/daemon-output.rules"), files)
        self.assertNotIn(Path("packaging/reproducible/Containerfile"), files)

    def test_recipe_digest_includes_driver_and_recipe_contents(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            recipe = root / "recipe"
            recipe.mkdir()
            driver = root / "driver.py"
            driver.write_text("driver one")
            (recipe / "Containerfile").write_text("recipe one")
            before = reproducible.recipe_digest(recipe, driver)
            driver.write_text("driver two")
            self.assertNotEqual(before, reproducible.recipe_digest(recipe, driver))
            driver.write_text("driver one")
            (recipe / "Containerfile").write_text("recipe two")
            self.assertNotEqual(before, reproducible.recipe_digest(recipe, driver))

    def test_dirty_checkout_requires_explicit_worktree_opt_in(self):
        with patch.object(reproducible.shutil, "which", return_value="podman"), \
                patch.object(reproducible, "git", return_value=b" M src/main.rs"), \
                contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                reproducible.main([])
            self.assertEqual(error.exception.code, 2)

    def test_existing_output_is_never_reused(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(reproducible.shutil, "which", return_value="podman"), \
                patch.object(reproducible, "git", return_value=b""), \
                contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                reproducible.main(["--output", directory])
            self.assertEqual(error.exception.code, 2)

    def test_command_failures_preserve_logs_and_propagate(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "build.log"
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaisesRegex(RuntimeError, "Command failed \\(7\\)"):
                    reproducible.logged([sys.executable, "-c",
                                         "import sys; print('compiler failed'); sys.exit(7)"], log)
                self.assertIn("compiler failed", log.read_text())
                reproducible.logged([sys.executable, "-c", "print('compiled')"], log)
                self.assertEqual(log.read_text(), "compiled\n")

    def test_recipe_rejects_non_file_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "unexpected-directory").mkdir()
            with self.assertRaises(ValueError):
                reproducible.recipe_digest(root, Path(__file__))

    def test_missing_engine_or_source_inputs_fail_before_output_creation(self):
        with contextlib.redirect_stderr(io.StringIO()), \
                patch.object(reproducible.shutil, "which", return_value=None):
            with self.assertRaises(SystemExit) as error:
                reproducible.main([])
            self.assertEqual(error.exception.code, 2)
        with tempfile.TemporaryDirectory() as directory, \
                contextlib.redirect_stderr(io.StringIO()), \
                patch.object(reproducible.shutil, "which", return_value="podman"), \
                patch.object(reproducible, "git", return_value=b""), \
                patch.object(reproducible, "source_files", return_value=[]):
            output = Path(directory) / "result"
            with self.assertRaises(SystemExit) as error:
                reproducible.main(["--output", str(output)])
            self.assertEqual(error.exception.code, 2)
            self.assertFalse(output.exists())

    def test_full_driver_records_only_success_and_freezes_its_inputs(self):
        for mismatch in [False, "binary", "rpm", "deb", "arch", "flatpak", "flatpak-binary"]:
            with self.subTest(mismatch=mismatch), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                for name in ["Cargo.toml", "Cargo.lock", "build.rs", "src/main.rs", "src/lib.rs",
                             "packaging/70-usbguard-gui.rules.example"]:
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("fixture")
                recipe = root / "recipe"
                recipe.mkdir()
                (recipe / "Containerfile").write_text("FROM fixed-digest")
                (recipe / "__pycache__").mkdir()
                (recipe / "__pycache__/rpm.pyc").write_bytes(b"local cache")
                output = root / "results"

                def fake_git(_root, *args):
                    if args[0] == "status":
                        return b""
                    if args[0] == "rev-parse":
                        return b"test-commit\n"
                    if args[0] == "show":
                        return b"1234\n"
                    return b"Cargo.toml\0Cargo.lock\0build.rs\0src/main.rs\0src/lib.rs\0packaging/70-usbguard-gui.rules.example\0"

                def fake_container(command, log, *, output=output, recipe=recipe, mismatch=mismatch):
                    log.write_text("container log")
                    if command[1] == "build":
                        # The build context must be the frozen recipe, not the
                        # live checkout. Change the latter to prove isolation.
                        self.assertEqual(command[-1], str(output / "inputs/recipe"))
                        (recipe / "Containerfile").write_text("modified during build")
                        return
                    phase = command[-1]
                    mount = next(part for part in command if part.endswith(":/output:Z"))
                    result = Path(mount.removesuffix(":/output:Z"))
                    if phase == "prepare":
                        (result / "vendor").mkdir()
                        (result / "packages.txt").write_text("packages")
                        (result / "toolchain.txt").write_text("compiler")
                    else:
                        (result / "usbguard-gui").write_bytes(
                            b"different" if mismatch == "binary" and phase == "second" else b"ELF binary")
                        (result / "usbguard-gui-0.1.1-1.x86_64.rpm").write_bytes(
                            b"changed RPM" if mismatch == "rpm" and phase == "second" else b"RPM fixture")
                        (result / "usbguard-gui_0.1.1-1_amd64.deb").write_bytes(
                            b"changed DEB" if mismatch == "deb" and phase == "second" else b"DEB fixture")
                        (result / "usbguard-gui-0.1.1-1-x86_64.pkg.tar.zst").write_bytes(
                            b"changed Arch" if mismatch == "arch" and phase == "second" else b"Arch fixture")
                        (result / "usbguard-gui-0.1.1.flatpak").write_bytes(
                            b"changed Flatpak" if mismatch == "flatpak" and phase == "second" else b"Flatpak fixture")
                        (result / "flatpak-bin").mkdir()
                        (result / "flatpak-bin/usbguard-gui").write_bytes(
                            b"changed binary" if mismatch == "flatpak-binary" and phase == "second" else b"Flatpak ELF")
                        (result / "elf-dynamic.txt").write_text("libgtk-4.so.1 libadwaita-1.so.0")

                with patch.object(reproducible, "ROOT", root), \
                        patch.object(reproducible, "RECIPE", recipe), \
                        patch.object(reproducible.shutil, "which", return_value="podman"), \
                        patch.object(reproducible, "git", side_effect=fake_git), \
                        patch.object(reproducible, "logged", side_effect=fake_container), \
                        patch.object(reproducible.subprocess, "check_output", return_value=b"image-id\n"), \
                        contextlib.redirect_stdout(io.StringIO()):
                    if mismatch:
                        with self.assertRaisesRegex(ValueError, "Binaries differ|RPM packages differ|DEB packages differ|PKG.TAR.ZST packages differ|FLATPAK packages differ"):
                            reproducible.main(["--output", str(output)])
                        self.assertFalse((output / "report.json").exists())
                    else:
                        self.assertEqual(reproducible.main(["--output", str(output)]), 0)
                        report = json.loads((output / "report.json").read_text())
                        self.assertTrue(report["byte_identical"])
                        self.assertFalse(report["modified_worktree"])
                        self.assertEqual(report["source_date_epoch"], 1234)
                        self.assertEqual(report["revision"], "test-commit")
                        self.assertEqual((output / "inputs/recipe/Containerfile").read_text(), "FROM fixed-digest")
                        self.assertEqual(report["recipe_archive_sha256"],
                                         reproducible.sha256(output / "inputs/recipe.tar"))


if __name__ == "__main__":
    unittest.main()
