//! The USBGuard D-Bus interfaces (docs/architecture.md §8).
//!
//! The single place in the program where raw upstream signatures appear.
//! Everything above this module uses the domain types of [`crate::model`];
//! [`super::client`] does the conversion.
//!
//! Verified against `docs/dbus-introspection/usbguard-1.1.4-*.xml` (captured on
//! Fedora; the interface is the same upstream code on every distribution).
//! Method names are camelCase upstream, so each carries an explicit
//! `#[zbus(name = …)]`; zbus would otherwise send `GetParameter`.
//!
//! The four mutating methods carry `allow_interactive_auth`: the bridge
//! honours the D-Bus `ALLOW_INTERACTIVE_AUTHORIZATION` flag, and without it
//! Polkit refuses at once ("Not authorized.") instead of asking for a
//! password. The read methods deliberately do not: the event worker reads
//! before the window is on screen, and a password prompt must never appear
//! without a window to attribute it to (docs/architecture.md §4.2). Where a
//! distribution gates reads with `auth_admin`, the diagnostics explain it.

// The `#[proxy]` macro generates signal-argument structs and stream types that
// cannot carry doc comments; the traits below are the documentation.
#![allow(missing_docs)]

use std::collections::HashMap;

use zbus::proxy;

/// Root interface: runtime parameters and asynchronous daemon exceptions.
#[proxy(
    interface = "org.usbguard1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1",
    gen_blocking = false
)]
pub trait UsbGuard {
    /// `name`: `ImplicitPolicyTarget` or `InsertedDevicePolicy`.
    #[zbus(name = "getParameter")]
    fn get_parameter(&self, name: &str) -> zbus::Result<String>;

    /// Returns the PREVIOUS value.
    #[zbus(name = "setParameter", allow_interactive_auth)]
    fn set_parameter(&self, name: &str, value: &str) -> zbus::Result<String>;

    /// A runtime parameter changed.
    #[zbus(signal)]
    fn property_parameter_changed(
        &self,
        name: &str,
        value_old: &str,
        value_new: &str,
    ) -> zbus::Result<()>;

    /// An asynchronous daemon-side error.
    #[zbus(signal)]
    fn exception_message(&self, context: &str, object: &str, reason: &str) -> zbus::Result<()>;
}

/// Devices: listing, authorization, and presence/policy signals.
#[proxy(
    interface = "org.usbguard.Devices1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1/Devices",
    gen_blocking = false
)]
pub trait UsbGuardDevices {
    /// `query` is rule-language syntax: `match` for every device. Result
    /// order is unspecified.
    #[zbus(name = "listDevices")]
    fn list_devices(&self, query: &str) -> zbus::Result<Vec<(u32, String)>>;

    /// The returned rule id is meaningful only when `permanent` is true.
    #[zbus(name = "applyDevicePolicy", allow_interactive_auth)]
    fn apply_device_policy(&self, id: u32, target: u32, permanent: bool) -> zbus::Result<u32>;

    /// A device appeared, changed, or left.
    #[zbus(signal)]
    fn device_presence_changed(
        &self,
        id: u32,
        event: u32,
        target: u32,
        device_rule: &str,
        attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;

    /// A device's authorization target changed.
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

    /// A policy decision was applied to a device, whether or not it changed
    /// the target. Found by Phase 0 introspection; not in the upstream
    /// documentation.
    #[zbus(signal)]
    fn device_policy_applied(
        &self,
        id: u32,
        target_new: u32,
        device_rule: &str,
        rule_id: u32,
        attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;
}

/// Policy: the ruleset.
#[proxy(
    interface = "org.usbguard.Policy1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1/Policy",
    gen_blocking = false
)]
pub trait UsbGuardPolicy {
    /// Returns `(rule_id, rule_text)` in evaluation order.
    ///
    /// The argument is a **label filter**, not a query: `""` returns every
    /// rule, and `"match"` returns only rules labelled `match` — normally
    /// none. Verified in Phase 0; docs/architecture.md §2.2 had it wrong.
    #[zbus(name = "listRules")]
    fn list_rules(&self, label: &str) -> zbus::Result<Vec<(u32, String)>>;

    /// Third parameter is `temporary`, NOT `permanent` (§2.4.7).
    #[zbus(name = "appendRule", allow_interactive_auth)]
    fn append_rule(&self, rule: &str, parent_id: u32, temporary: bool) -> zbus::Result<u32>;

    /// Remove by rule id.
    #[zbus(name = "removeRule", allow_interactive_auth)]
    fn remove_rule(&self, id: u32) -> zbus::Result<()>;
}
