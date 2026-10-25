use super::{Distro, remedy};
use crate::model::{AccessState, Capabilities};

#[test]
fn distro_detection() {
    assert_eq!(Distro::from_os_release("ID=fedora\n"), Distro::Fedora);
    assert_eq!(
        Distro::from_os_release("ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n"),
        Distro::Debian
    );
    assert_eq!(
        Distro::from_os_release("ID=endeavouros\nID_LIKE=arch\n"),
        Distro::Arch
    );
    assert_eq!(
        Distro::from_os_release("ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n"),
        Distro::Fedora
    );
    assert_eq!(Distro::from_os_release("ID=nixos\n"), Distro::Unknown);
    assert_eq!(Distro::from_os_release(""), Distro::Unknown);
}

#[test]
fn not_installed_and_not_running_have_different_remedies() {
    // T8/T9: the two states must never read the same.
    let a = remedy(&AccessState::BridgeNotInstalled, Distro::Fedora).unwrap();
    let b = remedy(
        &AccessState::BridgeNotRunning {
            daemon_running: Some(true),
        },
        Distro::Fedora,
    )
    .unwrap();
    assert_ne!(a, b);
    assert_eq!(a.commands[0], "sudo dnf install usbguard-dbus");
    assert!(!b.commands.iter().any(|c| c.contains("install")));
}

#[test]
fn stopped_daemon_is_started_first() {
    let r = remedy(
        &AccessState::BridgeNotRunning {
            daemon_running: Some(false),
        },
        Distro::Arch,
    )
    .unwrap();
    assert!(r.commands[0].contains("usbguard.service"));
}

#[test]
fn ok_has_no_remedy() {
    assert!(
        remedy(
            &AccessState::Ok {
                caps: Capabilities::default()
            },
            Distro::Unknown
        )
        .is_none()
    );
}

#[test]
fn every_checkpoint_gives_only_administrative_instructions() {
    for state in [
        AccessState::BusUnavailable,
        AccessState::DeniedByBusPolicy,
        AccessState::DeniedByPolkit { action: None },
        AccessState::DeniedByIpcAcl,
        AccessState::NoPolkitAgent,
        AccessState::DeniedUnattributed {
            detail: "new".into(),
        },
    ] {
        let r = remedy(&state, Distro::Unknown).unwrap();
        assert_ne!(r.explanation, "");
        assert!(r.commands.iter().all(|s| s.starts_with("sudo ")));
    }
    assert!(Distro::Debian.install_bridge().contains("apt"));
    assert!(Distro::Arch.install_bridge().contains("pacman"));
    assert!(Distro::Unknown.install_bridge().contains("distribution"));
    let path = std::env::temp_dir().join(format!(
        "missing-usbguard-os-release-{}",
        std::process::id()
    ));
    assert_eq!(Distro::from_os_release_file(&path), Distro::Unknown);
    std::fs::write(&path, "ID='ubuntu'\n").unwrap();
    assert_eq!(Distro::from_os_release_file(&path), Distro::Debian);
    std::fs::remove_file(path).unwrap();
}
