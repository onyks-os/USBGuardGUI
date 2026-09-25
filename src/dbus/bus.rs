//! Which D-Bus bus the program talks to.
//!
//! Always the **system** bus, where USBGuard's bridge lives — except in
//! development builds, where `USBGUARD_GUI_BUS=session` selects the session
//! bus instead, so the window can be run against the demo bridge
//! (`cargo run --example demo-bridge`) with invented devices, for screenshots
//! and development without touching the real daemon.
//!
//! Release builds ignore the variable: an installed program must never be
//! redirectable to a stand-in for USBGuard by an environment variable.

use zbus::Connection;

/// Opens the bus the USBGuard bridge is expected on.
///
/// # Errors
///
/// Whatever zbus reports when the bus cannot be reached.
pub async fn connect() -> zbus::Result<Connection> {
    if uses_session_bus() {
        Connection::session().await
    } else {
        Connection::system().await
    }
}

/// True when a development build was asked to use the session bus.
#[must_use]
pub fn uses_session_bus() -> bool {
    cfg!(debug_assertions) && std::env::var("USBGUARD_GUI_BUS").is_ok_and(|v| v == "session")
}
