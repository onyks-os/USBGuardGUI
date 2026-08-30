# Technical Project Specifications: USBGuardGUI

## 1. Project Overview

### 1.1. Objective

Develop a modern and responsive graphical interface for USBGuard, allowing a desktop session user to manage USB device authorization policies through an unprivileged client that communicates exclusively via D‑Bus with `usbguard-dbus`, without any access to the system filesystem and without any privilege escalation in its own executable.

### 1.2. Scope

- Main window: list of devices known to the daemon with authorization status, real‑time updates, resilience to event bursts.
- Rule management: reading, inserting, and removing rules in the active ruleset, with correct handling of ID volatility.
- Interactive notifications: popups for newly inserted devices, with quick actions.
- Access diagnostics: module that distinguishes and explains the different reasons why communication may fail (§4), with remediation instructions.
- System tray and background mode: status icon via `StatusNotifierItem`, with explicit degradation where the protocol is not available.

### 1.3. Out of scope (V3.0)

- Modification of `usbguard-daemon.conf` and IPC ACLs from within the application. This would require root filesystem access and contradict the security model; the application only diagnoses and instructs.
- Management of rule folders (`RuleFolder`) as distinct entities: the daemon already exposes them aggregated via `listRules`.
- Offline editing of `rules.conf` without an active daemon.

### 1.4. Target audience

End users of Linux desktop distributions and system administrators managing workstations with restrictive USB policies.

---

## 2. Constraints Imposed by USBGuard (Read Before Designing)

These constraints are non‑negotiable and determine much of the architecture.

1. **Two distinct ID types**  
   IDs are of two distinct types and must be kept separate in the code. The *device id* is the integer identifying a device in the daemon’s internal table, and is the first number in the output of `usbguard list-devices`; it has nothing to do with the device’s `id` attribute (VID:PID). The *rule id* identifies a rule in the ruleset. A distinct Rust type for each (`DeviceId(u32)`, `RuleId(u32)`) prevents the most likely bug class of the entire project.

2. **Rule IDs are not stable**  
   Rule IDs are not stable. They are reassigned when the ruleset is reloaded or modified by third parties. No local cache can assume they remain valid over time.

3. **`listDevices` and `listRules` return strings**  
   `listDevices` and `listRules` return strings in the rule language. There is no structured accessor for name, serial, or VID:PID. To populate the table columns, a client‑side parser of the rule language is required. Signals, however, carry a dictionary of already structured attributes and avoid parsing on the hot path.

4. **`DevicePresent` signal unusable**  
   The `DevicePresent` signal is unusable by clients. The official documentation explicitly states: it does not reach clients on the bus because connections are handled after device processing. The initial state can only be obtained with `listDevices`.

5. **`DevicePresenceChanged` alone is not sufficient**  
   `DevicePresenceChanged` alone is not enough. It does not tell whether the target it carries is final or whether a policy decision will follow. That is why `DevicePolicyChanged` exists and must be subscribed to together with the former.

6. **`appendRule` accepts a `parent_id`**  
   `appendRule` accepts `parent_id`, not a position. The value `4294967293` (`UINT32_MAX - 2`) is the maximum possible ID and, when used as `parent_id`, appends the rule to the end of the ruleset. It is the constant to use for the “append at end” case.

```rust
/// Conventional parent id for appending a rule at the end of the ruleset.
pub const RULE_PARENT_APPEND_LAST: u32 = u32::MAX - 2; // 4_294_967_293
```

---

## 3. Access Model: The Two Authorization Gates

V2.2 promised an application that "does not require direct root permissions" and implied that this would suffice to make it work. That is not the case. Between the GUI and the daemon there are two independent checks, both closed by default on most distributions.

### 3.1. Gate 1 — USBGuard IPC ACL

The daemon applies its own access control list before D‑Bus even comes into play. The user must appear in `IPCAllowedUsers` or their group in `IPCAllowedGroups` in `/etc/usbguard/usbguard-daemon.conf`, or be registered with `usbguard add-user`. Without this, every call is rejected regardless of Polkit.

```bash
# Granular grant, run as root, then restart the daemon
usbguard add-user $USER --devices=modify,list,listen \
                        --policy=list,modify \
                        --exceptions=listen \
                        --parameters=list
```

Omitting `policy=modify` yields a working read‑only GUI: this is a legitimate configuration that the interface must recognise and represent, disabling write controls instead of letting them fail.

### 3.2. Gate 2 — Polkit

The actions defined in `/usr/share/polkit-1/actions/org.usbguard1.policy` are set to `auth_admin` by default. The actions relevant to this application are:

- `org.usbguard.Policy1.listRules`
- `org.usbguard.Policy1.appendRule`
- `org.usbguard.Policy1.removeRule`
- `org.usbguard.Devices1.applyDevicePolicy`
- `org.usbguard.Devices1.listDevices`
- `org.usbguard1.getParameter`
- `org.usbguard1.setParameter`

Note that even read‑only operations are subject to Polkit: without a local rule, simply populating the table at startup may cause an administrator password prompt to appear. This has a direct consequence on the design: the initial synchronization cannot be silent and automatic in an unprepared configuration, otherwise the user sees a password prompt immediately after launch without understanding why. The GUI therefore performs the probe described in §4 before any call that could trigger authentication.

