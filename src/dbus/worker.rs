//! The event worker (docs/architecture.md §5.4): one long-lived task owning
//! the signal streams, turning them into [`UiEvent`]s.
//!
//! It subscribes **before** taking the initial snapshot, so anything the daemon
//! emits during `listDevices` is already buffered in a stream rather than lost,
//! and it never waits for `DevicePresent`, which cannot reach a D-Bus client
//! (§2.4.4).

use std::pin::Pin;
use std::time::Instant;

use async_channel::Sender;
use futures_util::stream::{self, Stream, StreamExt};
use tracing::{Instrument as _, debug, debug_span, warn};

use super::client::Client;
use super::coalesce::{Coalescer, DeviceSignal};
use crate::model::{AppError, DeviceAttributes, DeviceEvent, DeviceId, Target, UiEvent};

/// Why the worker stopped. It never stops for any other reason: a worker that
/// silently exits leaves the window showing stale authorization state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerExit {
    /// The interface dropped its receiver: shut down cleanly.
    UiClosed,
    /// The daemon became unreachable, or a call failed.
    Lost(AppError),
}

impl From<AppError> for WorkerExit {
    fn from(e: AppError) -> Self {
        Self::Lost(e)
    }
}

/// One decoded signal of any kind.
#[derive(Debug)]
enum Incoming {
    Device {
        signal: DeviceSignal,
        /// Another client made a permanent change (§5.6).
        invalidates_rules: bool,
    },
    Exception {
        context: String,
        object: String,
        reason: String,
    },
    Parameter {
        name: String,
        value: String,
    },
    /// The bridge left the bus.
    OwnerLost,
}

type IncomingStream = Pin<Box<dyn Stream<Item = Incoming> + Send>>;

async fn send(ui_tx: &Sender<UiEvent>, event: UiEvent) -> Result<(), WorkerExit> {
    ui_tx.send(event).await.map_err(|_| WorkerExit::UiClosed)
}

async fn snapshot(client: &Client, ui_tx: &Sender<UiEvent>) -> Result<(), WorkerExit> {
    let devices = client.list_devices().await?;
    send(ui_tx, UiEvent::DeviceSnapshot(devices)).await
}

async fn flush(coalescer: &mut Coalescer, ui_tx: &Sender<UiEvent>) -> Result<(), WorkerExit> {
    let batch = coalescer.take();
    if batch.is_empty() {
        return Ok(());
    }
    // `.instrument()` rather than `.entered()`: an entered span guard is not
    // `Send` and must not be held across an `.await`.
    let span = debug_span!("coalescing_window", devices = batch.len());
    send(ui_tx, UiEvent::DeviceBatch(batch))
        .instrument(span)
        .await
}

/// Runs until the daemon is lost or the interface goes away. Always returns
/// the reason; the supervisor decides what happens next.
pub async fn event_loop(client: &Client, ui_tx: &Sender<UiEvent>) -> WorkerExit {
    match run(client, ui_tx).await {
        Ok(never) => match never {},
        Err(exit) => exit,
    }
}

/// `Infallible` in the `Ok` position: the loop only ever ends with a reason.
async fn run(
    client: &Client,
    ui_tx: &Sender<UiEvent>,
) -> Result<std::convert::Infallible, WorkerExit> {
    let mut events = subscribe(client).await?;

    snapshot(client, ui_tx).await?;
    // The ruleset may have changed while nobody was listening (§5.5).
    send(ui_tx, UiEvent::InvalidateRuleCache).await?;

    let mut coalescer = Coalescer::new();
    loop {
        let next = match coalescer.budget(Instant::now()) {
            None => events.next().await,
            Some(budget) => {
                if let Ok(item) = tokio::time::timeout(budget, events.next()).await {
                    item
                } else {
                    flush(&mut coalescer, ui_tx).await?;
                    continue;
                }
            }
        };

        let Some(incoming) = next else {
            flush(&mut coalescer, ui_tx).await?;
            return Err(AppError::Unreachable("the signal streams closed".into()).into());
        };

        match incoming {
            Incoming::Device {
                signal,
                invalidates_rules,
            } => {
                coalescer.push(signal, Instant::now());
                if invalidates_rules {
                    send(ui_tx, UiEvent::InvalidateRuleCache).await?;
                }
                if coalescer.needs_resync() {
                    debug!("burst over the resync threshold; taking a snapshot");
                    coalescer.clear();
                    snapshot(client, ui_tx).await?;
                }
            }
            Incoming::Exception {
                context,
                object,
                reason,
            } => {
                // Out of band: never coalesced, never dropped.
                send(
                    ui_tx,
                    UiEvent::DaemonException {
                        context,
                        object,
                        reason,
                    },
                )
                .await?;
                send(ui_tx, UiEvent::InvalidateRuleCache).await?;
            }
            Incoming::Parameter { name, value } => {
                send(ui_tx, UiEvent::ParameterChanged { name, value }).await?;
            }
            Incoming::OwnerLost => {
                flush(&mut coalescer, ui_tx).await?;
                return Err(
                    AppError::Unreachable("the USBGuard bridge left the bus".into()).into(),
                );
            }
        }
    }
}

