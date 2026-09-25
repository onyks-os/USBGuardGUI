//! A stand-in for USBGuard's D-Bus bridge on the **session** bus, with
//! invented devices and rules — for screenshots and for working on the
//! interface without touching the real daemon.
//!
//! ```bash
//! cargo run --example demo-bridge                    # terminal 1
//! USBGUARD_GUI_BUS=session cargo run                 # terminal 2
//! ```
//!
//! `USBGUARD_GUI_BUS=session` is honoured by development builds only. Every
//! action succeeds at once, with no password prompt. Fifteen seconds after
//! start, a new device is "plugged in", blocked, to show the notification.
//!
//! Every name, id, serial, and hash below is invented.

// `main` is mostly the invented data set, one device per entry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

#[path = "../tests/support/mod.rs"]
mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use support::{Devices, Policy, Root, State};
use zbus::object_server::SignalEmitter;

fn device(target: &str, id: &str, name: &str, serial: &str, port: &str, ifaces: &str) -> String {
    format!(
        "{target} id {id} serial \"{serial}\" name \"{name}\" hash \"{name}=\" \
         parent-hash \"hub=\" via-port \"{port}\" with-interface {ifaces} \
         with-connect-type \"hotplug\""
    )
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    let devices = vec![
        (
            1,
            device(
                "allow",
                "1d6b:0002",
                "xHCI Host Controller",
                "0000:00:01.0",
                "usb1",
                "09:00:00",
            ),
        ),
        (
            2,
            device(
                "allow",
                "1d6b:0003",
                "xHCI Host Controller",
                "0000:00:01.0",
                "usb2",
                "09:00:00",
            ),
        ),
        (
            3,
            device(
                "allow",
                "1111:0001",
                "Example Keyboard",
                "",
                "1-1",
                "{ 03:01:01 03:00:00 }",
            ),
        ),
        (
            4,
            device("allow", "1111:0002", "Example Mouse", "", "1-2", "03:01:02"),
        ),
        (
            5,
            device(
                "allow",
                "2222:0001",
                "Example Webcam",
                "0001",
                "1-4",
                "{ 0e:01:00 0e:02:00 }",
            ),
        ),
        (
            6,
            device(
                "block",
                "3333:0001",
                "Example USB Drive",
                "EX0000000001",
                "2-1",
                "08:06:50",
            ),
        ),
    ];
    let rules = vec![
        (
            1,
            "allow with-interface equals { 09:00:00 } label \"root hubs\"".to_owned(),
        ),
        (
            2,
            "allow id 1111:0001 serial \"\" label \"keyboard\"".to_owned(),
        ),
        (3, "allow id 1111:0002 label \"mouse\"".to_owned()),
        (
            4,
            "allow id 2222:0001 serial \"0001\" label \"webcam\"".to_owned(),
        ),
        (
            5,
            "reject with-interface all-of { 08:*:* 03:*:* } label \"storage posing as a keyboard\""
                .to_owned(),
        ),
    ];
    let state = Arc::new(Mutex::new(State {
        devices,
        rules,
        next_rule_id: 5,
        parameters: HashMap::from([
            ("ImplicitPolicyTarget".to_owned(), "block".to_owned()),
            ("InsertedDevicePolicy".to_owned(), "apply-policy".to_owned()),
        ]),
        ..State::default()
    }));

    let connection = zbus::connection::Builder::session()?
        .name("org.usbguard1")?
        .serve_at("/org/usbguard1", Root(state.clone()))?
        .serve_at("/org/usbguard1/Devices", Devices(state.clone()))?
        .serve_at("/org/usbguard1/Policy", Policy(state.clone()))?
        .build()
        .await?;
    println!("Demo USBGuard bridge running on the session bus. Start the window with:");
    println!("    USBGUARD_GUI_BUS=session cargo run");
    println!("A new blocked device will be plugged in 15 seconds from now. Ctrl+C to stop.");

    tokio::time::sleep(Duration::from_secs(15)).await;
    let phone = device(
        "block",
        "4444:0001",
        "Example Phone",
        "EXPHONE01",
        "2-2",
        "{ 06:01:01 ff:42:01 }",
    );
    state.lock().unwrap().devices.push((7, phone.clone()));
    let emitter = SignalEmitter::new(&connection, "/org/usbguard1/Devices")?;
    let attrs = HashMap::from([("name".to_owned(), "Example Phone".to_owned())]);
    Devices::device_presence_changed(&emitter, 7, 1, 1, &phone, attrs.clone()).await?;
    Devices::device_policy_changed(&emitter, 7, 1, 1, &phone, 0, attrs).await?;
    println!("Plugged in: Example Phone (blocked).");

    std::future::pending::<()>().await;
    Ok(())
}
