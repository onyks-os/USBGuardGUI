//! Real executable commands against a disposable bus, never the host daemon.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use support::{Mock, PrivateBus, State};

fn command(bus: &PrivateBus, argument: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_usbguard-gui"))
        .env("DBUS_SYSTEM_BUS_ADDRESS", &bus.address)
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .env_remove("USBGUARD_GUI_BUS")
        .arg(argument)
        .output()
        .unwrap()
}
fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn executable_commands_report_success_denial_and_missing_service() {
    let bus = PrivateBus::start();
    for arg in ["-h", "--help", "-V", "--version"] {
        assert!(command(&bus, arg).status.success());
    }
    let usage = command(&bus, "--not-an-option");
    assert_eq!(usage.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&usage.stderr).contains("unexpected argument"));
    let diagnose = command(&bus, "--diagnose");
    assert_eq!(diagnose.status.code(), Some(4));
    assert!(stdout(&diagnose).contains("not installed"));
    for arg in ["--list-devices", "--list-rules"] {
        assert_eq!(command(&bus, arg).status.code(), Some(1));
    }
    let state = Arc::new(Mutex::new(State {
        devices: vec![
            (1, "reject name \"Broken\"".into()),
            (2, "allow name \"unterminated".into()),
        ],
        rules: vec![(1, "block".into())],
        parameters: [("ImplicitPolicyTarget".into(), "block".into())].into(),
        ..State::default()
    }));
    let _mock = Mock::serve_on_bus(&bus, state.clone()).await;
    assert!(stdout(&command(&bus, "--version")).contains("1.1.0 or later"));
    let devices = command(&bus, "--list-devices");
    assert!(devices.status.success());
    assert!(stdout(&devices).contains("rejected"));
    assert!(stdout(&devices).contains("could not parse"));
    assert!(stdout(&command(&bus, "--list-rules")).contains("[id 1] block"));
    let report = command(&bus, "--diagnose");
    assert_eq!(report.status.code(), Some(0));
    assert!(stdout(&report).contains("Result: connected"));
    state.lock().unwrap().denied.insert("getParameter");
    assert_eq!(command(&bus, "--diagnose").status.code(), Some(3));
    state.lock().unwrap().denied.insert("listRules");
    assert_eq!(command(&bus, "--list-rules").status.code(), Some(1));
}
#[test]
fn missing_bus_has_a_specific_exit_code() {
    let out = Command::new(env!("CARGO_BIN_EXE_usbguard-gui"))
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            "unix:path=/tmp/usbguard-gui-no-such-bus",
        )
        .env_remove("USBGUARD_GUI_BUS")
        .arg("--diagnose")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(stdout(&out).contains("no system D-Bus"));
}

#[cfg(not(feature = "gui"))]
#[test]
fn headless_build_explains_the_missing_graphical_feature() {
    let out = Command::new(env!("CARGO_BIN_EXE_usbguard-gui"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no graphical interface"));
}
