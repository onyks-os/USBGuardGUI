//! USBGuard GUI (USBGuardGUI)
//!
//! Unprivileged GTK4 desktop client for managing USB device authorization policy through the USBGuard daemon.
//!
//! Everything public in this crate is covered by semantic versioning; items that
//! are not exported here may change in any release.

/// The crate version, taken from `Cargo.toml` at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod cli;
pub mod dbus;
pub mod device_store;
pub mod model;
pub mod remedy;
pub mod rules;
pub mod runtime;
#[cfg(feature = "gui")]
pub mod ui;

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_is_not_empty() {
        assert!(!VERSION.is_empty());
    }
}