**Reference Polkit rule**, to be placed in `/etc/polkit-1/rules.d/70-usbguard-gui.rules`:

```javascript
// Read without authentication, write with remembered user authentication
polkit.addRule(function(action, subject) {
    if (!subject.local || !subject.active || !subject.isInGroup("wheel")) {
        return;
    }
    var readOnly = [
        "org.usbguard.Devices1.listDevices",
        "org.usbguard.Policy1.listRules",
        "org.usbguard1.getParameter"
    ];
    var mutating = [
        "org.usbguard.Devices1.applyDevicePolicy",
        "org.usbguard.Policy1.appendRule",
        "org.usbguard.Policy1.removeRule",
        "org.usbguard1.setParameter"
    ];
    if (readOnly.indexOf(action.id) >= 0) {
        return polkit.Result.YES;
    }
    if (mutating.indexOf(action.id) >= 0) {
        return polkit.Result.AUTH_SELF_KEEP;
    }
});
```

The `wheel` group must be adapted to the distribution (`sudo` on Debian/Ubuntu, `plugdev` in some configurations). Changes to Polkit rules are automatically detected by the Polkit daemon, without restart.

### 3.3. Packaging decision

The `.deb` and `.rpm` packages do not install this rule automatically. This compromise is deliberate: silently installing a rule that opens access to USB policies to an entire group is a change to the system’s security posture and must be performed by a conscious administrator. The packages instead install the file as `/usr/share/doc/usbguard-gui/70-usbguard-gui.rules.example` and the GUI indicates it in the diagnostic panel with a copy‑to‑clipboard button.

---

## 4. Diagnostics: Distinguishing Failure Modes

A red dot saying "service unavailable" is useless, because there are five reasons and the remedies differ. The diagnostic module separates them in this order, stopping at the first that fails.

| # | Check | How | Message and remedy |
|---|-------|-----|---------------------|
| 1 | System bus reachable | `Connection::system()` | Session without system D‑Bus. Rare case, typically a misconfigured container environment. |
| 2 | Name `org.usbguard1` owned | `DBusProxy::name_has_owner` | `usbguard-dbus.service` not active. Remedy: enable and start it. Distinguish from the case where `usbguard.service` itself is stopped. |
| 3 | Read authorization | `get_parameter("ImplicitPolicyTarget")` | This is the lightest probe call. An `AccessDenied` here indicates gate 1 or gate 2 closed. |
| 4 | Which of the two gates | Analysis of the error message | A rejection originating from Polkit and one originating from the IPC ACL produce distinguishable errors; in case of ambiguity the GUI shows both remedies. |
| 5 | Write authorization | No test call | Do not probe with a real write. The first operation requested by the user is attempted and the error is handled; a test write would modify the system policy. |

The probe at point 3 may itself cause a Polkit prompt. The GUI therefore executes it only after having shown the main window, so that the user sees which application the request comes from, and never during the initial splash.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessState {
    Ok { can_modify_policy: bool },
    BusUnavailable,
    ServiceNotRunning { daemon_running: bool },
    DeniedByIpcAcl,
    DeniedByPolkit,
    DeniedUnknown { detail: String },
}
```

The distinction between `DeniedByIpcAcl` and `DeniedByPolkit` is based on inspection of the D‑Bus error:

```rust
fn classify(err: &zbus::Error) -> AccessState {
    let zbus::Error::MethodError(name, detail, _) = err else {
        return AccessState::DeniedUnknown { detail: err.to_string() };
    };
    let detail = detail.as_deref().unwrap_or_default().to_ascii_lowercase();
    match name.as_str() {
        "org.freedesktop.DBus.Error.AccessDenied"
        | "org.freedesktop.DBus.Error.AuthFailed" => {
            // The daemon signals IPC ACL rejections with a recognisable
            // message; Polkit instead produces a generic AccessDenied.
            if detail.contains("ipc") || detail.contains("not authorized to") {
                AccessState::DeniedByIpcAcl
            } else {
                AccessState::DeniedByPolkit
            }
        }
        _ => AccessState::DeniedUnknown { detail },
    }
}
```

The heuristic on the message text is fragile by construction and must be calibrated on the USBGuard version under test. In case of doubt, the classification falls back to `DeniedUnknown`, which in the panel shows both remedies instead of guessing.

---

## 5. Architecture and Implementation

### 5.0. Components

```
┌─────────────────────────────────────────────────────────────┐
│ Main thread — GLib MainContext                              │
│                                                             │
│   GTK4 / libadwaita  ◄── glib::spawn_future_local ──┐       │
│   Device model, rule model                          │       │
└─────────────────────────────────────────────────────┼───────┘
                                                      │
                                    async_channel (bounded 64)
                                                      │
┌─────────────────────────────────────────────────────┼───────┐
│ Tokio Runtime (multi‑thread, OnceLock)              │       │
│                                                     │       │
│   Supervisor ──► D‑Bus Worker (zbus)  ──────────────┘       │
│      │              ├─ signal streams (merge + coalescing)  │
│      │              └─ cancellable method calls             │
│      └─ backoff, wait for NameOwnerChanged                  │
└─────────────────────────────────────────────────────────────┘
                                    │
                              System D‑Bus
                                    │
                          usbguard-dbus ──► usbguard-daemon (root)
