//! The access probe sequence (docs/architecture.md §4.2).
//!
//! Executed in order, stopping at the first failure. It runs **after** the
//! main window is presented, never during startup, so that any Polkit prompt
//! it causes is attributable to a window the user can see.
//!
//! | # | Question                             | Outcome on failure                  |
//! |---|--------------------------------------|-------------------------------------|
//! | 1 | Is there a system bus?               | `BusUnavailable`                    |
//! | 2 | Is `org.usbguard1` owned/activatable? | `BridgeNotInstalled`               |
//! | 3 | Is a Polkit agent running?           | recorded; explains a later denial   |
//! | 4 | Can we read a parameter?             | → 5                                 |
//! | 5 | Which checkpoint denied it?          | a `Denied*` state                   |
//! | 6 | Can we write?                        | **not probed** — only a write can tell |

use std::path::Path;

use tracing::debug;
use zbus::Connection;
use zbus::fdo::{DBusProxy, IntrospectableProxy};
use zbus::names::BusName;

use super::client::Client;
use super::errors::{ErrorShape, classify};
use super::supervisor::BRIDGE_NAME;
use crate::model::{AccessState, AppError, Capabilities, Parameter};

/// What the bridge's interface says about the daemon's API level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiLevel {
    /// `appendRule` takes `temporary`: USBGuard 1.1.0 or later.
    AtLeast1_1,
    /// `appendRule` has no `temporary`: older than 1.1.0. Runtime-only rules
    /// cannot be offered (§2.4.6).
    Before1_1,
}

/// Everything the probe learned, for the diagnostic panel and `--diagnose`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReport {
    /// The outcome.
    pub state: AccessState,
    /// Whether the bridge name had an owner before probing.
    pub bridge_owned: Option<bool>,
    /// Whether the bridge is D-Bus activatable (installed).
    pub bridge_activatable: Option<bool>,
    /// Whether `usbguard.service` is active, per systemd.
    pub daemon_running: Option<bool>,
    /// Probe 3: a known Polkit agent is running in this session.
    pub agent_present: Option<bool>,
    /// The daemon's API level, when reachable.
    pub api: Option<ApiLevel>,
    /// `ImplicitPolicyTarget`, when readable.
    pub implicit_policy_target: Option<String>,
}

impl ProbeReport {
    fn new(state: AccessState) -> Self {
        Self {
            state,
            bridge_owned: None,
            bridge_activatable: None,
            daemon_running: None,
            agent_present: None,
            api: None,
            implicit_policy_target: None,
        }
    }
}

/// Runs the probe sequence against the system bus.
pub async fn probe() -> ProbeReport {
    // Probe 1.
    let Ok(connection) = Connection::system().await else {
        return ProbeReport::new(AccessState::BusUnavailable);
    };
    probe_on(&connection).await
}

/// Runs probes 2–5 on an existing bus connection.
pub async fn probe_on(connection: &Connection) -> ProbeReport {
    let mut report = ProbeReport::new(AccessState::DeniedUnattributed {
        detail: String::new(),
    });

    // Probe 2a/2b. Neither call touches the filesystem: the D-Bus service file
    // that makes the name activatable is installed by the bridge package and
    // nothing else, so "not activatable and not owned" means "not installed".
    if let Ok(dbus) = DBusProxy::new(connection).await {
        if let Ok(name) = BusName::try_from(BRIDGE_NAME) {
            report.bridge_owned = dbus.name_has_owner(name).await.ok();
        }
        report.bridge_activatable = dbus
            .list_activatable_names()
            .await
            .ok()
            .map(|names| names.iter().any(|n| n.as_str() == BRIDGE_NAME));
    }
    report.daemon_running = unit_active(connection, "usbguard.service").await;

    if report.bridge_owned == Some(false) && report.bridge_activatable == Some(false) {
        report.state = AccessState::BridgeNotInstalled;
        return report;
    }

    // Probe 3. Inside a Flatpak sandbox /proc shows only the sandbox's own
    // processes, so the heuristic would always say "no agent": report
    // "unknown" instead.
    report.agent_present = if Path::new("/.flatpak-info").exists() {
        None
    } else {
        polkit_agent_present(Path::new("/proc"))
    };

    // Probe 4. If the name is activatable but unowned, this call is what
    // activates it.
    let client = match Client::new(connection).await {
        Ok(c) => c,
        Err(e) => {
            report.state = AccessState::DeniedUnattributed {
                detail: e.to_string(),
            };
            return report;
        }
    };
    match client
        .root_proxy()
        .get_parameter(Parameter::ImplicitPolicyTarget.name())
        .await
    {
        Ok(value) => {
            report.implicit_policy_target = Some(value);
            report.api = api_level(connection).await;
            report.state = AccessState::Ok {
                caps: Capabilities {
                    get_parameters: true,
                    ..Capabilities::default()
                },
            };
        }
        // Probe 5.
        Err(err) => {
            debug!("probe call failed: {err}");
            report.state = match classify(&ErrorShape::from(&err), report.agent_present) {
                AccessState::BridgeNotRunning { .. } => AccessState::BridgeNotRunning {
                    daemon_running: report.daemon_running,
                },
                other => other,
            };
        }
    }
    report
}

