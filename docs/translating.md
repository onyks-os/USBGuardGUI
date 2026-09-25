<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# Translating USBGuardGUI

The interface uses [GNU gettext](https://www.gnu.org/software/gettext/). Every translation is one
file, `po/<language>.po`, and you need no Rust knowledge to write one.

## What is translated, and what is not

- **Translated**: every text in the window, the notifications, the status icon menu, the
  application's menu entry (`.desktop`), and its software-store description (AppStream metainfo).
- **Not translated, on purpose**:
  - rule-language keywords — `allow`, `block`, `reject`, `with-interface`, `label`, `one-of`, … —
    are syntax: a translated keyword would be a syntax error;
  - the command-line output (`usbguard-gui --diagnose` and friends), which is pasted into bug
    reports and must be readable by everyone;
  - the application name, `USBGuardGUI`, which appears in the strings as `{app}`.

## Start a new language

You need `gettext` (`sudo dnf install gettext`, `sudo apt install gettext`, or
`sudo pacman -S gettext`).

```bash
git clone https://github.com/onyks-os/USBGuardGUI.git && cd USBGuardGUI
make i18n-pot                                   # refresh po/usbguard-gui.pot
msginit -i po/usbguard-gui.pot -o po/fr.po -l fr_FR.UTF-8
echo fr >> po/LINGUAS
```

Then translate `po/fr.po` with any editor — [Poedit](https://poedit.net/),
[GNOME Translation Editor](https://apps.gnome.org/Gtranslator/), [Lokalize](https://apps.kde.org/lokalize/),
or a plain text editor. `po/it.po` is a complete example.

## Rules for each string

- **Keep every `{placeholder}` exactly as written**, but move it wherever your grammar needs it:
  `Rejected “{device}” permanently` may become `“{device}” rifiutato permanentemente`. `msgfmt
  --check` rejects a translation that drops, adds, or renames one.
- **Keep the underscore** in menu entries (`_Preferences`): it marks the keyboard access key. Put it
  before a letter of your translation.
- **Read the `#. Translators:` comments** above a string: they say what it refers to.
- **`translator-credits`** is not a sentence: replace it with your name, to be credited in the
  About dialog.
- Plural forms (`msgstr[0]`, `msgstr[1]`, …) follow your language's rule, which `msginit` writes
  into the header.

## Check and try it

```bash
msgfmt --check --statistics -o /dev/null po/fr.po    # must report no error
make i18n                                            # regenerates data/*.desktop and metainfo
LANGUAGE=fr cargo run                                # the window, in French
```

To see the window with invented devices instead of your own, run `cargo run --example demo-bridge`
in another terminal and start the window with `LANGUAGE=fr USBGUARD_GUI_BUS=session cargo run`.

## Submit

Add `po/<lang>.po`, the new line in `po/LINGUAS`, the regenerated `data/*.desktop` and
`data/*.metainfo.xml`, and one packaging line for your language in `Cargo.toml` (copy the `it`
lines under `[package.metadata.deb]` and `[package.metadata.generate-rpm]`). Open a pull request;
every commit needs a DCO `Signed-off-by` line (`git commit -s`). See [CONTRIBUTING.md](../CONTRIBUTING.md).

## Keeping a translation current

When the interface changes, a maintainer runs `make i18n-update`, which merges new strings into
every `.po` file and marks changed ones *fuzzy*. Translate the new and fuzzy entries, remove the
`#, fuzzy` marker, and submit again.
