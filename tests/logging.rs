//! Privacy regressions for ordinary logs and the explicit debug opt-in.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use support::Mock;
use tracing::instrument::WithSubscriber;
use usbguard_gui::dbus::supervisor::{ServiceWait, supervise};
use usbguard_gui::model::{AppError, DeviceId, DevicePolicy, Persistence};

#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for LogBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const IDENTIFIERS: [&str; 3] = [
    "PRIVATE_DEVICE_NAME",
    "PRIVATE_SERIAL_9281",
    "PRIVATE_HASH_72=",
];

async fn exercise_logs(level: tracing::Level) -> String {
    let output = LogBuffer::default();
    let writer = output.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_ansi(false)
        .without_time()
        .with_writer(move || writer.clone())
        .finish();
    async {
        let text = format!(
            "block id 1234:5678 name \"{}\" serial \"{}\" hash \"{}\"",
            IDENTIFIERS[0], IDENTIFIERS[1], IDENTIFIERS[2]
        );
        let mock = Mock::start(vec![(3, text.clone())], vec![(1, text.clone())]).await;
        let client = mock.client().await;
        client.list_devices().await.unwrap();
        client.list_rules().await.unwrap();
        client
            .apply_device_policy(
                DeviceId::new(3),
                DevicePolicy::Allow,
                Persistence::RuntimeOnly,
            )
            .await
            .unwrap();
        // A bridge can include any part of a device descriptor in an error.
        mock.state
            .lock()
            .unwrap()
            .failures
            .insert("listDevices", text);
        let err = client.list_devices().await.unwrap_err();
        assert!(matches!(err, AppError::Rejected(_)));
        let (tx, rx) = async_channel::bounded(8);
        supervise(
            || async { Err(err.clone()) },
            |_| async {
                rx.close();
                ServiceWait::TimedOut
            },
            tx,
        )
        .await;
    }
    .with_subscriber(subscriber)
    .await;
    let bytes = output.0.lock().unwrap().clone();
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn default_and_info_logs_do_not_disclose_device_identifiers() {
    for level in [tracing::Level::WARN, tracing::Level::INFO] {
        let log = exercise_logs(level).await;
        assert!(
            log.contains("connection to USBGuard lost"),
            "the logger must be exercised"
        );
        for identifier in IDENTIFIERS {
            assert!(
                !log.contains(identifier),
                "{level} disclosed {identifier}: {log}"
            );
        }
    }
}

#[tokio::test]
async fn debug_logs_retain_details_for_explicit_diagnostics() {
    let log = exercise_logs(tracing::Level::DEBUG).await;
    for identifier in IDENTIFIERS {
        assert!(
            log.contains(identifier),
            "missing diagnostic detail: {identifier}"
        );
    }
}

#[test]
fn debug_output_warns_about_identifiers_and_default_output_does_not() {
    for filter in [
        None,
        Some("warn"),
        Some("info"),
        Some("debug"),
        Some("trace"),
        Some("usbguard_gui=debug"),
        Some("zbus=trace"),
    ] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_usbguard-gui"));
        command.arg("--help").env_remove("USBGUARD_GUI_LOG");
        if let Some(filter) = filter {
            command.env("USBGUARD_GUI_LOG", filter);
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        let verbose = matches!(
            filter,
            Some("debug" | "trace" | "usbguard_gui=debug" | "zbus=trace")
        );
        assert_eq!(
            stderr.contains("Privacy notice:"),
            verbose,
            "filter={filter:?}: {stderr}"
        );
        if verbose {
            assert!(stderr.contains("Review them before sharing"));
        }
    }
}
