//! Entry point for `usbguard-gui`.
//!
//! This binary parses arguments, sets up logging, and hands over to the
//! library: headless commands to [`usbguard_gui::cli`], the window to the GTK
//! application. The logic lives in the library crate so it stays testable
//! without a terminal or a display.

use std::process::ExitCode;

use tracing_subscriber::{EnvFilter, filter::LevelFilter};
use usbguard_gui::cli::{self, Command};

fn main() -> ExitCode {
    init_logging();

    let command = match cli::parse_args(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(arg) => {
            eprintln!("usbguard-gui: unexpected argument {arg:?}\n");
            eprint!("{}", cli::usage());
            return ExitCode::from(cli::EXIT_USAGE);
        }
    };

    match command {
        Command::Gui { background } => run_gui(background),
        other => cli::run(other),
    }
}

/// `USBGUARD_GUI_LOG` controls verbosity; the default is `warn`. Logs go to
/// stderr so that stdout carries only command output.
fn init_logging() {
    let filter =
        EnvFilter::try_from_env("USBGUARD_GUI_LOG").unwrap_or_else(|_| EnvFilter::new("warn"));
    let verbose = filter
        .max_level_hint()
        .is_none_or(|level| level >= LevelFilter::DEBUG);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
    if verbose {
        // A module-specific filter (for example zbus=trace) may suppress this
        // binary's own debug events while enabling sensitive dependency logs.
        eprintln!(
            "usbguard-gui: Privacy notice: debug and trace logs may contain device names, serial numbers, hashes, and daemon error details. Review them before sharing."
        );
    }
}

#[cfg(feature = "gui")]
fn run_gui(background: bool) -> ExitCode {
    usbguard_gui::ui::run(background)
}

#[cfg(not(feature = "gui"))]
fn run_gui(_background: bool) -> ExitCode {
    eprintln!(
        "usbguard-gui: this build has no graphical interface (built without the `gui` \
         feature). The headless commands still work:\n"
    );
    eprint!("{}", cli::usage());
    ExitCode::FAILURE
}
