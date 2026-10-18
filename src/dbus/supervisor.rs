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
use tracing::{debug, info, warn};
use zbus::Connection;
use zbus::fdo::DBusProxy;

use super::client::Client;
use super::worker::{WorkerExit, event_loop_with_ready};
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
    /// The name was already owned when the wait began.
    Present,
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
            if let Ok(connection) = super::bus::connect().await {
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
                let mut ready = false;
                let exit = event_loop_with_ready(&client, &ui_tx, ever_connected, || {
                    ready = true;
                })
                .await;
                if ready {
                    ever_connected = true;
                    backoff = BACKOFF_MIN;
                }
                match exit {
                    WorkerExit::UiClosed => return,
                    WorkerExit::Lost(err) => err,
                }
            }
            Err(err) => err,
        };

        warn!("connection to USBGuard lost; retrying");
        debug!("connection failure detail: {lost}");
        if ui_tx
            .send(UiEvent::ConnectionLost(lost.to_string()))
            .await
            .is_err()
        {
            return;
        }

        let retry_started = tokio::time::Instant::now();
        let waiting = wait(backoff);
        let status = tokio::select! {
            () = ui_tx.closed() => return,
            status = waiting => status,
        };
        // Name ownership alone does not mean the bridge can serve requests.
        // An already-owned but failing service must observe exponential backoff;
        // a genuine appearance wakes a disconnected client within the minimum.
        let delay = if status == ServiceWait::Appeared {
            BACKOFF_MIN
        } else {
            backoff
        };
        tokio::select! {
            () = ui_tx.closed() => return,
            () = tokio::time::sleep_until(retry_started + delay) => {}
        }
        if status == ServiceWait::Appeared {
            info!("USBGuard bridge is back");
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
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
            return Some(ServiceWait::Present);
        }
        while let Some(signal) = changes.next().await {
            if signal.args().is_ok_and(|a| a.new_owner().is_some()) {
                return Some(ServiceWait::Appeared);
            }
        }
        None
    };
    match tokio::time::timeout(max, appeared).await {
        Ok(Some(status)) => status,
        Ok(None) => {
            // The watch itself failed: behave like a plain timed wait.
            tokio::time::sleep(max).await;
            ServiceWait::TimedOut
        }
        Err(_) => ServiceWait::TimedOut,
    }
}
