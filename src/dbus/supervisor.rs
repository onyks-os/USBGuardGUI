//! Supervision and reconnection (docs/architecture.md §5.5).
//!
//! A worker that exits on its first error leaves the window silently frozen on
//! stale state — the worst failure mode for a security tool. The supervisor
//! restarts it, waiting for the bridge to come back rather than polling, and
//! tells the interface to discard both caches on every reconnection.

use std::future::Future;
use std::time::Duration;

use async_channel::Sender;
use futures_util::StreamExt;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use zbus::Connection;
use zbus::fdo::DBusProxy;

use super::client::Client;
use super::worker::{WorkerExit, event_loop};
use crate::model::{AppError, UiEvent};
use crate::runtime::runtime;

/// The bridge's well-known name.
pub const BRIDGE_NAME: &str = "org.usbguard1";

/// First retry delay, and the delay after the service reappears.
pub const BACKOFF_MIN: Duration = Duration::from_millis(500);
/// Longest wait between retries while the service stays absent.
pub const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Result of waiting for the bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceWait {
    /// The name gained an owner.
    Appeared,
    /// The wait timed out.
    TimedOut,
}

/// Starts supervising the system bus on the shared runtime.
pub fn start(ui_tx: Sender<UiEvent>) -> JoinHandle<()> {
    runtime().spawn(supervise(
        Client::connect_system,
        |max| async move {
            if let Ok(connection) = Connection::system().await {
                wait_for_bridge(&connection, max).await
            } else {
                tokio::time::sleep(max).await;
                ServiceWait::TimedOut
            }
        },
        ui_tx,
    ))
}

/// The supervision loop, generic over how to connect and how to wait, so the
/// tests can drive it against the mock daemon.
pub async fn supervise<C, CF, W, WF>(connect: C, wait: W, ui_tx: Sender<UiEvent>)
where
    C: Fn() -> CF + Send,
    CF: Future<Output = Result<Client, AppError>> + Send,
    W: Fn(Duration) -> WF + Send,
    WF: Future<Output = ServiceWait> + Send,
{
    let mut backoff = BACKOFF_MIN;
    let mut ever_connected = false;

    loop {
        let lost = match connect().await {
            Ok(client) => {
                if ever_connected {
                    // Everything observed before the disconnection is void.
                    if ui_tx.send(UiEvent::Reconnected).await.is_err() {
                        return;
                    }
                }
                ever_connected = true;
                match event_loop(&client, &ui_tx).await {
                    WorkerExit::UiClosed => return,
                    WorkerExit::Lost(err) => err,
                }
            }
            Err(err) => err,
        };

        warn!("connection to USBGuard lost: {lost}");
        if ui_tx
            .send(UiEvent::ConnectionLost(lost.to_string()))
            .await
            .is_err()
        {
            return;
        }

        match wait(backoff).await {
            ServiceWait::Appeared => {
                info!("USBGuard bridge is back");
                backoff = BACKOFF_MIN;
            }
            ServiceWait::TimedOut => backoff = (backoff * 2).min(BACKOFF_MAX),
        }
    }
}

/// Waits until `org.usbguard1` has an owner, or `max` elapses. Subscribes to
/// `NameOwnerChanged` first and checks the current owner second, so an owner
/// appearing in between is not missed. A service absent for an hour costs no
/// traffic at all.
pub async fn wait_for_bridge(connection: &Connection, max: Duration) -> ServiceWait {
    let appeared = async {
        let dbus = DBusProxy::new(connection).await.ok()?;
        let mut changes = dbus
            .receive_name_owner_changed_with_args(&[(0, BRIDGE_NAME)])
            .await
            .ok()?;
        let name = zbus::names::BusName::try_from(BRIDGE_NAME).ok()?;
        if dbus.name_has_owner(name).await.ok()? {
            return Some(());
        }
        while let Some(signal) = changes.next().await {
            if signal.args().is_ok_and(|a| a.new_owner().is_some()) {
                return Some(());
            }
        }
        None
    };
    match tokio::time::timeout(max, appeared).await {
        Ok(Some(())) => ServiceWait::Appeared,
        Ok(None) => {
            // The watch itself failed: behave like a plain timed wait.
            tokio::time::sleep(max).await;
            ServiceWait::TimedOut
        }
        Err(_) => ServiceWait::TimedOut,
    }
}
