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

#[test]
fn reports_distinguish_activated_unknown_and_inactive_states() {
    use crate::dbus::diagnostics::ApiLevel;
    for (owned, activatable, state, expected) in [
        (
            Some(false),
            Some(true),
            AccessState::BridgeNotRunning {
                daemon_running: Some(false),
            },
            "installed, started on demand",
        ),
        (None, None, AccessState::BusUnavailable, "unknown"),
    ] {
        let report = ProbeReport {
            state,
            bridge_owned: owned,
            bridge_activatable: activatable,
            daemon_running: Some(false),
            agent_present: Some(false),
            api: Some(ApiLevel::Before1_1),
            implicit_policy_target: None,
        };
        let text = format_report(&report, Distro::Unknown);
        assert!(text.contains(expected));
        assert!(text.contains("not active"));
        assert!(text.contains("none detected"));
        assert!(text.contains("older than 1.1.0"));
    }
    assert_eq!(super::run(Command::Help), std::process::ExitCode::SUCCESS);
    assert_eq!(
        super::run(Command::Gui { background: false }),
        std::process::ExitCode::SUCCESS
    );
}

#[test]
fn unknown_device_state_is_never_reported_as_allowed() {
    let mut device = parse_device(DeviceId::new(1), "block".into());
    device.target = crate::model::Target::Other(98);
    assert!(format_devices(&[device]).contains("unknown"));
}
