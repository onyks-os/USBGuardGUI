//! A mock USBGuard D-Bus bridge (docs/architecture.md §13.1).
//!
//! Implements the three interfaces on a private peer-to-peer connection, so
//! the D-Bus tests never touch a live system daemon, and so the scenarios that
//! are impractical or destructive with real hardware — a 40-device burst (T3),
//! duplicate rules (T5), hostile device names (T6) — are reproducible.
//!
//! Behaviour mirrors what Phase 0 observed on usbguard 1.1.4: `listRules`
//! filters by *label* (`""` means all), and a Polkit refusal is
//! `org.freedesktop.DBus.Error.AccessDenied` with the text "Not authorized.".

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, unreachable_pub)]
// zbus hands `#[zbus(header)]` parameters over by value.
#![allow(clippy::needless_pass_by_value)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tokio::net::UnixStream;
use usbguard_gui::dbus::Client;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, fdo, interface};

/// Everything the mock knows, shared by the three interfaces and the test.
#[derive(Debug, Default)]
pub struct State {
    pub devices: Vec<(u32, String)>,
    pub rules: Vec<(u32, String)>,
    pub next_rule_id: u32,
    pub parameters: HashMap<String, String>,
    /// Method names (as on the wire) that answer "Not authorized.".
    pub denied: HashSet<&'static str>,
    /// Every call received, with its arguments, in order.
    pub calls: Vec<String>,
    /// For each call, in order: the method name and whether the message
    /// carried ALLOW_INTERACTIVE_AUTHORIZATION.
    pub interactive: Vec<(&'static str, bool)>,
}

pub type Shared = Arc<Mutex<State>>;

fn not_authorized() -> fdo::Error {
    fdo::Error::AccessDenied("Not authorized.".into())
}

fn check(
    state: &Shared,
    header: &Header<'_>,
    method: &'static str,
    call: String,
) -> fdo::Result<()> {
    let mut s = state.lock().unwrap();
    s.calls.push(call);
    let interactive = header
        .primary()
        .flags()
        .contains(zbus::message::Flags::AllowInteractiveAuth);
    s.interactive.push((method, interactive));
    if s.denied.contains(method) {
        Err(not_authorized())
    } else {
        Ok(())
    }
}

pub struct Root(pub Shared);

#[interface(name = "org.usbguard1")]
impl Root {
    #[zbus(name = "getParameter")]
    fn get_parameter(&self, #[zbus(header)] header: Header<'_>, name: &str) -> fdo::Result<String> {
        check(
            &self.0,
            &header,
            "getParameter",
            format!("getParameter {name}"),
        )?;
        self.0
            .lock()
            .unwrap()
            .parameters
            .get(name)
            .cloned()
            .ok_or_else(|| fdo::Error::Failed(format!("getParameter: {name}: unknown parameter")))
    }

    #[zbus(name = "setParameter")]
    fn set_parameter(
        &self,
        #[zbus(header)] header: Header<'_>,
        name: &str,
        value: &str,
    ) -> fdo::Result<String> {
        check(
            &self.0,
            &header,
            "setParameter",
            format!("setParameter {name} {value}"),
        )?;
        let mut s = self.0.lock().unwrap();
        s.parameters
            .insert(name.to_owned(), value.to_owned())
            .ok_or_else(|| fdo::Error::Failed(format!("setParameter: {name}: unknown parameter")))
    }

    #[zbus(signal)]
    pub async fn property_parameter_changed(
        emitter: &SignalEmitter<'_>,
        name: &str,
        value_old: &str,
        value_new: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn exception_message(
        emitter: &SignalEmitter<'_>,
        context: &str,
        object: &str,
        reason: &str,
    ) -> zbus::Result<()>;
}

pub struct Devices(pub Shared);

#[interface(name = "org.usbguard.Devices1")]
impl Devices {
    #[zbus(name = "listDevices")]
    fn list_devices(
        &self,
        #[zbus(header)] header: Header<'_>,
        query: &str,
    ) -> fdo::Result<Vec<(u32, String)>> {
        check(
            &self.0,
            &header,
            "listDevices",
            format!("listDevices {query}"),
        )?;
        if query != "match" {
            return Err(fdo::Error::Failed(
                "IPC method: usbguard.IPC.listDevices: RuleParserError".into(),
            ));
        }
        Ok(self.0.lock().unwrap().devices.clone())
    }