```

The command flow (GUI → D‑Bus) and the event flow (D‑Bus → GUI) are separate: the former uses `runtime().spawn()` for one‑off tasks that report back on the same `async_channel`, the latter is the persistent worker.

---

### 5.1. D‑Bus Proxies

```rust
// src/dbus/proxies.rs
use std::collections::HashMap;
use zbus::proxy;

/// Root interface: runtime parameters and asynchronous exceptions.
#[proxy(
    interface = "org.usbguard1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1"
)]
pub trait UsbGuard {
    /// name: "InsertedDevicePolicy" | "ImplicitPolicyTarget"
    fn get_parameter(&self, name: &str) -> zbus::Result<String>;
    /// Returns the previous value.
    fn set_parameter(&self, name: &str, value: &str) -> zbus::Result<String>;

    #[zbus(signal)]
    fn property_parameter_changed(
        &self,
        name: &str,
        value_old: &str,
        value_new: &str,
    ) -> zbus::Result<()>;

    /// Asynchronous exceptions from the daemon to IPC/D‑Bus clients.
    #[zbus(signal)]
    fn exception_message(
        &self,
        context: &str,
        object: &str,
        reason: &str,
    ) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.usbguard.Devices1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1/Devices"
)]
pub trait UsbGuardDevices {
    /// query in rule language syntax: "match" for all,
    /// "allow" for only allowed ones. Order is NOT defined.
    fn list_devices(&self, query: &str) -> zbus::Result<Vec<(u32, String)>>;

    /// If `permanent` is true, the daemon adds or modifies a rule
    /// and returns the involved rule id; otherwise the value is
    /// meaningless and must be ignored.
    fn apply_device_policy(
        &self,
        id: u32,
        target: u32,
        permanent: bool,
    ) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn device_presence_changed(
        &self,
        id: u32,
        event: u32,
        target: u32,
        device_rule: &str,
        attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn device_policy_changed(
        &self,
        id: u32,
        target_old: u32,
        target_new: u32,
        device_rule: &str,
        rule_id: u32,
        attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.usbguard.Policy1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1/Policy"
)]
pub trait UsbGuardPolicy {
    /// Returns (rule_id, rule_text) in evaluation order.
    /// The query uses rule language syntax; the parameter has been
    /// renamed over time, so we rely on position, not name.
    fn list_rules(&self, query: &str) -> zbus::Result<Vec<(u32, String)>>;

    /// ATTENTION: the third parameter is `temporary`, not `permanent`.
    /// temporary = true  -> rule only in memory, not written to rules.conf
    /// temporary = false -> rule persisted
    /// Use RULE_PARENT_APPEND_LAST as parent_id to append at the end.
    fn append_rule(
        &self,
        rule: &str,
        parent_id: u32,
        temporary: bool,
    ) -> zbus::Result<u32>;

