//! What to tell the user for each access state (docs/architecture.md §4, §3.6).
//!
//! The program never runs any of these commands: every remedy changes system
//! configuration, and that is the administrator's decision. It prints them,
//! and the interface offers a copy button.
//!
//! The distribution is read from `/usr/lib/os-release`, not `/etc/os-release`:
//! the program reads nothing under `/etc` (docs/interfaces.md §5), and the
//! `/usr/lib` file is the one `/etc/os-release` normally links to.

use std::path::Path;

use crate::model::AccessState;

/// Where the packages ship the example Polkit rule (§3.6).
pub const POLKIT_RULE_EXAMPLE: &str = "/usr/share/doc/usbguard-gui/70-usbguard-gui.rules.example";

/// The example rule's content, for display with a copy button.
pub const POLKIT_RULE_TEXT: &str = include_str!("../packaging/70-usbguard-gui.rules.example");

/// The distribution families whose USBGuard packaging differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    /// Fedora, RHEL, CentOS, and derivatives: `dnf`.
    Fedora,
    /// Debian, Ubuntu, and derivatives: `apt`.
    Debian,
    /// Arch and derivatives: `pacman`.
    Arch,
    /// Anything else.
    Unknown,
}

impl Distro {
    /// Detects the running distribution. Under Flatpak, `/usr/lib/os-release`
    /// describes the sandbox runtime, not the host; the host's file is
    /// exposed at `/run/host/os-release`, and is read first.
    #[must_use]
    pub fn detect() -> Self {
        let host = Path::new("/run/host/os-release");
        if host.exists() {
            Self::from_os_release_file(host)
        } else {
            Self::from_os_release_file(Path::new("/usr/lib/os-release"))
        }
    }

    /// Detects from an os-release file; `Unknown` if it cannot be read.
    #[must_use]
    pub fn from_os_release_file(path: &Path) -> Self {
        std::fs::read_to_string(path).map_or(Self::Unknown, |text| Self::from_os_release(&text))
    }

    /// Detects from os-release content, using `ID` then `ID_LIKE`.
    #[must_use]
    pub fn from_os_release(text: &str) -> Self {
        let field = |key: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
                .map(|v| {
                    v.trim_matches(|c| c == '"' || c == '\'')
                        .to_ascii_lowercase()
                })
                .unwrap_or_default()
        };
        let ids = format!("{} {}", field("ID"), field("ID_LIKE"));
        for id in ids.split_whitespace() {
            match id {
                "fedora" | "rhel" | "centos" => return Self::Fedora,
                "debian" | "ubuntu" => return Self::Debian,
                "arch" => return Self::Arch,
                _ => {}
            }
        }
        Self::Unknown
    }

    /// The command that installs the D-Bus bridge.
    ///
    /// Verified (docs/architecture.md §13.4): Fedora ships the bridge as a
    /// separate `usbguard-dbus` package; Debian, Ubuntu, and Arch include it
    /// in `usbguard` itself.
    #[must_use]
    pub const fn install_bridge(self) -> &'static str {
        match self {
            Self::Fedora => "sudo dnf install usbguard-dbus",
            Self::Debian => "sudo apt install usbguard",
            Self::Arch => "sudo pacman -S usbguard",
            Self::Unknown => "install your distribution's usbguard-dbus (or usbguard) package",
        }
    }
}

/// A remedy: one explanation, and the commands that apply it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remedy {
    /// What is wrong, in one or two sentences.
    pub explanation: String,
    /// Commands for an administrator, in order.
    pub commands: Vec<String>,
}

const START_BRIDGE: &str = "sudo systemctl enable --now usbguard-dbus.service";
const START_DAEMON: &str = "sudo systemctl enable --now usbguard.service";
const ADD_IPC_USER: &str = "sudo usbguard add-user \"$USER\" --devices=modify,list,listen \
--policy=list,modify --exceptions=listen --parameters=list";

