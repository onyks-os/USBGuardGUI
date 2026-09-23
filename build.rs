//! Build script: compiles the GSettings schema for development runs.
//!
//! An installed package puts the schema in `/usr/share/glib-2.0/schemas/`,
//! where the distribution's triggers compile it. A `cargo run` from the source
//! tree has no installed schema, so the schema is compiled here, into
//! `OUT_DIR`, and the program falls back to that directory at run time
//! (`src/ui/config.rs`). Without `glib-compile-schemas` the build still
//! succeeds; the program then uses built-in defaults.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=data/io.github.onyks_os.UsbguardGui.gschema.xml");
    if std::env::var_os("CARGO_FEATURE_GUI").is_none() {
        return;
    }
    let Some(out_dir) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
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