    fn remove_rule(&self, id: u32) -> zbus::Result<()>;
}
```

The inversion between `temporary` and `permanent` between `Policy1.appendRule` and `Devices1.applyDevicePolicy` is a real trap in the upstream API. The two booleans have opposite polarities while describing the same concept. The spec mandates a wrapper that exposes a single type to the rest of the application:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persistence { Permanent, RuntimeOnly }

impl Persistence {
    /// For Policy1.appendRule (parameter `temporary`)
    fn as_temporary(self) -> bool { self == Persistence::RuntimeOnly }
    /// For Devices1.applyDevicePolicy (parameter `permanent`)
    fn as_permanent(self) -> bool { self == Persistence::Permanent }
}
```

---

### 5.2. Signal Enumerations

For `DevicePresenceChanged`, the `event` field is documented as: `0 = Present`, `1 = Insert`, `2 = Update`, `3 = Remove`.

The `target` field corresponds to USBGuard’s `Rule::Target` enumeration, whose order is `Allow`, `Block`, `Reject`, `Match`, `Device`, `Unknown`, `Empty`, `Invalid`. Only the first three appear as an actual device state. This mapping must be confirmed with the procedure in §11 before release: it depends on the declaration order in the source and is not part of a stable contract.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target { Allow, Block, Reject, Other(u32) }

impl From<u32> for Target {
    fn from(v: u32) -> Self {
        match v {
            0 => Target::Allow,
            1 => Target::Block,
            2 => Target::Reject,
            n => Target::Other(n), // displayed in UI as "Unknown"
        }
    }
}
```

`Target::Other` is not a defensive theoretical branch: if a future version of the daemon reorders the enum, the GUI will show "Unknown" instead of declaring a blocked device as allowed. The reverse conversion, used for `applyDevicePolicy`, rejects `Other` at the type level.

The `attributes` dictionary of the signals contains keys `id`, `name`, `serial`, `hash`, `parent-hash`, `via-port`, `with-interface`. It must be treated as optional on a per‑key basis: absent keys are normal, not an error.

---

### 5.3. Tokio Runtime Bootstrap

This is the point where V2.2 reproduced the panic it claimed to have fixed. GTK owns the main thread and its event loop; `tokio::spawn` called from there without an active runtime panics. The runtime must be created explicitly and used via its handle.

```rust
// src/runtime.rs
use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Shared I/O runtime. Two workers are sufficient: the load is
/// entirely I/O‑bound on a single D‑Bus connection.
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all() // reactor I/O + timer: both needed
            .thread_name("usbguard-io")
            .build()
            .expect("Tokio runtime creation failed")
    })
}
```

**Project rule, enforced in code review:** `tokio::spawn` never appears in the source code. Only `runtime().spawn(...)` is used. A `clippy.toml` lint with `disallowed-methods` makes the rule mechanical:

```toml
disallowed-methods = [
    { path = "tokio::spawn", reason = "use runtime::runtime().spawn()" },
]
```

No future that depends on Tokio’s reactor or timers must be run on `glib::spawn_future_local`, and vice versa. The only contact point between the two worlds is the channel of §5.4, which is runtime‑agnostic precisely for this reason.

---

### 5.4. Bridge Between the Two Event Loops

`glib::Sender` and `glib::MainContext::channel` were deprecated and then removed from glib-rs. The idiomatic replacement is `async_channel`, which works equally on both Tokio and GLib executors. This also eliminates the forwarder of V2.2: one channel was needed, not two in cascade.

```rust
// src/main.rs (excerpt)
fn build_ui(app: &adw::Application) {
    let window = MainWindow::new(app);

    // A single channel crosses the two executors.
    let (ui_tx, ui_rx) = async_channel::bounded::<UiEvent>(64);

    // Consumer on the GTK side, on the main thread.
    glib::spawn_future_local({
        let window = window.clone();
        async move {
            while let Ok(event) = ui_rx.recv().await {
                window.apply(event); // execution guaranteed on the GTK thread
            }
        }
    });

    // Producer on the Tokio side.
    dbus::bridge::start(ui_tx);

    window.present();
}
```

The channel is bounded to 64 elements. Since incoming events are already coalesced (§5.6) and the worker emits at most a dozen messages per second under a burst, saturation is not realistically reachable; if it were, backpressure would propagate back to the coalescer, which still drains the signal stream and merges events. There is therefore no risk, present in V2.2, of blocking the shared zbus stream with method calls.

---

### 5.5. Rule Identity and Invalidation

V2.2 proposed re‑reading the ruleset just before `removeRule` to validate the ID. This solves nothing: between the re‑read and the removal, the ruleset can change again, and one would end up deleting a different rule than the one selected by the user. The window of risk is narrowed but not closed, and it is a TOCTOU on a destructive operation involving system security.

The rule is instead identified by its **canonical text**, which is stable, and the ID is treated as a volatile reference resolved at use time.

```rust
#[derive(Debug, Clone)]
pub struct RuleHandle {
    /// Volatile reference: valid only as long as the ruleset does not change.
    pub id: RuleId,
    /// Stable identity: the text as returned by listRules.
    pub text: String,
}

pub enum RemoveOutcome {
    Removed,
    /// The rule no longer exists: someone already removed it.
    AlreadyGone,
    /// Multiple rules with identical text: explicit user confirmation required.
    Ambiguous { candidates: Vec<RuleId> },
}

async fn remove_rule(
    policy: &UsbGuardPolicyProxy<'_>,
    handle: &RuleHandle,
) -> zbus::Result<RemoveOutcome> {
    let current = policy.list_rules("match").await?;
    let matches: Vec<u32> = current
        .iter()
        .filter(|(_, text)| text == &handle.text)
        .map(|(id, _)| *id)
        .collect();

    match matches.as_slice() {
        [] => Ok(RemoveOutcome::AlreadyGone),
        [only] => {
            policy.remove_rule(*only).await?;
            Ok(RemoveOutcome::Removed)
        }
        many => Ok(RemoveOutcome::Ambiguous {
            candidates: many.iter().copied().map(RuleId).collect(),
        }),
    }
}
```

A race window remains between `list_rules` and `remove_rule`. It is irreducible with this API, because the daemon does not offer conditional removal. What changes relative to V2.2 is that the operation fails safely: if the rule is already gone, this is detected instead of removing another rule that happened to have that ID, and if there are multiple identical ones, the user is asked rather than guessing.

A ruleset rich in textual duplicates is rare but legitimate. The `Ambiguous` branch shows the ordinal position of the candidates, since `listRules` returns rules in evaluation order.

**Cache invalidation.** Since `PropertyParameterChanged` concerns only runtime parameters and not the ruleset, the rule cache is marked stale in three cases: after every successful `appendRule` or `removeRule` (including those originating from other clients, detectable via the `rule_id` of `DevicePolicyChanged`), after every reconnection to the bus, and on every received `ExceptionMessage`. In the absence of a dedicated signal for ruleset changes, the rule panel additionally performs an opportunistic re‑read when its view becomes visible.

---

### 5.6. Event Worker and Coalescing

V2.2’s `interval.tick()` did not do debouncing: it delayed each event by 50 ms sequentially, so that a hundred queued events produced five seconds of latency, while the zbus stream was not drained and backpressure propagated back to the shared connection used for method calls. The denial‑of‑service protection produced the very deadlock it was supposed to prevent.

Proper coalescing merges events instead of delaying them: the stream is always drained, events accumulate in a map indexed by device id — where a later state overwrites the previous one — and a single aggregated update is emitted per window.

```rust
// src/dbus/worker.rs
use std::collections::HashMap;
use std::time::{Duration, Instant};
use futures_util::stream::{self, StreamExt};

