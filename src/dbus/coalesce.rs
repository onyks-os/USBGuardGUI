//! Burst coalescing (docs/architecture.md §5.4) as pure logic.
//!
//! Correct coalescing **merges rather than delays**: events accumulate in a
//! map keyed by device id, a later state supersedes an earlier one, and one
//! aggregated batch is emitted per window. The worker owns the timing; this
//! module owns the merge rules, so they are tested without a bus or a clock.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::model::{DeviceAttributes, DeviceDelta, DeviceEvent, DeviceId, Target};

/// Emit once no event has arrived for this long.
pub const QUIET_WINDOW: Duration = Duration::from_millis(80);
/// Emit regardless after this long, even under a continuous burst.
pub const MAX_LATENCY: Duration = Duration::from_millis(250);
/// Beyond this many pending devices, stop tracking deltas and resynchronize
/// with one `listDevices`: its cost is constant in the size of the burst.
pub const RESYNC_THRESHOLD: usize = 40;

/// A device signal, decoded from the wire but not yet merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSignal {
    /// `DevicePresenceChanged`.
    Presence {
        /// Device id.
        id: DeviceId,
        /// What happened.
        event: DeviceEvent,
        /// Target at the time of the event — possibly not final (§2.4.5).
        target: Target,
        /// Device rule text.
        rule: String,
        /// Attribute dictionary.
        attrs: DeviceAttributes,
    },
    /// `DevicePolicyChanged` or `DevicePolicyApplied`: the policy decision,
    /// which is the final word on the target.
    Policy {
        /// Device id.
        id: DeviceId,
        /// The decided target.
        target: Target,
        /// Device rule text.
        rule: String,
        /// Attribute dictionary.
        attrs: DeviceAttributes,
    },
}

#[derive(Debug)]
struct Pending {
    delta: DeviceDelta,
    /// A policy event has set the target; presence events may no longer
    /// override it (§5.4 rule 2).
    policy_decided: bool,
}

impl Pending {
    fn fresh(id: DeviceId) -> Self {
        Self {
            delta: DeviceDelta {
                id,
                removed: false,
                target: None,
                rule_text: None,
                attrs: DeviceAttributes::default(),
            },
            policy_decided: false,
        }
    }
}

/// Accumulates device signals over one window.
#[derive(Debug, Default)]
pub struct Coalescer {
    pending: HashMap<DeviceId, Pending>,
    window_start: Option<Instant>,
}

impl Coalescer {
    /// An empty coalescer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of devices with pending changes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// True when nothing is pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// True once the burst is large enough that a full resynchronization is
    /// cheaper than the deltas.
    #[must_use]
    pub fn needs_resync(&self) -> bool {
        self.pending.len() >= RESYNC_THRESHOLD
    }

    /// How long to wait for the next event before flushing, or `None` when
    /// nothing is pending (wait indefinitely).
    #[must_use]
    pub fn budget(&self, now: Instant) -> Option<Duration> {
        let start = self.window_start?;
        let elapsed = now.saturating_duration_since(start);
        Some(QUIET_WINDOW.min(MAX_LATENCY.saturating_sub(elapsed)))
    }

    /// Merges one signal, with the precedence rules of §5.4:
    ///
    /// 1. A removal supersedes any pending delta for that device.
    /// 2. A policy target supersedes a presence target.
    /// 3. Attribute maps accumulate, later keys winning.
    pub fn push(&mut self, signal: DeviceSignal, now: Instant) {
        self.window_start.get_or_insert(now);
        match signal {
            DeviceSignal::Presence {
                id,
                event: DeviceEvent::Remove,
                ..
            } => {
                let mut removed = Pending::fresh(id);
                removed.delta.removed = true;
                self.pending.insert(id, removed);
            }
            DeviceSignal::Presence {
                id,
                target,
                rule,
                attrs,
                ..
            } => {
                let entry = self.live_entry(id);
                if !entry.policy_decided {
                    entry.delta.target = Some(target);
                }
                entry.delta.rule_text = Some(rule);
                entry.delta.attrs.merge_from(attrs);
            }
            DeviceSignal::Policy {
                id,
                target,
                rule,
                attrs,
            } => {
                let entry = self.live_entry(id);
                entry.delta.target = Some(target);
                entry.policy_decided = true;
                entry.delta.rule_text = Some(rule);
                entry.delta.attrs.merge_from(attrs);
            }
        }
    }

    /// The pending entry for `id`, replacing a pending removal: a device id
    /// that reappears after a removal in the same window is a new device
    /// (ids are recycled, §2.4.1).
    fn live_entry(&mut self, id: DeviceId) -> &mut Pending {
        let entry = self.pending.entry(id).or_insert_with(|| Pending::fresh(id));
        if entry.delta.removed {
            *entry = Pending::fresh(id);
        }
        entry
    }

    /// Ends the window: returns the batch, ordered by device id, and resets.
    pub fn take(&mut self) -> Vec<DeviceDelta> {
        self.window_start = None;
        let mut batch: Vec<DeviceDelta> = self.pending.drain().map(|(_, p)| p.delta).collect();
        batch.sort_by_key(|d| d.id);
        batch
    }

    /// Drops everything pending — used when a full snapshot replaces it.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.window_start = None;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/dbus_coalesce.rs"]
mod tests;