/// The remedy for an access state, or `None` for `Ok`.
#[must_use]
pub fn remedy(state: &AccessState, distro: Distro) -> Option<Remedy> {
    let r = |explanation: &str, commands: &[&str]| Remedy {
        explanation: explanation.to_owned(),
        commands: commands.iter().map(|c| (*c).to_owned()).collect(),
    };
    let install_polkit_rule = format!(
        "sudo install -m 0644 {POLKIT_RULE_EXAMPLE} /etc/polkit-1/rules.d/70-usbguard-gui.rules"
    );
    Some(match state {
        AccessState::Ok { .. } => return None,
        AccessState::BusUnavailable => r(
            "There is no system D-Bus in this session. This usually means the program runs \
             in a container or sandbox without access to the host's system bus.",
            &[],
        ),
        AccessState::BridgeNotInstalled => r(
            "The USBGuard D-Bus bridge is not installed. This program can only reach USBGuard \
             through the bridge. On Fedora it is a separate package (usbguard-dbus); on \
             Debian, Ubuntu, and Arch it comes with the usbguard package itself.",
            &[distro.install_bridge(), START_BRIDGE],
        ),
        AccessState::BridgeNotRunning { daemon_running } => {
            let mut remedy = r(
                "The USBGuard D-Bus bridge is installed but not running.",
                &[START_BRIDGE],
            );
            if *daemon_running == Some(false) {
                remedy
                    .explanation
                    .push_str(" The USBGuard daemon is not running either.");
                remedy.commands.insert(0, START_DAEMON.to_owned());
            }
            remedy
        }
        AccessState::DeniedByBusPolicy => r(
            "The system bus policy does not allow this user to talk to USBGuard. This is set \
             in /usr/share/dbus-1/system.d/org.usbguard1.conf (or an override under /etc), \
             and a Polkit rule cannot change it.",
            &[],
        ),
        AccessState::DeniedByPolkit { .. } => Remedy {
            explanation: "Polkit refused the request. An administrator can allow reading \
                without a password, and changes with your own password, by installing the \
                example rule shipped with this program. Review it first: it grants the \
                administrator group control over USB policy."
                .to_owned(),
            commands: vec![install_polkit_rule],
        },
        AccessState::DeniedByIpcAcl => r(
            "USBGuard's own access control refused the request. Grant this user access, then \
             restart the daemon.",
            &[ADD_IPC_USER, "sudo systemctl restart usbguard.service"],
        ),
        AccessState::NoPolkitAgent => r(
            "The request needs authentication, but no known Polkit authentication agent is \
             running in this session, so no password prompt could be shown. Start your \
             desktop's Polkit agent (for example polkit-gnome or polkit-kde-agent).",
            &[],
        ),
        AccessState::DeniedUnattributed { .. } => Remedy {
            explanation: "Access was denied, and the refusing component could not be \
                identified. Either a Polkit rule or USBGuard's IPC access control applies; \
                both remedies are listed."
                .to_owned(),
            commands: vec![install_polkit_rule, ADD_IPC_USER.to_owned()],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{Distro, remedy};
    use crate::model::{AccessState, Capabilities};

    #[test]
    fn distro_detection() {
        assert_eq!(Distro::from_os_release("ID=fedora\n"), Distro::Fedora);
        assert_eq!(
            Distro::from_os_release("ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n"),
            Distro::Debian
        );
        assert_eq!(
            Distro::from_os_release("ID=endeavouros\nID_LIKE=arch\n"),
            Distro::Arch
        );
        assert_eq!(
            Distro::from_os_release("ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n"),
            Distro::Fedora
        );
        assert_eq!(Distro::from_os_release("ID=nixos\n"), Distro::Unknown);
        assert_eq!(Distro::from_os_release(""), Distro::Unknown);
    }

    #[test]
    fn not_installed_and_not_running_have_different_remedies() {
        // T8/T9: the two states must never read the same.
        let a = remedy(&AccessState::BridgeNotInstalled, Distro::Fedora).unwrap();
        let b = remedy(
            &AccessState::BridgeNotRunning {
                daemon_running: Some(true),
            },
            Distro::Fedora,
        )
        .unwrap();
        assert_ne!(a, b);
        assert_eq!(a.commands[0], "sudo dnf install usbguard-dbus");
        assert!(!b.commands.iter().any(|c| c.contains("install")));
    }

    #[test]
    fn stopped_daemon_is_started_first() {
        let r = remedy(
            &AccessState::BridgeNotRunning {
                daemon_running: Some(false),
            },
            Distro::Arch,
        )
        .unwrap();
        assert!(r.commands[0].contains("usbguard.service"));
    }

    #[test]
    fn ok_has_no_remedy() {
        assert!(
            remedy(
                &AccessState::Ok {
                    caps: Capabilities::default()
                },
                Distro::Unknown
            )
            .is_none()
        );
    }
}