/// Quiet window: emit when no events arrive for this time.
const QUIET_WINDOW: Duration = Duration::from_millis(80);
/// Latency ceiling: emit anyway, even under continuous burst.
const MAX_LATENCY: Duration = Duration::from_millis(250);
/// Above this threshold, stop tracking deltas and resynchronise.
const RESYNC_THRESHOLD: usize = 40;

enum RawEvent {
    Presence { id: u32, event: u32, target: u32, attrs: HashMap<String, String>, rule: String },
    Policy   { id: u32, target_new: u32, rule_id: u32, attrs: HashMap<String, String>, rule: String },
    Exception { context: String, reason: String },
}

async fn event_loop(
    devices: &UsbGuardDevicesProxy<'_>,
    root: &UsbGuardProxy<'_>,
    ui_tx: &async_channel::Sender<UiEvent>,
) -> Result<(), WorkerError> {
    // Subscribe BEFORE the initial synchronisation: events emitted
    // during listDevices remain in the stream buffer.
    let presence = devices.receive_device_presence_changed().await?;
    let policy   = devices.receive_device_policy_changed().await?;
    let except   = root.receive_exception_message().await?;

    let mut events = stream::select(
        stream::select(presence.map(into_raw_presence), policy.map(into_raw_policy)),
        except.map(into_raw_exception),
    );

    // Initial synchronisation, after subscription.
    let snapshot = devices.list_devices("match").await?;
    ui_tx.send(UiEvent::DeviceSnapshot(parse_snapshot(snapshot))).await?;

    let mut pending: HashMap<u32, DeviceDelta> = HashMap::new();
    let mut window_start: Option<Instant> = None;

    loop {
        let next = if pending.is_empty() {
            events.next().await
        } else {
            // Limited wait: min(quiet window, remaining ceiling).
            let elapsed = window_start.map(|t| t.elapsed()).unwrap_or_default();
            let budget = QUIET_WINDOW.min(MAX_LATENCY.saturating_sub(elapsed));
            match tokio::time::timeout(budget, events.next()).await {
                Ok(item) => item,
                Err(_) => {
                    flush(&mut pending, &mut window_start, ui_tx).await?;
                    continue;
                }
            }
        };

        let Some(raw) = next else { break }; // stream closed: clean exit

        match raw {
            RawEvent::Exception { context, reason } => {
                // Out‑of‑band: does not participate in coalescing.
                ui_tx.send(UiEvent::DaemonException { context, reason }).await?;
                ui_tx.send(UiEvent::InvalidateRuleCache).await?;
            }
            other => {
                window_start.get_or_insert_with(Instant::now);
                merge_into(&mut pending, other);

                if pending.len() >= RESYNC_THRESHOLD {
                    // Massive burst: a full re‑read costs less
                    // than forty deltas and realigns with certainty.
                    pending.clear();
                    window_start = None;
                    let snap = devices.list_devices("match").await?;
                    ui_tx.send(UiEvent::DeviceSnapshot(parse_snapshot(snap))).await?;
                }
            }
        }
    }

    if !pending.is_empty() {
        flush(&mut pending, &mut window_start, ui_tx).await?;
    }
    Ok(())
}

async fn flush(
    pending: &mut HashMap<u32, DeviceDelta>,
    window_start: &mut Option<Instant>,
    ui_tx: &async_channel::Sender<UiEvent>,
) -> Result<(), WorkerError> {
    let batch: Vec<DeviceDelta> = pending.drain().map(|(_, d)| d).collect();
    *window_start = None;
    ui_tx.send(UiEvent::DeviceBatch(batch)).await?;
    Ok(())
}
```

`merge_into` applies sensible precedence: a `Remove` event overwrites any previous delta for that device id, a `Policy` overwrites the target of a `Presence` for the same id, and attributes accumulate. The result is that a device inserted and removed within the same window never appears in the table, which is the correct behaviour.

The resynchronisation threshold is the true DoS defence: above forty devices moving simultaneously, the cost of a single `listDevices` is less than that of forty incremental updates, and above all it is constant with respect to the burst size.

The latency ceiling of 250 ms makes the responsiveness KPI verifiable: the maximum latency between a D‑Bus event and a table update is `MAX_LATENCY` plus rendering time, regardless of volume, while in the common case of an isolated event it is `QUIET_WINDOW`.

---

### 5.7. Supervisor and Reconnection

In V2.2, the worker terminated on the first error and the application remained mute: the test "daemon restart during use" from the hardening phase could not pass. The worker is now wrapped by a supervisor.

```rust
pub fn start(ui_tx: async_channel::Sender<UiEvent>) {
    runtime().spawn(async move {
        let mut backoff = Duration::from_millis(500);
        const BACKOFF_MAX: Duration = Duration::from_secs(30);

        loop {
            match run_once(&ui_tx).await {
                Ok(()) => {
                    // Clean exit: the GUI closed the channel.
                    break;
                }
                Err(err) => {
                    let _ = ui_tx.send(UiEvent::ConnectionLost(err.to_string())).await;
                    if ui_tx.is_closed() { break; }
                }
            }

            // Active wait for service return: NameOwnerChanged
            // avoids retrying uselessly for the entire backoff duration.
            match wait_for_service(backoff).await {
                ServiceWait::Appeared => backoff = Duration::from_millis(500),
                ServiceWait::TimedOut => backoff = (backoff * 2).min(BACKOFF_MAX),
            }
        }
    });
}
```

`wait_for_service` subscribes to `NameOwnerChanged` on `org.freedesktop.DBus` filtered for `org.usbguard1` and waits, with a cap equal to the current backoff. When the service returns, reconnection is almost immediate, while a long‑absent service does not generate useless traffic on the bus.

After each reconnection, both device and rule caches are considered invalid without exception: the ruleset may have changed during the absence and rule IDs may have been reassigned.

---

### 5.8. Cancellable Operations Instead of Timeouts

V2.2 set a D‑Bus timeout of 120 seconds to avoid expiry of the Polkit prompt wait, attributing to the client a default 25‑second timeout that belongs to libdbus and not to zbus. With zbus, a method call waits for the reply without limit: the real risk is the opposite of what was described — indefinite waiting.

The adopted model does not impose an arbitrary deadline on an operation that depends on how long a human takes to type a password, but it does not leave the interface waiting blindly either.

```rust
pub struct PendingOperation {
    pub label: String,
    handle: tokio::task::JoinHandle<()>,
}

