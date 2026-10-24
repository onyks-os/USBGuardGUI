"""Exercise the real cargo-deny policy with disposable dependency graphs."""
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

POLICY = Path(__file__).resolve().parents[1] / "deny.toml"


class DependencyPolicyTests(unittest.TestCase):
    """Forbidden dependencies must fail even when no Rust code is compiled."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="usbguard-policy-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.env = dict(os.environ, CARGO_HOME=str(self.root / "cargo-home"))
        self.package(self.root, "policy-probe", "MIT")

    @staticmethod
    def package(root, name, license_expression):
        (root / "src").mkdir(parents=True)
        (root / "src/lib.rs").write_text("", encoding="utf-8")
        (root / "Cargo.toml").write_text(
            f'[package]\nname = "{name}"\nversion = "0.1.0"\n'
            f'edition = "2021"\nlicense = "{license_expression}"\n',
            encoding="utf-8",
        )

    def command(self, *args):
        return subprocess.run(
            args, cwd=self.root, env=self.env, capture_output=True,
            text=True, timeout=60, check=False,
        )

    def check_policy(self, *checks):
        # Fixtures contain only local paths or a local Git URL: this primes an
        # isolated Cargo home without contacting a registry or compiling code.
        lock = self.command("cargo", "generate-lockfile")
        self.assertEqual(lock.returncode, 0, lock.stderr)
        return self.command(
            "cargo", "deny", "--locked", "--offline", "--color", "never",
            "--config", str(POLICY), "check", *checks,
        )

    def dependency(self, license_expression, section="dependencies"):
        self.package(self.root / "dependency", "policy-dependency", license_expression)
        with (self.root / "Cargo.toml").open("a", encoding="utf-8") as manifest:
            manifest.write(f'\n[{section}]\npolicy-dependency = {{ path = "dependency" }}\n')

    def test_dual_license_with_allowed_choice_passes(self):
        self.dependency("MIT OR GPL-3.0-only")
        result = self.check_policy("licenses", "sources")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_forbidden_license_in_each_dependency_kind_fails(self):
        for section in (
            "dependencies", "dev-dependencies", "build-dependencies",
            "target.'cfg(windows)'.dependencies",
        ):
            with self.subTest(section=section):
                manifest = self.root / "Cargo.toml"
                original = manifest.read_text(encoding="utf-8")
                dependency = self.root / "dependency"
                if dependency.exists():
                    # Reuse the package across dependency-kind subtests.
                    with manifest.open("a", encoding="utf-8") as output:
                        output.write(f'\n[{section}]\npolicy-dependency = {{ path = "dependency" }}\n')
                else:
                    self.dependency("GPL-3.0-only", section)
                result = self.check_policy("licenses")
                self.assertNotEqual(result.returncode, 0, result.stderr)
                self.assertIn("error[rejected]", result.stderr)
                manifest.write_text(original, encoding="utf-8")

    def test_and_expression_requires_every_license(self):
        self.dependency("MIT AND GPL-3.0-only")
        result = self.check_policy("licenses")
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("error[rejected]", result.stderr)

    def test_optional_dependency_is_checked(self):
        self.dependency("GPL-3.0-only")
        manifest = self.root / "Cargo.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8").replace(
                'path = "dependency"', 'path = "dependency", optional = true'
            ), encoding="utf-8",
        )
        result = self.check_policy("licenses")
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("error[rejected]", result.stderr)

    def test_missing_license_fails(self):
        self.dependency("MIT")
        manifest = self.root / "dependency/Cargo.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8").replace('license = "MIT"\n', ""),
            encoding="utf-8",
        )
        result = self.check_policy("licenses")
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("error[unlicensed]", result.stderr)

    def test_unapproved_git_source_fails_even_with_pinned_revision(self):
        dependency = self.root / "git-dependency"
        self.package(dependency, "policy-dependency", "MIT")
        for args in (
            ("init", "--quiet", str(dependency)),
            ("-C", str(dependency), "add", "."),
            ("-C", str(dependency), "-c", "user.name=Policy test", "-c",
             "user.email=policy-test@example.invalid", "-c", "commit.gpgsign=false",
             "-c", "core.hooksPath=/dev/null", "commit", "--quiet", "-m", "Fixture"),
        ):
            result = self.command("git", *args)
            self.assertEqual(result.returncode, 0, result.stderr)
        revision = self.command("git", "-C", str(dependency), "rev-parse", "HEAD")
        self.assertEqual(revision.returncode, 0, revision.stderr)
        with (self.root / "Cargo.toml").open("a", encoding="utf-8") as manifest:
            manifest.write(
                f'\n[dependencies]\npolicy-dependency = {{ git = "{dependency.as_uri()}", '
                f'rev = "{revision.stdout.strip()}" }}\n'
            )
        result = self.check_policy("sources")
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("source-not-allowed", result.stderr)


if __name__ == "__main__":
    unittest.main()
