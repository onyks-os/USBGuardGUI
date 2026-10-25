//! The installed entry point and single-instance actions on an isolated desktop.
#![cfg(feature = "gui")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{Mock, PrivateBus, State};
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the isolated display/configuration from scripts/test-gui.sh"]
async fn gui_lifecycle() {
    let bus = PrivateBus::start();
    let _mock = Mock::serve_on_bus(
        &bus,
        Arc::new(Mutex::new(State {
            parameters: [
                ("ImplicitPolicyTarget".into(), "block".into()),
                ("InsertedDevicePolicy".into(), "apply-policy".into()),
            ]
            .into(),
            devices: vec![(3, support::device_rule("block", 3))],
            ..State::default()
        })),
    )
    .await;
    let connection = bus.connect().await;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_usbguard-gui"))
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .env("DBUS_SYSTEM_BUS_ADDRESS", &bus.address)
        .env("USBGUARD_GUI_BUS", "session")
        .arg("--background")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let name = zbus::names::BusName::try_from("io.github.onyks_os.UsbguardGui").unwrap();
    while !dbus.name_has_owner(name.clone()).await.unwrap() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("GUI did not register on the private bus");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let actions = zbus::Proxy::new(
        &connection,
        "io.github.onyks_os.UsbguardGui",
        "/io/github/onyks_os/UsbguardGui",
        "org.gtk.Actions",
    )
    .await
    .unwrap();
    let platform = std::collections::HashMap::<String, zbus::OwnedVariant>::new();
    // A second invocation activates the existing window, rather than a new instance.
    let second = Command::new(env!("CARGO_BIN_EXE_usbguard-gui"))
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .env("DBUS_SYSTEM_BUS_ADDRESS", &bus.address)
        .env("USBGUARD_GUI_BUS", "session")
        .output()
        .unwrap();
    assert!(second.status.success());
    actions
        .call::<_, _, ()>(
            "Activate",
            &("show-devices", Vec::<zbus::OwnedVariant>::new(), &platform),
        )
        .await
        .unwrap();
    actions
        .call::<_, _, ()>(
            "Activate",
            &(
                "allow-device",
                vec![zbus::OwnedVariant::from(999_u32)],
                &platform,
            ),
        )
        .await
        .unwrap();
    actions
        .call::<_, _, ()>(
            "Activate",
            &("quit", Vec::<zbus::OwnedVariant>::new(), &platform),
        )
        .await
        .unwrap();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("GUI did not quit");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
