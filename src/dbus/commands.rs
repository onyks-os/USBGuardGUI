//! Cancellable operations (docs/architecture.md §5.7).
//!
//! zbus imposes no client-side reply deadline, so a call waiting on a Polkit
//! prompt waits as long as the human takes — and that is correct: no deadline
//! is imposed on an operation whose duration is a person typing a password.
//! Instead every mutating operation is a task with a handle, which the row
//! that started it can cancel.
//!
//! **Cancellation is client-side only.** Aborting stops waiting for the reply;
//! it does not withdraw a request the daemon may already have executed. So a
//! cancellation is always followed by a re-read.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_channel::Sender;
use tokio::task::JoinHandle;

use crate::model::{AppError, OperationId, OperationOutcome, PendingKind, UiEvent};
use crate::runtime::runtime;

/// After this long, show "Waiting for authentication".
pub const WAITING_NOTICE_AFTER: Duration = Duration::from_secs(20);
/// After this long, suggest that no Polkit agent may be running. The
/// operation is **not** cancelled.
pub const NO_AGENT_HINT_AFTER: Duration = Duration::from_secs(180);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A fresh, process-unique operation id.
#[must_use]
pub fn next_operation_id() -> OperationId {
    OperationId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

/// An operation in flight.
#[derive(Debug)]
pub struct PendingOperation {
    /// Identifies the operation in [`UiEvent::OperationFinished`].
    pub id: OperationId,
    /// Human-readable description, for the pending row.
    pub label: String,
    /// When it started.
    pub started: Instant,
    handle: JoinHandle<()>,
    ui_tx: Sender<UiEvent>,
}

impl PendingOperation {
    /// Which notice the pending row should show now.
    #[must_use]
    pub fn stage(&self, now: Instant) -> PendingKind {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= NO_AGENT_HINT_AFTER {
            PendingKind::PossiblyNoAgent
        } else if elapsed >= WAITING_NOTICE_AFTER {
            PendingKind::WaitingForAuthentication
        } else {
            PendingKind::Started
        }
    }

    /// True once the task has finished (its outcome has been sent).
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Bound to the Cancel button. Stops waiting, reports
    /// [`AppError::Cancelled`], and asks for a re-read, because the daemon
    /// may have acted anyway.
    pub fn cancel(self) {
        if self.handle.is_finished() {
            return;
        }
        self.handle.abort();
        let (id, ui_tx) = (self.id, self.ui_tx);
        // try_send: a full channel must not block the GTK main thread; the
        // cache invalidation below is what matters, and a snapshot follows
        // any reconnect anyway.
        let _ = ui_tx.try_send(UiEvent::OperationFinished {
            op: id,
            result: Err(AppError::Cancelled),
        });
        let _ = ui_tx.try_send(UiEvent::InvalidateRuleCache);
    }
}

/// Spawns `work` on the shared runtime and reports its outcome as
/// [`UiEvent::OperationFinished`].
pub fn spawn<F>(label: impl Into<String>, ui_tx: Sender<UiEvent>, work: F) -> PendingOperation
where
    F: Future<Output = Result<OperationOutcome, AppError>> + Send + 'static,
{
    let id = next_operation_id();
    let tx = ui_tx.clone();
    let handle = runtime().spawn(async move {
        let result = work.await;
        let _ = tx.send(UiEvent::OperationFinished { op: id, result }).await;
    });
    PendingOperation {
        id,
        label: label.into(),
        started: Instant::now(),
        handle,
        ui_tx,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{NO_AGENT_HINT_AFTER, WAITING_NOTICE_AFTER, spawn};
    use crate::model::{AppError, OperationOutcome, PendingKind as PendingStage, UiEvent};

    #[test]
    fn completed_operation_reports_its_outcome() {
        let (tx, rx) = async_channel::unbounded();
        let op = spawn("test", tx, async {
            Ok(OperationOutcome::ParameterSet {
                previous: "block".into(),
            })
        });
        let event = rx.recv_blocking().unwrap();
        assert_eq!(
            event,
            UiEvent::OperationFinished {
                op: op.id,
                result: Ok(OperationOutcome::ParameterSet {
                    previous: "block".into()
                }),
            }
        );
    }

    #[test]
    fn cancel_reports_and_invalidates() {
        let (tx, rx) = async_channel::unbounded();
        let op = spawn("slow", tx, async {
            // Stands in for a Polkit prompt left open.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Err(AppError::Cancelled)
        });
        let id = op.id;
        op.cancel();
        assert_eq!(
            rx.recv_blocking().unwrap(),
            UiEvent::OperationFinished {
                op: id,
                result: Err(AppError::Cancelled)
            }
        );
        assert_eq!(rx.recv_blocking().unwrap(), UiEvent::InvalidateRuleCache);
    }

    #[test]
    fn stages_never_auto_cancel() {
        let (tx, _rx) = async_channel::unbounded();
        let op = spawn("x", tx, async {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Err(AppError::Cancelled)
        });
        let t0 = op.started;
        assert_eq!(op.stage(t0), PendingStage::Started);
        assert_eq!(
            op.stage(t0 + WAITING_NOTICE_AFTER),
            PendingStage::WaitingForAuthentication
        );
        assert_eq!(
            op.stage(t0 + NO_AGENT_HINT_AFTER * 3),
            PendingStage::PossiblyNoAgent
        );
        assert!(!op.is_finished());
        let _ = Instant::now();
        op.cancel();
    }
}
