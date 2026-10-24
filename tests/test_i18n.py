"""Check real gettext catalogues and their package installation declarations."""
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT

import gettext
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[1]


class TranslationTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="usbguard-i18n-")
        self.addCleanup(self.directory.cleanup)
        self.output = Path(self.directory.name)

    def compile(self, language, source=None):
        destination = self.output / f"{language}.mo"
        result = subprocess.run(
            ["msgfmt", "--check", "--check-format", "-o", str(destination),
             str(source or ROOT / f"po/{language}.po")],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        with destination.open("rb") as catalogue:
            return gettext.GNUTranslations(catalogue)

    def test_registered_catalogues_compile_and_are_packaged(self):
        package = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["metadata"]
        deb = {asset[0]: asset[1] for asset in package["deb"]["assets"]}
        rpm = {asset["source"]: asset["dest"] for asset in package["generate-rpm"]["assets"]}
        languages = [line for line in (ROOT / "po/LINGUAS").read_text().splitlines()
                     if line and not line.startswith("#")]
        self.assertIn("fr", languages)
        for language in languages:
            with self.subTest(language=language):
                self.compile(language)
                source = f"target/locale/{language}/LC_MESSAGES/usbguard-gui.mo"
                self.assertEqual(deb[source], f"usr/share/locale/{language}/LC_MESSAGES/")
                self.assertEqual(rpm[source], f"/usr/share/locale/{language}/LC_MESSAGES/usbguard-gui.mo")

    def test_french_covers_current_template_without_fuzzy_fallback(self):
        result = subprocess.run(
            ["msgcmp", "--no-fuzzy-matching", str(ROOT / "po/fr.po"),
             str(ROOT / "po/usbguard-gui.pot")],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_french_plural_counts_and_placeholders(self):
        catalogue = self.compile("fr")
        for count in (0, 1, 2, 40):
            with self.subTest(count=count):
                translated = catalogue.ngettext(
                    "{count} USB device blocked", "{count} USB devices blocked", count)
                expected = "périphérique USB bloqué" if count < 2 else "périphériques USB bloqués"
                self.assertEqual(translated.format(count=count), f"{count} {expected}")

    def test_french_preserves_security_meaning_and_rule_keywords(self):
        translate = self.compile("fr").gettext
        self.assertEqual(translate("Block"), "Bloquer")
        self.assertEqual(translate("Reject"), "Rejeter")
        self.assertEqual(translate("This session only"), "Pour cette session uniquement")
        self.assertEqual(translate("Permanently"), "De façon permanente")
        self.assertIn("allow, block ou reject", translate("Expected allow, block, or reject"))
        message = translate("USBGuard refused the request: {detail}")
        self.assertEqual(message.format(detail="example"), "USBGuard a refusé la requête : example")

    def test_gettext_rejects_renamed_format_placeholder(self):
        source = (ROOT / "po/fr.po").read_text()
        original = 'msgstr "Non autorisé : {reason}"'
        self.assertIn(original, source)
        invalid = self.output / "invalid.po"
        invalid.write_text(source.replace(original, 'msgstr "Non autorisé : {cause}"'))
        result = subprocess.run(
            ["msgfmt", "--check", "--check-format", "-o", str(self.output / "invalid.mo"), str(invalid)],
            capture_output=True, text=True, check=False,
        )
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("cause", result.stderr)

    def test_french_desktop_and_store_metadata(self):
        desktop = (ROOT / "data/io.github.onyks_os.UsbguardGui.desktop").read_text()
        self.assertIn("GenericName[fr]=Politique des périphériques USB", desktop)
        metadata = ET.parse(ROOT / "data/io.github.onyks_os.UsbguardGui.metainfo.xml")
        namespace = "{http://www.w3.org/XML/1998/namespace}lang"
        summaries = [item.text for item in metadata.findall("summary") if item.get(namespace) == "fr"]
        self.assertEqual(summaries, ["Contrôler quels périphériques USB se connectent"])