/// Subscribes to every signal the program uses and merges them into one stream.
#[allow(clippy::too_many_lines)] // a flat list of six subscriptions; splitting it hides the list
async fn subscribe(client: &Client) -> Result<IncomingStream, WorkerExit> {
    let err = |e: zbus::Error| WorkerExit::Lost(super::errors::app_error(&e));
    let devices = client.devices_proxy();
    let root = client.root_proxy();

    let presence = devices
        .receive_device_presence_changed()
        .await
        .map_err(err)?
        .filter_map(|s| async move {
            let a = s
                .args()
                .map_err(|e| warn!("malformed DevicePresenceChanged: {e}"))
                .ok()?;
            Some(Incoming::Device {
                signal: DeviceSignal::Presence {
                    id: DeviceId::new(*a.id()),
                    event: DeviceEvent::from(*a.event()),
                    target: Target::from(*a.target()),
                    rule: (*a.device_rule()).to_owned(),
                    attrs: DeviceAttributes::from_signal_map(a.attributes()),
                },
                invalidates_rules: false,
            })
        });

    let changed = devices
        .receive_device_policy_changed()
        .await
        .map_err(err)?
        .filter_map(|s| async move {
            let a = s
                .args()
                .map_err(|e| warn!("malformed DevicePolicyChanged: {e}"))
                .ok()?;
            Some(Incoming::Device {
                signal: DeviceSignal::Policy {
                    id: DeviceId::new(*a.id()),
                    target: Target::from(*a.target_new()),
                    rule: (*a.device_rule()).to_owned(),
                    attrs: DeviceAttributes::from_signal_map(a.attributes()),
                },
                invalidates_rules: *a.rule_id() != 0,
            })
        });

    let applied = devices
        .receive_device_policy_applied()
        .await
        .map_err(err)?
        .filter_map(|s| async move {
            let a = s
                .args()
                .map_err(|e| warn!("malformed DevicePolicyApplied: {e}"))
                .ok()?;
            Some(Incoming::Device {
                signal: DeviceSignal::Policy {
                    id: DeviceId::new(*a.id()),
                    target: Target::from(*a.target_new()),
                    rule: (*a.device_rule()).to_owned(),
                    attrs: DeviceAttributes::from_signal_map(a.attributes()),
                },
                invalidates_rules: false,
            })
        });

    let exceptions = root
        .receive_exception_message()
        .await
        .map_err(err)?
        .filter_map(|s| async move {
            let a = s.args().ok()?;
            Some(Incoming::Exception {
                context: (*a.context()).to_owned(),
                object: (*a.object()).to_owned(),
                reason: (*a.reason()).to_owned(),
            })
        });

    let parameters = root
        .receive_property_parameter_changed()
        .await
        .map_err(err)?
        .filter_map(|s| async move {
            let a = s.args().ok()?;
            Some(Incoming::Parameter {
                name: (*a.name()).to_owned(),
                value: (*a.value_new()).to_owned(),
            })
        });

    let mut streams: Vec<IncomingStream> = vec![
        Box::pin(presence),
        Box::pin(changed),
        Box::pin(applied),
        Box::pin(exceptions),
        Box::pin(parameters),
    ];

    // On a real bus, watch the bridge's name so a stopped bridge is noticed
    // at once (test T1: disconnected within 2 s). A private test connection
    // has no bus daemon and no names.
    if client.connection().is_bus() {
        let owner = devices
            .inner()
            .receive_owner_changed()
            .await
            .map_err(err)?
            .filter_map(|owner| async move { owner.is_none().then_some(Incoming::OwnerLost) });
        streams.push(Box::pin(owner));
    }

    Ok(Box::pin(stream::select_all(streams)))
}