/// Reads the API level from the bridge's own introspection data (the only
/// version information the D-Bus interface exposes).
pub async fn api_level(connection: &Connection) -> Option<ApiLevel> {
    let proxy = IntrospectableProxy::builder(connection)
        .destination(BRIDGE_NAME)
        .ok()?
        .path("/org/usbguard1/Policy")
        .ok()?
        .build()
        .await
        .ok()?;
    let xml = proxy.introspect().await.ok()?;
    Some(if xml.contains("name=\"temporary\"") {
        ApiLevel::AtLeast1_1
    } else {
        ApiLevel::Before1_1
    })
}

/// Whether a systemd unit is active, asked over the system bus. `None` when
/// systemd cannot be asked.
async fn unit_active(connection: &Connection, unit: &str) -> Option<bool> {
    let manager = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .ok()?;
    let path: zbus::zvariant::OwnedObjectPath = match manager.call("GetUnit", &(unit,)).await {
        Ok(path) => path,
        // NoSuchUnit: the unit is not loaded, which means it is not running.
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.systemd1.NoSuchUnit" =>
        {
            return Some(false);
        }
        Err(_) => return None,
    };
    let unit = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        path,
        "org.freedesktop.systemd1.Unit",
    )
    .await
    .ok()?;
    let state: String = unit.get_property("ActiveState").await.ok()?;
    Some(state == "active" || state == "reloading")
}

/// Process names (as `/proc/<pid>/comm` shows them, truncated to 15 bytes)
/// of known Polkit authentication agents, including desktop shells that embed
/// one.
const KNOWN_AGENTS: [&str; 17] = [
    "gnome-shell",
    "polkit-gnome-au",
    "polkit-kde-auth",
    "polkit-mate-aut",
    "mate-polkit",
    "lxpolkit",
    "lxqt-policykit-",
    "xfce-polkit",
    "hyprpolkitagent",
    "cinnamon",
    "budgie-polkit-d",
    "pantheon-agent-",
    "cosmic-osd",
    "soteria",
    "ukui-polkit",
    "deepin-polkit-a",
    "phosh",
];

/// Probe 3 (§3.4). Polkit offers an unprivileged process no way to ask
/// whether an agent is registered, so this is a heuristic over process names:
/// `Some(true)` when a known agent runs, `Some(false)` when none does, `None`
/// when `/proc` cannot be read. It only ever changes which explanation a
/// denial gets, never control flow.
#[must_use]
pub fn polkit_agent_present(proc_root: &Path) -> Option<bool> {
    let entries = std::fs::read_dir(proc_root).ok()?;
    let found = entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .any(|comm| KNOWN_AGENTS.contains(&comm.trim_end()));
    Some(found)
}

/// Maps the probe outcome to the `--diagnose` exit codes of
/// docs/interfaces.md §3.
#[must_use]
pub const fn exit_code(state: &AccessState) -> u8 {
    match state {
        AccessState::Ok { .. } => 0,
        AccessState::BusUnavailable => 5,
        AccessState::BridgeNotInstalled | AccessState::BridgeNotRunning { .. } => 4,
        AccessState::DeniedByBusPolicy
        | AccessState::DeniedByPolkit { .. }
        | AccessState::DeniedByIpcAcl
        | AccessState::NoPolkitAgent
        | AccessState::DeniedUnattributed { .. } => 3,
    }
}

/// Converts an operation error into the access state it implies, if any, so
/// a denied write can update the diagnostic panel (§3.5).
#[must_use]
pub fn state_from_error(err: &AppError) -> Option<AccessState> {
    match err {
        AppError::Denied(state) => Some(state.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{exit_code, polkit_agent_present};
    use crate::model::{AccessState, Capabilities};

    #[test]
    fn exit_codes_match_the_interface_document() {
        assert_eq!(
            exit_code(&AccessState::Ok {
                caps: Capabilities::default()
            }),
            0
        );
        assert_eq!(exit_code(&AccessState::DeniedByIpcAcl), 3);
        assert_eq!(exit_code(&AccessState::BridgeNotInstalled), 4);
        assert_eq!(
            exit_code(&AccessState::BridgeNotRunning {
                daemon_running: None
            }),
            4
        );
        assert_eq!(exit_code(&AccessState::BusUnavailable), 5);
    }

    #[test]
    fn agent_heuristic_reads_comm() {
        let dir = std::env::temp_dir().join(format!("usbguard-gui-proc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("42")).unwrap();
        fs::write(dir.join("42/comm"), "bash\n").unwrap();
        fs::create_dir_all(dir.join("self")).unwrap();
        assert_eq!(polkit_agent_present(&dir), Some(false));
        fs::create_dir_all(dir.join("77")).unwrap();
        fs::write(dir.join("77/comm"), "polkit-kde-auth\n").unwrap();
        assert_eq!(polkit_agent_present(&dir), Some(true));
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(polkit_agent_present(&dir), None);
    }
}
