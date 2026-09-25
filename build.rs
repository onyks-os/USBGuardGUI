//! Build script: compiles the GSettings schema for development runs.
//!
//! An installed package puts the schema in `/usr/share/glib-2.0/schemas/`,
//! where the distribution's triggers compile it. A `cargo run` from the source
//! tree has no installed schema, so the schema is compiled here, into
//! `OUT_DIR`, and the program falls back to that directory at run time
//! (`src/ui/config.rs`). Without `glib-compile-schemas` the build still
//! succeeds; the program then uses built-in defaults.
//!
//! The translations in `po/` are compiled the same way, for the same reason:
//! packages install them under /usr/share/locale (`make i18n`), but a
//! `cargo run` finds them in `OUT_DIR` (`src/ui/i18n.rs`). Without `msgfmt`
//! the interface is simply in English.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=data/io.github.onyks_os.UsbguardGui.gschema.xml");
    println!("cargo:rerun-if-changed=po");
    if std::env::var_os("CARGO_FEATURE_GUI").is_none() {
        return;
    }
    let Some(out_dir) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    compile_translations(&out_dir);
    let schema_dir = out_dir.join("schemas");
    if std::fs::create_dir_all(&schema_dir).is_err()
        || std::fs::copy(
            "data/io.github.onyks_os.UsbguardGui.gschema.xml",
            schema_dir.join("io.github.onyks_os.UsbguardGui.gschema.xml"),
        )
        .is_err()
    {
        println!("cargo:warning=could not stage the GSettings schema");
        return;
    }
    match Command::new("glib-compile-schemas")
        .arg("--strict")
        .arg(&schema_dir)
        .status()
    {
        Ok(status) if status.success() => {
            println!(
                "cargo:rustc-env=USBGUARD_GUI_DEV_SCHEMA_DIR={}",
                schema_dir.display()
            );
        }
        _ => println!(
            "cargo:warning=glib-compile-schemas failed or is missing; settings will not persist in development runs"
        ),
    }
}

/// Compiles every language listed in `po/LINGUAS` into
/// `OUT_DIR/locale/<lang>/LC_MESSAGES/usbguard-gui.mo`.
fn compile_translations(out_dir: &std::path::Path) {
    let Ok(linguas) = std::fs::read_to_string("po/LINGUAS") else {
        return;
    };
    let locale_dir = out_dir.join("locale");
    let languages = linguas
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'));
    for lang in languages {
        let target = locale_dir.join(lang).join("LC_MESSAGES");
        let compiled = std::fs::create_dir_all(&target).is_ok()
            && Command::new("msgfmt")
                .arg("-o")
                .arg(target.join("usbguard-gui.mo"))
                .arg(format!("po/{lang}.po"))
                .status()
                .is_ok_and(|s| s.success());
        if !compiled {
            println!(
                "cargo:warning=msgfmt failed or is missing; the {lang} translation is not built"
            );
            return;
        }
    }
    println!(
        "cargo:rustc-env=USBGUARD_GUI_DEV_LOCALE_DIR={}",
        locale_dir.display()
    );
}