impl PendingOperation {
    /// Called from the "Cancel" button in the pending row.
    pub fn cancel(self) { self.handle.abort(); }
}
```

Every mutating operation becomes a task with its own handle. The corresponding row in the table shows a spinner, a "Cancel" button, and after 20 seconds without outcome, the message "Waiting for authentication". After 180 seconds, the GUI shows a warning suggesting the absence of a working Polkit agent in the session, but does not cancel the operation: if the agent is simply slow, or the user has gone to look for the password, cancelling would be the wrong choice.

Client‑side cancellation aborts the wait for the reply but does not revoke the request already forwarded to the daemon. The spec requires that the interface explicitly state this, and that after cancellation a re‑read be performed to show the actual state instead of the presumed one.

---

### 5.9. Rule Language Parser

First‑class component, absent from V2.2 even though it was needed for two distinct purposes.

On reading, `listDevices` and `listRules` return strings: without a parser, the Name, Vendor, VID:PID, and Serial columns cannot be populated. On writing, local validation before sending avoids causing a Polkit prompt for a syntactically incorrect rule, which would be rejected shortly after.

The parser must handle: the targets `allow`, `block`, `reject`, `match`; the attributes `id`, `hash`, `parent-hash`, `name`, `serial`, `via-port`, `with-interface`, `with-connect-type`, `label`; the set operators `all-of`, `one-of`, `none-of`, `equals`, `equals-ordered`; the `if` conditions; and string quoting with escapes, since device names and serials regularly contain quotes, backslashes, and non‑UTF‑8 bytes.

**Robustness constraints**, non‑negotiable because the input comes from potentially hostile hardware:

- No `panic!`, `unwrap()`, or `expect()` on the parsing path. A device with a malformed descriptor must not be able to crash the application.
- An unparseable rule produces a row with known fields filled in and others marked as unavailable, never an absent row. Hiding a device because its descriptor was not understood is the worst possible failure for a security tool.
- Explicit limits on input length and nesting depth.
- The rule generator on writing does the reverse path with correct escaping, and every generated rule is re‑parsed before sending to verify round‑trip.
- A fuzzing‑based test suite (`cargo-fuzz`) on the parser is part of the completion definition of the corresponding phase.

---

## 6. User Interface

### 6.1. Main Window

- **Header bar.** Three‑state status indicator — connected, connected read‑only, disconnected — which opens the diagnostic panel of §4. The "read‑only" condition is what was missing in V2.2 and is instead the most common case in a partial configuration.

- **Device view.** Columns: status, name, VID:PID, serial, port. The device id remains available but is not the main column, because it is an internal detail of the daemon that confuses when placed alongside VID:PID. Per‑row actions: Allow, Block, Reject, each with the explicit choice between temporary and permanent application. Rows with ongoing operations show a spinner and a cancel button.

- **Rules view.** List in evaluation order, which is semantically relevant and must be visually preserved. Each rule shows its canonical text. Removal with the logic of §5.5, including the disambiguation dialog.

- **New rule dialog.** Guided composition with real‑time local validation, preview of the generated canonical text, choice of insertion position (at the end via `RULE_PARENT_APPEND_LAST`, or after a selected rule), and choice between temporary and permanent.

### 6.2. Notifications

Desktop notifications via `notify-rust` for newly inserted devices not yet authorized, with quick actions.

Support for interactive actions depends on the `actions` capability of the notification server, which must be queried with `get_capabilities` before sending. Where actions are not supported, the notification is informative and clicking opens the main window with the device highlighted. The spec requires this check because a notification with buttons that do not appear is worse than a notification without buttons.

Under Flatpak, the portal `org.freedesktop.portal.Notification` is preferred when available, with fallback to the session bus.

### 6.3. System Tray

`ksni` implements the `StatusNotifierItem` protocol. It does not perform any automatic fallback: the contrary assertion in V2.2 was unfounded. Detection must be implemented by querying the session bus for the owner of `org.kde.StatusNotifierWatcher` before attempting registration. In its absence — GNOME without the AppIndicator extension is the typical case — the application enters background mode without an icon, keeping notifications active, and communicates this once in the settings instead of writing errors to the console.

It should also be considered that `ksni` brings its own separate D‑Bus stack from zbus. The duplication is tolerable but must be accounted for in compilation times and dependency footprint; a tray implemented directly on zbus is an alternative to evaluate if the weight proves excessive.

---

## 7. Packaging

### 7.1. Flatpak

The manifest of V2.2 was not compilable: it completely lacked the `modules` section. It also lacked the permissions necessary for the tray. The YAML format is adopted, which allows comments.

```yaml
# io.github.ESEMPIO.UsbguardGui.yaml
#
# NOTE on app-id: Flathub requires verification of domain ownership.
# The V2.2 "org.usbguard.GUI" is not usable without control of usbguard.org.
# Replace ESEMPIO with your GitHub username.
app-id: io.github.ESEMPIO.UsbguardGui
runtime: org.gnome.Platform
# Align to the latest version available on Flathub at build time;
# 48 is the minimum supported by this project.
runtime-version: '48'
sdk: org.gnome.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable
command: usbguard-gui

