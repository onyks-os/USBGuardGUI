//! "Start at login": an XDG autostart entry in the user's own configuration
//! directory, starting the program hidden (`--background`) so that newly
//! inserted devices are announced from the start of the session.
//!
//! The only file the program ever writes outside GSettings, and it is in the
//! user's home, never under /etc (docs/interfaces.md §5). Not offered under
//! Flatpak, where the Background portal is the proper mechanism.

use std::path::{Path, PathBuf};

use gtk::glib;

use super::APP_ID;

const ENTRY: &str = "[Desktop Entry]
Type=Application
Name=USBGuard
Comment=Announce newly inserted USB devices
Exec=usbguard-gui --background
Icon=io.github.onyks_os.UsbguardGui
NoDisplay=true
X-GNOME-Autostart-enabled=true
";

/// True when running inside a Flatpak sandbox.
pub(super) fn sandboxed() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn path() -> PathBuf {
    glib::user_config_dir()
        .join("autostart")
        .join(format!("{APP_ID}.desktop"))
}

pub(super) fn enabled() -> bool {
    path().exists()
}

/// Creates or removes the entry.
pub(super) fn set(enabled: bool) -> std::io::Result<()> {
    let path = path();
    if enabled {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, ENTRY)
    } else {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}
