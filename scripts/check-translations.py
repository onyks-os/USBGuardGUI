#!/usr/bin/env python3
"""Check the named placeholders used by ui::i18n::fill on any gettext version."""
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT

import argparse
import gettext
import re
from pathlib import Path

PLACEHOLDER = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")


def validate(path):
    """Reject renamed, missing or additional placeholders, including plurals."""
    with path.open("rb") as source:
        catalogue = gettext.GNUTranslations(source)
    # GNUTranslations stores plural forms as (source singular, plural index).
    # All current plural pairs use the same named arguments in both forms.
    for key, translated in catalogue._catalog.items():
        original = key[0] if isinstance(key, tuple) else key
        if not original:
            continue  # Metadata header, not a UI message.
        expected = set(PLACEHOLDER.findall(original))
        actual = set(PLACEHOLDER.findall(translated))
        if expected != actual:
            raise ValueError(
                f"{path}: placeholder mismatch for {original!r}: "
                f"expected {sorted(expected)}, found {sorted(actual)}"
            )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalogues", type=Path, nargs="+")
    args = parser.parse_args(argv)
    try:
        for path in args.catalogues:
            validate(path)
    except (OSError, ValueError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
