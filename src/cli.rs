//! The command line (docs/interfaces.md §1, §3).
//!
//! Headless commands only: each one runs, prints to stdout, and exits. The
//! window is started by `main.rs` when no command is given.

use std::fmt::Write as _;
use std::process::ExitCode;

use crate::dbus::Client;
use crate::dbus::diagnostics::{self, ApiLevel, ProbeReport};
use crate::device_store::port_key;
use crate::model::{AccessState, Device, RuleHandle, Target};
use crate::remedy::{Distro, remedy};
use crate::runtime::runtime;

/// Exit code for invalid usage (docs/interfaces.md §3).
pub const EXIT_USAGE: u8 = 2;

/// What the command line asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// No command: start the window.
    Gui {
        /// `--background`: start without presenting the window.
        background: bool,
    },
    /// `--help`
    Help,
    /// `--version`
    Version,
    /// `--diagnose`
    Diagnose,
    /// `--list-devices`
    ListDevices,
    /// `--list-rules`
    ListRules,
}

/// Parses the arguments (without the program name).
///
/// # Errors
///
/// The offending argument, for the usage message.
pub fn parse_args<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = None;
    let mut background = false;
    for arg in args {
        let next = match arg.as_ref() {
            "-h" | "--help" => Command::Help,
            "-V" | "--version" => Command::Version,
            "--diagnose" => Command::Diagnose,
            "--list-devices" => Command::ListDevices,
            "--list-rules" => Command::ListRules,
            "--background" => {
                background = true;
                continue;
            }
            other => return Err(other.to_owned()),
        };
        if command.replace(next).is_some() {
            return Err(arg.as_ref().to_owned());
        }
    }
    match (command, background) {
        (None, background) => Ok(Command::Gui { background }),
        (Some(_), true) => Err("--background".to_owned()),
        (Some(c), false) => Ok(c),
    }
}

/// The `--help` text.
#[must_use]
pub fn usage() -> String {
    format!(
        "usbguard-gui {version}
Unprivileged desktop client for USBGuard.

Usage: usbguard-gui [OPTION]

  (no option)       open the window
  --background      start without presenting the window
  --diagnose        check access to the USBGuard daemon and print the result
  --list-devices    print the devices the daemon knows
  --list-rules      print the ruleset in evaluation order
  -V, --version     print the version and the USBGuard API level
  -h, --help        print this help

Logging: USBGUARD_GUI_LOG=<filter> (default: warn). The debug level records
device names and serial numbers.
",
        version = crate::VERSION
    )
}

/// Runs a headless command. [`Command::Gui`] is the caller's business.
#[must_use]
pub fn run(command: Command) -> ExitCode {
    match command {
        Command::Gui { .. } | Command::Help => {
            print!("{}", usage());
            ExitCode::SUCCESS
        }
        Command::Version => {
            let api = runtime().block_on(async {
                let conn = zbus::Connection::system().await.ok()?;
                diagnostics::api_level(&conn).await
            });
            println!("usbguard-gui {}", crate::VERSION);
            println!("{}", describe_api(api));
            ExitCode::SUCCESS
        }
        Command::Diagnose => {
            let report = runtime().block_on(diagnostics::probe());
            print!("{}", format_report(&report, Distro::detect()));
            ExitCode::from(diagnostics::exit_code(&report.state))
        }
        Command::ListDevices => {
            list(|client| async move { client.list_devices().await.map(|d| format_devices(&d)) })
        }
        Command::ListRules => {
            list(|client| async move { client.list_rules().await.map(|r| format_rules(&r)) })
        }
    }
}

fn list<F, Fut>(f: F) -> ExitCode
where
    F: FnOnce(Client) -> Fut,
    Fut: Future<Output = Result<String, crate::model::AppError>>,
{
    let result = runtime().block_on(async {
        let client = Client::connect_system().await?;
        f(client).await
    });
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("usbguard-gui: {err}");
            eprintln!("Run `usbguard-gui --diagnose` for the cause and the remedy.");
            ExitCode::FAILURE
        }
    }
}

fn describe_api(api: Option<ApiLevel>) -> &'static str {
    match api {
        Some(ApiLevel::AtLeast1_1) => {
            "USBGuard D-Bus API: 1.1.0 or later (runtime-only rules supported)"
        }
        Some(ApiLevel::Before1_1) => {
            "USBGuard D-Bus API: older than 1.1.0 (runtime-only rules not supported)"
        }
        None => "USBGuard D-Bus API: not reachable (see --diagnose)",
    }
}

fn yes_no(value: Option<bool>, yes: &str, no: &str) -> String {
    match value {
        Some(true) => yes.to_owned(),
        Some(false) => no.to_owned(),
        None => "unknown".to_owned(),
    }
}

/// Formats a probe report for `--diagnose`.
#[must_use]
pub fn format_report(report: &ProbeReport, distro: Distro) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "USBGuard GUI {} — access diagnostics\n",
        crate::VERSION
    );
    let line = |out: &mut String, label: &str, value: &str| {
        let _ = writeln!(out, "  {label:<28}{value}");
    };
    let bus = if report.state == AccessState::BusUnavailable {
        "unavailable"
    } else {
        "ok"
    };
    line(&mut out, "System bus", bus);
    let bridge = match (report.bridge_owned, report.bridge_activatable) {
        (Some(true), _) => "running".to_owned(),
        (Some(false), Some(true)) => "installed, started on demand".to_owned(),
        (Some(false), Some(false)) => "not installed".to_owned(),
        _ => "unknown".to_owned(),
    };
    line(&mut out, "Bridge (org.usbguard1)", &bridge);
    line(
        &mut out,
        "usbguard.service",
        &yes_no(report.daemon_running, "active", "not active"),
    );
    line(
        &mut out,
        "Polkit agent",
        &yes_no(report.agent_present, "detected", "none detected"),
    );
    let read = report.implicit_policy_target.as_ref().map_or_else(
        || "no".to_owned(),
        |v| format!("ok (ImplicitPolicyTarget = {v})"),
    );
    line(&mut out, "Read access", &read);
    line(
        &mut out,
        "Write access",
        "not probed — the only test of a write is a write",
    );
    if let Some(api) = report.api {
        line(
            &mut out,
            "API level",
            describe_api(Some(api)).trim_start_matches("USBGuard D-Bus API: "),
        );
    }
    let _ = writeln!(out, "\nResult: {}", report.state);
    if let Some(r) = remedy(&report.state, distro) {
        let _ = writeln!(out, "\n{}", r.explanation);
        for command in &r.commands {
            let _ = writeln!(out, "  $ {command}");
        }
    }
    out
}