    #[zbus(name = "applyDevicePolicy")]
    fn apply_device_policy(
        &self,
        #[zbus(header)] header: Header<'_>,
        id: u32,
        target: u32,
        permanent: bool,
    ) -> fdo::Result<u32> {
        check(
            &self.0,
            &header,
            "applyDevicePolicy",
            format!("applyDevicePolicy {id} {target} {permanent}"),
        )?;
        let mut s = self.0.lock().unwrap();
        let keyword = match target {
            0 => "allow",
            1 => "block",
            2 => "reject",
            _ => return Err(fdo::Error::InvalidArgs("bad target".into())),
        };
        let Some(device) = s.devices.iter_mut().find(|(d, _)| *d == id) else {
            return Err(fdo::Error::Failed("unknown device".into()));
        };
        let rest = device.1.split_once(' ').map_or("", |(_, r)| r).to_owned();
        device.1 = format!("{keyword} {rest}");
        let text = device.1.clone();
        if permanent {
            s.next_rule_id += 1;
            let rule_id = s.next_rule_id;
            s.rules.push((rule_id, text));
            Ok(rule_id)
        } else {
            Ok(0)
        }
    }

    #[zbus(signal)]
    pub async fn device_presence_changed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        event: u32,
        target: u32,
        device_rule: &str,
        attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn device_policy_changed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        target_old: u32,
        target_new: u32,
        device_rule: &str,
        rule_id: u32,
        attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn device_policy_applied(
        emitter: &SignalEmitter<'_>,
        id: u32,
        target_new: u32,
        device_rule: &str,
        rule_id: u32,
        attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;
}

pub struct Policy(pub Shared);

#[interface(name = "org.usbguard.Policy1")]
impl Policy {
    #[zbus(name = "listRules")]
    fn list_rules(
        &self,
        #[zbus(header)] header: Header<'_>,
        label: &str,
    ) -> fdo::Result<Vec<(u32, String)>> {
        check(
            &self.0,
            &header,
            "listRules",
            format!("listRules {label:?}"),
        )?;
        let rules = self.0.lock().unwrap().rules.clone();
        Ok(if label.is_empty() {
            rules
        } else {
            let needle = format!("label \"{label}\"");
            rules
                .into_iter()
                .filter(|(_, t)| t.contains(&needle))
                .collect()
        })
    }

    #[zbus(name = "appendRule")]
    fn append_rule(
        &self,
        #[zbus(header)] header: Header<'_>,
        rule: &str,
        parent_id: u32,
        temporary: bool,
    ) -> fdo::Result<u32> {
        check(
            &self.0,
            &header,
            "appendRule",
            format!("appendRule {rule} {parent_id} {temporary}"),
        )?;
        let mut s = self.0.lock().unwrap();
        s.next_rule_id += 1;
        let id = s.next_rule_id;
        let position = s
            .rules
            .iter()
            .position(|(r, _)| *r == parent_id)
            .map_or(s.rules.len(), |p| p + 1);
        s.rules.insert(position, (id, rule.to_owned()));
        Ok(id)
    }