build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/usbguard-gui/cargo

finish-args:
  - --socket=wayland
  - --socket=fallback-x11
  - --share=ipc
  - --device=dri

  # Only required system bus access.
  - --system-talk-name=org.usbguard1

  # Notifications: portal, with fallback to session bus.
  - --talk-name=org.freedesktop.portal.Notification
  - --talk-name=org.freedesktop.Notifications

  # System tray. The watcher must be contacted; the item must be owned.
  - --talk-name=org.kde.StatusNotifierWatcher
  # The SNI name has the form org.kde.StatusNotifierItem-<pid>-<n> and is not
  # a pointed subtree, so flatpak's wildcard does not cover it.
  # In the sandbox the process typically gets pid 2: a few variants are declared.
  # Verify on the target environment (§9, test T7):
  # if the tray does not appear under Flatpak, the cause is almost always this.
  - --own-name=org.kde.StatusNotifierItem-2-1
  - --own-name=org.kde.StatusNotifierItem-2-2
  - --own-name=org.kde.StatusNotifierItem-3-1

modules:
  - name: usbguard-gui
    buildsystem: simple
    build-options:
      env:
        CARGO_NET_OFFLINE: 'true'
    build-commands:
      - cargo --offline fetch --manifest-path Cargo.toml
      - cargo --offline build --release
      - install -Dm755 target/release/usbguard-gui /app/bin/usbguard-gui
      - install -Dm644 data/io.github.ESEMPIO.UsbguardGui.desktop
          /app/share/applications/io.github.ESEMPIO.UsbguardGui.desktop
      - install -Dm644 data/io.github.ESEMPIO.UsbguardGui.metainfo.xml
          /app/share/metainfo/io.github.ESEMPIO.UsbguardGui.metainfo.xml
    sources:
      - type: dir
        path: .
      # Generated with flatpak-builder-tools/cargo/flatpak-cargo-generator.py
      - cargo-sources.json
