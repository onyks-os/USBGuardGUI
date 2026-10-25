use std::fs;

use super::{exit_code, polkit_agent_present};
use crate::model::{AccessState, Capabilities};

#[test]
fn exit_codes_match_the_interface_document() {
    assert_eq!(
        exit_code(&AccessState::Ok {
            caps: Capabilities::default()
        }),
        0
    );
    assert_eq!(exit_code(&AccessState::DeniedByIpcAcl), 3);
    assert_eq!(exit_code(&AccessState::BridgeNotInstalled), 4);
    assert_eq!(
        exit_code(&AccessState::BridgeNotRunning {
            daemon_running: None
        }),
        4
    );
    assert_eq!(exit_code(&AccessState::BusUnavailable), 5);
}

#[test]
fn agent_heuristic_reads_comm() {
    let dir = std::env::temp_dir().join(format!("usbguard-gui-proc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("42")).unwrap();
    fs::write(dir.join("42/comm"), "bash\n").unwrap();
    fs::create_dir_all(dir.join("self")).unwrap();
    assert_eq!(polkit_agent_present(&dir), Some(false));
    fs::create_dir_all(dir.join("77")).unwrap();
    fs::write(dir.join("77/comm"), "polkit-kde-auth\n").unwrap();
    assert_eq!(polkit_agent_present(&dir), Some(true));
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(polkit_agent_present(&dir), None);
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.systemd1")]
enum UnitError {
    NoSuchUnit(String),
    AccessDenied(String),
}
struct Manager(std::sync::Arc<std::sync::Mutex<bool>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn get_unit(&self, name: &str) -> Result<zbus::OwnedObjectPath, UnitError> {
        match name {
            "usbguard.service" if *self.0.lock().unwrap() => {
                Ok("/org/freedesktop/systemd1/unit/usbguard"
                    .try_into()
                    .unwrap())
            }
            "usbguard.service" => Err(UnitError::NoSuchUnit("not loaded".into())),
            _ => Err(UnitError::AccessDenied("no access".into())),
        }
    }
}
struct Unit(std::sync::Arc<std::sync::Mutex<String>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn active_state(&self) -> String {
        self.0.lock().unwrap().clone()
    }
}
#[tokio::test]
async fn systemd_probe_distinguishes_active_reloading_missing_and_denied() {
    use std::sync::{Arc, Mutex};
    let bus = crate::testsupport::PrivateBus::start();
    let connection = bus.connect().await;
    let loaded = Arc::new(Mutex::new(true));
    let status = Arc::new(Mutex::new("active".into()));
    connection
        .object_server()
        .at("/org/freedesktop/systemd1", Manager(loaded.clone()))
        .await
        .unwrap();
    connection
        .object_server()
        .at(
            "/org/freedesktop/systemd1/unit/usbguard",
            Unit(status.clone()),
        )
        .await
        .unwrap();
    connection
        .request_name("org.freedesktop.systemd1")
        .await
        .unwrap();
    for (state, expected) in [
        ("active", true),
        ("reloading", true),
        ("inactive", false),
        ("failed", false),
    ] {
        *status.lock().unwrap() = state.into();
        assert_eq!(
            super::unit_active(&connection, "usbguard.service").await,
            Some(expected)
        );
    }
    *loaded.lock().unwrap() = false;
    assert_eq!(
        super::unit_active(&connection, "usbguard.service").await,
        Some(false)
    );
    assert_eq!(
        super::unit_active(&connection, "another.service").await,
        None
    );
    connection
        .release_name("org.freedesktop.systemd1")
        .await
        .unwrap();
    assert_eq!(
        super::unit_active(&connection, "usbguard.service").await,
        None
    );
}
#[test]
fn only_explicit_denials_update_the_access_model() {
    use crate::model::AppError;
    assert_eq!(
        super::state_from_error(&AppError::Denied(AccessState::DeniedByIpcAcl)),
        Some(AccessState::DeniedByIpcAcl)
    );
    assert_eq!(super::state_from_error(&AppError::Cancelled), None);
}
