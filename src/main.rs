//! Command line entry point for USBGuardGUI.
//!
//! This binary parses arguments and formats output; the logic lives in the
//! library crate so it stays testable without a terminal.

use std::process::ExitCode;

use usbguard_gui::VERSION;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("usbguard-gui {VERSION}");
        return ExitCode::SUCCESS;
    }

    // TODO(template): dispatch to the library here.
    println!("USBGuardGUI v{VERSION}");
    ExitCode::SUCCESS
}