```

The application does not require `--filesystem` of any kind, consistently with the model of §3. GUI preferences reside in the sandbox data directory.

### 7.2. Native Packages

`cargo-deb` and `cargo-generate-rpm` produce the packages. Contents: executable, `.desktop` file, AppStream metainfo, icons, and the Polkit rule as an example in `/usr/share/doc/`, not active, for the reasons of §3.3. The package declares a dependency on `usbguard` and a recommendation on `polkit`. The post‑install script does not modify any system configuration.

---

## 8. Roadmap

The estimate moves from 13 to 17 weeks. The four additional weeks cover work that was absent from the plan in V2.2, not underestimated: the rule language parser, differentiated diagnostics, and resilience tests.

- **Phase 0 — API reconnaissance.** Full introspection of `org.usbguard1` on the USBGuard versions present in Debian stable, Ubuntu LTS, Fedora, and Arch. Experimental verification of the numeric target mapping. Production of the reference XML versioned in the repository. No application code before this step: this is precisely the phase skipped in V2.2.

- **Phase 1 — Proxies and diagnostics.** zbus proxies for the three interfaces. Runtime bootstrap with the anti‑`tokio::spawn` lint. Diagnostic module with error classification. Command‑line prototype listing devices and rules: first verifiable milestone.

- **Phase 2 — Rule language parser.** Parser and generator, round‑trip tests, fuzzing suite, corpus of real rules collected from the reference distributions.

- **Phase 3 — Bridge and worker.** `async_channel`, coalescing, supervisor with backoff, waiting on `NameOwnerChanged`. Automated resilience tests with a simulated daemon.

- **Phase 4 — Interface.** Main window, device and rule views, read‑only mode, cancellable operations, new rule dialog, diagnostic panel.

- **Phase 5 — Tray and notifications.** Watcher detection, background mode, notification capabilities, Flatpak portal.

- **Phase 6 — Hardening and packaging.** Test matrix of §9, packaging, verification on all reference distributions.

---

## 9. Resilience Test Matrix

| ID | Scenario | Expected outcome |
|----|----------|------------------|
| T1 | `systemctl stop usbguard-dbus` with GUI open | "Disconnected" status within 2 s; no crash; automatic reconnection when service restarts |
| T2 | `systemctl restart usbguard` during an operation waiting for Polkit | Operation reported as inconclusive; state re‑read; no optimistic erroneous update |
| T3 | Insertion of a hub with 40+ devices | No interface freeze; resynchronisation triggered; latency within ceiling |
| T4 | Rule removed from CLI while selected in GUI | `AlreadyGone` outcome with understandable message; no removal of different rules |
| T5 | Ruleset with two rules of identical text | Disambiguation dialog with ordinal position |
| T6 | Device with name containing quotes, backslashes, and non‑UTF‑8 bytes | Row displayed with partial fields; no panic; no text corruption |
| T7 | Launch under Flatpak on GNOME, KDE, and Sway | Tray where supported, background mode elsewhere, no console errors |
| T8 | User absent from `IPCAllowedUsers` | Correct diagnosis of gate 1 with remedy shown |
| T9 | User with IPC granted but Polkit on `auth_admin` | No prompt at splash; prompt only on explicit action |
| T10 | User with `policy=list` but without `modify` | Read‑only mode; write controls disabled, not failing |
| T11 | Polkit prompt left open for 5 minutes | No timeout, no spurious error; informative warning after 180 s |
| T12 | Absence of Polkit agent in session | Understandable error with cause indication |

---

## 10. Metrics

| KPI | Target | Measurement method |
|-----|--------|---------------------|
| Isolated event latency | < 150 ms | From D‑Bus signal reception to GTK model update; measured with tracing, median over 100 events |
| Latency under burst | ≤ 250 ms + rendering | Guaranteed by the `MAX_LATENCY` ceiling of §5.6 |
| Polkit resilience | 0 timeouts on waits ≤ 10 min | Test T11 |
| Idle memory | RSS < 110 MB, PSS < 70 MB | `smem` after 10 minutes of background with 20 devices. The 35 MB target of V2.2 is not achievable with GTK4 and libadwaita, which share a significant part of their pages with other GTK applications: PSS is the only honest metric |
| System filesystem access | 0 | Verified with `strace` filtered on `/etc` and `/var` |
| Event loss under burst | 0 | Test T3, comparing final GUI state with `usbguard list-devices` |
| Parser coverage | > 90% lines, 0 panics on 10⁶ fuzzed inputs | `cargo-llvm-cov` and `cargo-fuzz` |

---

## 11. Appendix: API Verification Procedure

To be executed at the start of Phase 0 and at every update of the reference USBGuard version. The result must be versioned in the repository as `docs/dbus-introspection/<distro>-<version>.xml`.

```bash
# Full signatures of the three interfaces
gdbus introspect --system --dest org.usbguard1 \
      --object-path /org/usbguard1 --xml
gdbus introspect --system --dest org.usbguard1 \
      --object-path /org/usbguard1/Devices --xml
gdbus introspect --system --dest org.usbguard1 \
      --object-path /org/usbguard1/Policy --xml

# Verify actual method signatures
busctl --system call org.usbguard1 /org/usbguard1/Devices \
       org.usbguard.Devices1 listDevices s "match"
busctl --system call org.usbguard1 /org/usbguard1/Policy \
       org.usbguard.Policy1 listRules s ""

# Observe real signals during insertion and removal
gdbus monitor --system --dest org.usbguard1
```

The numeric target mapping is verified by comparing the textual output of `usbguard list-devices` with the `target` field values observed on `gdbus monitor` for the same devices, after applying each of the three targets to a test device.

The parameter of `listRules` has been renamed over the course of USBGuard development; the code relies on position and not on name, and the online documentation is known to lag behind the source. In case of discrepancy between this specification and the introspection of the installed version, the introspection prevails.

---

## 12. Open Risks

1. **Target mapping not stable**  
   The target mapping is not a stable contract. It derives from the declaration order of a C++ enum. Mitigation with `Target::Other` limits damage to degraded display, but an upstream change requires a new release.

2. **Rule text as identity**  
   Using rule text as identity assumes stable canonicalisation by the daemon. If a future version normalises rules returned by `listRules` differently, handles held across a long session may no longer match. Mitigation is to always re‑read shortly before use, as already done in §5.5.

3. **Access error classification**  
   Access error classification relies on strings. This is the only available hook with this API and must be recalibrated at every version. The `DeniedUnknown` branch makes it an optimisation of user experience, not a correctness requirement.

4. **D‑Bus name of the tray under Flatpak**  
   The D‑Bus name of the tray under Flatpak depends on the pid inside the sandbox. The solution in §7.1 is a known and empirically verified workaround, not a guarantee. Should it prove unstable, the background mode remains fully functional.

None of these risks can be mitigated without upstream changes in USBGuard. Contributing a `removeRuleIfMatches(id, expected_text)` method to the upstream project would eliminate the TOCTOU of §5.5 at the root, and it is the proposal with the best effort‑to‑benefit ratio to present to the maintainers.