    #[zbus(name = "removeRule")]
    fn remove_rule(&self, #[zbus(header)] header: Header<'_>, id: u32) -> fdo::Result<()> {
        check(&self.0, &header, "removeRule", format!("removeRule {id}"))?;
        let mut s = self.0.lock().unwrap();
        let before = s.rules.len();
        s.rules.retain(|(r, _)| *r != id);
        if s.rules.len() == before {
            return Err(fdo::Error::Failed(
                "IPC method: usbguard.IPC.removeRule: no such rule".into(),
            ));
        }
        Ok(())
    }
}

/// A running mock and the client-side connection to it.
pub struct Mock {
    pub state: Shared,
    pub server: Connection,
    pub client_connection: Connection,
}

impl Mock {
    /// Starts a mock with the given devices and rules.
    pub async fn start(devices: Vec<(u32, String)>, rules: Vec<(u32, String)>) -> Self {
        let next_rule_id = rules.iter().map(|(id, _)| *id).max().unwrap_or(0);
        let parameters = HashMap::from([
            ("ImplicitPolicyTarget".to_owned(), "block".to_owned()),
            ("InsertedDevicePolicy".to_owned(), "apply-policy".to_owned()),
        ]);
        let state = Arc::new(Mutex::new(State {
            devices,
            rules,
            next_rule_id,
            parameters,
            ..State::default()
        }));
        Self::serve(state).await
    }

    /// Serves an existing state on a fresh connection pair — used to model a
    /// bridge restart: same daemon state, new connection.
    pub async fn serve(state: Shared) -> Self {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        let guid = zbus::Guid::generate();
        let server = zbus::connection::Builder::unix_stream(server_stream)
            .server(guid)
            .unwrap()
            .p2p()
            .serve_at("/org/usbguard1", Root(state.clone()))
            .unwrap()
            .serve_at("/org/usbguard1/Devices", Devices(state.clone()))
            .unwrap()
            .serve_at("/org/usbguard1/Policy", Policy(state.clone()))
            .unwrap()
            .build();
        let client = zbus::connection::Builder::unix_stream(client_stream)
            .p2p()
            .build();
        let (server, client_connection) = tokio::join!(server, client);
        Self {
            state,
            server: server.unwrap(),
            client_connection: client_connection.unwrap(),
        }
    }

    /// A typed client over this mock.
    pub async fn client(&self) -> Client {
        Client::new(&self.client_connection).await.unwrap()
    }

    /// Denies a method with "Not authorized.".
    pub fn deny(&self, method: &'static str) {
        self.state.lock().unwrap().denied.insert(method);
    }

    /// Calls received so far.
    pub fn calls(&self) -> Vec<String> {
        self.state.lock().unwrap().calls.clone()
    }

    /// Emits `DevicePresenceChanged`.
    pub async fn presence(&self, id: u32, event: u32, target: u32, rule: &str, name: &str) {
        let emitter = SignalEmitter::new(&self.server, "/org/usbguard1/Devices").unwrap();
        let attrs = HashMap::from([
            ("name".to_owned(), name.to_owned()),
            ("id".to_owned(), "1234:5678".to_owned()),
        ]);
        Devices::device_presence_changed(&emitter, id, event, target, rule, attrs)
            .await
            .unwrap();
    }

    /// Emits `DevicePolicyChanged`.
    pub async fn policy_changed(&self, id: u32, target_new: u32, rule_id: u32) {
        let emitter = SignalEmitter::new(&self.server, "/org/usbguard1/Devices").unwrap();
        Devices::device_policy_changed(&emitter, id, 1, target_new, "", rule_id, HashMap::new())
            .await
            .unwrap();
    }

    /// Emits `ExceptionMessage`.
    pub async fn exception(&self, reason: &str) {
        let emitter = SignalEmitter::new(&self.server, "/org/usbguard1").unwrap();
        Root::exception_message(&emitter, "ctx", "obj", reason)
            .await
            .unwrap();
    }

    /// Drops the server connection, as a stopped bridge would.
    pub async fn stop(self) -> Shared {
        self.server.graceful_shutdown().await;
        self.state
    }
}

/// A device rule as the daemon would render it.
pub fn device_rule(target: &str, n: u32) -> String {
    format!(
        "{target} id 1234:{n:04x} serial \"S{n}\" name \"Device {n}\" hash \"h{n}=\" \
         parent-hash \"p=\" via-port \"1-{n}\" with-interface 08:06:50 with-connect-type \"hotplug\""
    )
}