const fn state_label(target: Target) -> &'static str {
    match target {
        Target::Allow => "allowed",
        Target::Block => "blocked",
        Target::Reject => "rejected",
        Target::Other(_) => "unknown",
    }
}

/// Formats devices as a table, sorted by port (§9.2: `listDevices` order is
/// not stable, sorting is the client's job).
#[must_use]
pub fn format_devices(devices: &[Device]) -> String {
    let mut sorted: Vec<&Device> = devices.iter().collect();
    sorted.sort_by(|a, b| {
        port_key(a.attrs.via_port.as_deref())
            .cmp(&port_key(b.attrs.via_port.as_deref()))
            .then(a.id.cmp(&b.id))
    });
    let dash = |v: Option<&str>| match v {
        Some(s) if !s.trim().is_empty() => s.trim().to_owned(),
        _ => "—".to_owned(),
    };
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<5} {:<9} {:<10} {:<8} {:<28} SERIAL",
        "ID", "STATE", "VID:PID", "PORT", "NAME"
    );
    for d in sorted {
        let _ = writeln!(
            out,
            "{:<5} {:<9} {:<10} {:<8} {:<28} {}",
            d.id,
            state_label(d.target),
            d.attrs
                .usb_id
                .map_or_else(|| "—".to_owned(), |id| id.to_string()),
            dash(d.attrs.via_port.as_deref()),
            dash(d.attrs.name.as_deref()),
            dash(d.attrs.serial.as_deref()),
        );
        if let Some(err) = d.parse_error {
            let _ = writeln!(out, "      (could not parse: {err}) {}", d.rule_text);
        }
    }
    out
}

/// Formats the ruleset, numbered in evaluation order (never re-sorted, §9.3).
#[must_use]
pub fn format_rules(rules: &[RuleHandle]) -> String {
    if rules.is_empty() {
        return "The ruleset is empty.\n".to_owned();
    }
    let mut out = String::new();
    for r in rules {
        let _ = writeln!(out, "{:>4}. [id {}] {}", r.position + 1, r.id, r.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Command, format_devices, format_report, format_rules, parse_args};
    use crate::dbus::client::to_handles;
    use crate::dbus::diagnostics::ProbeReport;
    use crate::model::{AccessState, DeviceId};
    use crate::remedy::Distro;
    use crate::rules::parse_device;

    #[test]
    fn argument_parsing() {
        assert_eq!(
            parse_args::<[&str; 0], _>([]),
            Ok(Command::Gui { background: false })
        );
        assert_eq!(
            parse_args(["--background"]),
            Ok(Command::Gui { background: true })
        );
        assert_eq!(parse_args(["--diagnose"]), Ok(Command::Diagnose));
        assert_eq!(parse_args(["-V"]), Ok(Command::Version));
        assert_eq!(parse_args(["--bogus"]), Err("--bogus".to_owned()));
        assert_eq!(
            parse_args(["--diagnose", "--version"]),
            Err("--version".to_owned())
        );
        assert_eq!(
            parse_args(["--diagnose", "--background"]),
            Err("--background".to_owned())
        );
    }

    #[test]
    fn devices_sorted_by_port_with_dashes_for_absent_fields() {
        let devices = vec![
            parse_device(
                DeviceId::new(5),
                r#"allow id 1234:0001 via-port "3-10""#.into(),
            ),
            parse_device(
                DeviceId::new(6),
                r#"block id 1234:0002 name "Key" serial "" via-port "3-2""#.into(),
            ),
            parse_device(DeviceId::new(1), r#"allow via-port "usb1""#.into()),
        ];
        let table = format_devices(&devices);
        let rows: Vec<&str> = table.lines().skip(1).collect();
        assert!(rows[0].starts_with("1 "));
        assert!(rows[1].starts_with("6 "));
        assert!(rows[1].contains("blocked"));
        assert!(rows[2].starts_with("5 "));
        assert!(rows[2].contains('—'));
    }

    #[test]
    fn rules_are_numbered_in_evaluation_order() {
        let text = format_rules(&to_handles(vec![(9, "block".into()), (3, "allow".into())]));
        assert_eq!(text, "   1. [id 9] block\n   2. [id 3] allow\n");
        assert_eq!(format_rules(&[]), "The ruleset is empty.\n");
    }

    #[test]
    fn report_includes_the_remedy() {
        let report = ProbeReport {
            state: AccessState::BridgeNotInstalled,
            bridge_owned: Some(false),
            bridge_activatable: Some(false),
            daemon_running: Some(true),
            agent_present: Some(true),
            api: None,
            implicit_policy_target: None,
        };
        let text = format_report(&report, Distro::Fedora);
        assert!(text.contains("not installed"));
        assert!(text.contains("$ sudo dnf install usbguard-dbus"));
    }
}
