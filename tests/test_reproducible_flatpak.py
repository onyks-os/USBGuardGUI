"""Flatpak metadata, payload and isolated runtime-verification tests."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


flatpak = module("repro_flatpak", "packaging/reproducible/flatpak.py")
runtime = module("repro_runtime", "scripts/verify-flatpak-runtime.py")


class FlatpakTests(unittest.TestCase):
    def test_manifest_reuses_existing_permissions_and_staging_commands(self):
        data, commands = flatpak.manifest(ROOT)
        self.assertEqual(data["runtime"], "org.gnome.Platform")
        self.assertEqual(data["runtime-version"], "50")
        self.assertIn("--system-talk-name=org.usbguard1", data["finish-args"])
        self.assertFalse(any("filesystem=" in value for value in data["finish-args"]))
        self.assertTrue(commands[0].startswith("install -Dm755"))
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            (source / "packaging").mkdir()
            path = source / "packaging/io.github.onyks_os.UsbguardGui.yaml"
            original = (ROOT / "packaging/io.github.onyks_os.UsbguardGui.yaml").read_text()
            for text in (original.replace("io.github.onyks_os.UsbguardGui", "wrong.Application"),
                         original.replace("cargo --offline fetch", "unexpected fetch")):
                path.write_text(text)
                with self.assertRaises(ValueError):
                    flatpak.manifest(source)

    def test_export_preserves_elf_normalizes_time_and_imports_actual_bundle(self):
        for corrupt in (None, "stage", "import"):
            with self.subTest(corrupt=corrupt), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source, output = root / "source", root / "output"
                source.mkdir()
                (source / "Cargo.toml").write_text('[package]\nversion = "0.1.1"\n')
                (output / "flatpak-bin").mkdir(parents=True)
                binary = output / "flatpak-bin/usbguard-gui"
                binary.write_bytes(b"verified Flatpak ELF")
                data, _ = flatpak.manifest(ROOT)
                calls = []

                def run(command, **kwargs):
                    calls.append(command)
                    build = root / "flatpak/build"
                    if command[0] == "bash":
                        installed = build / "files/bin/usbguard-gui"
                        installed.parent.mkdir()
                        installed.write_bytes(b"wrong" if corrupt == "stage" else binary.read_bytes())
                        self.assertNotIn("target/release/usbguard-gui", command[-1])
                        self.assertIn(str(output / "flatpak-bin/usbguard-gui"), command[-1])
                    elif command[1] == "build-export":
                        self.assertIn("--timestamp=1970-01-01T00:20:34Z", command)
                        self.assertEqual((build / "files/bin/usbguard-gui").stat().st_mtime, 1234)
                    elif command[1] == "build-bundle":
                        self.assertEqual(kwargs["env"]["FAKETIME"], "1970-01-01 00:20:34")
                        self.assertEqual(kwargs["env"]["FAKETIME_DONT_FAKE_MONOTONIC"], "1")
                        (output / "usbguard-gui-0.1.1.flatpak").write_bytes(b"Flatpak bundle")
                    elif "checkout" in command:
                        shutil.copytree(build, output / "flatpak-checkout")
                        if corrupt == "import":
                            (output / "flatpak-checkout/files/bin/usbguard-gui").write_bytes(b"wrong")
                    elif "rev-parse" in command:
                        kwargs["stdout"].write("ostree-commit\n")

                with patch.object(flatpak.Path, "cwd", return_value=source), \
                        patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "1234"}), \
                        patch.object(flatpak, "manifest", return_value=(data, ["install -Dm755 target/release/usbguard-gui /app/bin/usbguard-gui"])), \
                        patch.object(flatpak.subprocess, "run", side_effect=run):
                    if corrupt:
                        with self.assertRaises(ValueError):
                            flatpak.main(output)
                    else:
                        flatpak.main(output)
                        self.assertTrue(any("build-import-bundle" in call for call in calls))
                        self.assertEqual((output / "flatpak-commit.txt").read_text(), "ostree-commit\n")
                        self.assertIn("runtime=org.gnome.Platform/x86_64/50", (output / "flatpak-metadata.txt").read_text())

    def test_runtime_verification_is_private_pinned_and_records_only_success(self):
        for mode, failure in (("local", None), ("remote", None), ("cached", None),
                              ("remote", "commit"), ("remote", "cli"), ("remote", "command")):
            with self.subTest(mode=mode, failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                bundle, lockfile, output = root / "app.flatpak", root / "runtime.json", root / "result"
                bundle.write_bytes(b"bundle")
                lockfile.write_text(json.dumps({"ref": "runtime/org.gnome.Platform/x86_64/50",
                                               "commit": "pinned", "repository_file": "https://flathub.org/repo/flathub.flatpakrepo"}))
                calls = []

                def run(command, **kwargs):
                    calls.append(command)
                    self.assertEqual(kwargs["env"]["FLATPAK_USER_DIR"], str(output / "installation"))
                    self.assertEqual(kwargs["env"]["HOME"], str(output / "home"))
                    text = ""
                    if "info" in command:
                        text = "wrong" if failure == "commit" else "pinned"
                    if "--help" in command:
                        text = "wrong" if failure == "cli" else "Usage: usbguard-gui [OPTION]"
                    return subprocess.CompletedProcess(command, 1 if failure == "command" else 0, text, "details")

                with patch.object(runtime.subprocess, "run", side_effect=run):
                    if failure:
                        with self.assertRaises((ValueError, subprocess.CalledProcessError)):
                            runtime.verify(bundle, output, lockfile)
                        self.assertFalse((output / "report.json").exists())
                        self.assertTrue((output / "commands.log").exists())
                    else:
                        report = runtime.verify(bundle, output, lockfile,
                                                root / "local-repo" if mode == "local" else None,
                                                root / "runtime.flatpak" if mode == "cached" else None)
                        self.assertEqual(report["runtime_commit"], "pinned")
                        if mode == "local":
                            self.assertTrue(any("--runtime" in call for call in calls))
                        if mode == "remote":
                            self.assertTrue(any("--commit=pinned" in call for call in calls))
                        self.assertTrue(any("--bundle" in call for call in calls))
                        self.assertTrue((output / "report.json").exists())
                        with self.assertRaises(FileExistsError):
                            runtime.verify(bundle, output, lockfile)

    def test_runtime_cli_passes_explicit_paths_to_verifier(self):
        with patch.object(runtime, "verify") as verify:
            runtime.main(["app.flatpak", "--output", "results", "--runtime-lock", "lock.json",
                          "--runtime-repository", "local-repo"])
            verify.assert_called_once_with(Path("app.flatpak"), Path("results"), Path("lock.json"), Path("local-repo"), None)

    def test_missing_bundle_fails_before_installation_or_network_access(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(runtime.subprocess, "run") as command:
            root = Path(directory)
            with self.assertRaises(FileNotFoundError):
                runtime.verify(root / "missing.flatpak", root / "result", root / "lock.json")
            command.assert_not_called()
            self.assertFalse((root / "result").exists())
