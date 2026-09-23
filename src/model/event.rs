//! [`UiEvent`] — the only message type crossing the executor boundary
//! (docs/architecture.md §5.3, §6.1).
//!
//! It is `Send`, contains no GTK type and no zbus type, and this module depends
//! on neither. That is what lets the compiler, rather than code review, keep
//! the Tokio side and the GTK side apart.

use super::access::AccessState;
use super::device::{Device, DeviceDelta};
use super::error::AppError;
use super::ids::RuleId;
use super::rule::{RemoveOutcome, RuleHandle};

/// Identifies one user-initiated operation, so its outcome can be matched to
/// the row that started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationId(pub u64);

/// What a finished operation achieved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationOutcome {
    /// `applyDevicePolicy` succeeded. `rule` is the rule the daemon created or
    /// changed — present only for a permanent change (§2.4.8).
    DevicePolicyApplied {
        /// The rule id, when meaningful.
        rule: Option<RuleId>,
    },
    /// `appendRule` succeeded.
    RuleAppended(RuleId),
    /// A safe removal finished (§6.4).
    RuleRemoved(RemoveOutcome),
    /// `setParameter` succeeded; the daemon returned the previous value.
    ParameterSet {
        /// The value before the change.
        previous: String,
    },
}

/// Everything the Tokio side can tell the interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    /// A full device list: replace the model.
    DeviceSnapshot(Vec<Device>),
    /// One coalescing window's worth of changes.
    DeviceBatch(Vec<DeviceDelta>),
    /// A full ruleset, in evaluation order.
    RuleSnapshot(Vec<RuleHandle>),
    /// Reading the ruleset failed.
    RuleSnapshotFailed(AppError),
    /// The ruleset may have changed: re-read it before trusting any rule id.
    InvalidateRuleCache,
    /// A runtime parameter changed.
    ParameterChanged {
        /// Parameter name.
        name: String,
        /// New value.
        value: String,
    },
    /// The daemon reported an asynchronous error.
    DaemonException {
        /// Where it happened.
        context: String,
        /// What it concerned.
        object: String,
        /// Why.
        reason: String,
    },
    /// A user operation finished.
    OperationFinished {
        /// Which one.
        op: OperationId,
        /// How it ended.
        result: Result<OperationOutcome, AppError>,
    },
    /// The diagnostic probe produced a new state.
    AccessStateChanged(AccessState),
    /// The event worker lost the daemon and is retrying.
    ConnectionLost(String),
    /// The event worker is connected again; both caches were discarded.
    Reconnected,
}

#[cfg(test)]
mod tests {
    use super::UiEvent;

    fn assert_send<T: Send + 'static>() {}

    #[test]
    fn ui_event_crosses_threads() {
        assert_send::<UiEvent>();
    }
}
