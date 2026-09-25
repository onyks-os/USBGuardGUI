//! Translation of the interface (docs/architecture.md §9.7).
//!
//! Every string the user reads in the window passes through `gettext`. Two
//! things deliberately do not:
//!
//! - **Rule-language keywords** (`allow`, `with-interface`, …): they are
//!   syntax, and a translated keyword would be a syntax error.
//! - **The command line** (`--diagnose` and friends): its output is meant to
//!   be pasted into bug reports, where everyone must be able to read it.
//!
//! `model` and `rules` stay free of any translation dependency: they return
//! structured values, and the functions below word them.

use gettextrs::{bind_textdomain_codeset, bindtextdomain, gettext, textdomain};

use crate::model::{AccessState, AppError, AttributeName, ParseErrorKind};
use crate::rules::builder::{FieldError, FieldErrorKind};

/// The gettext domain: the name of the `.mo` catalogue.
pub(super) const DOMAIN: &str = "usbguard-gui";

/// Where catalogues are installed. Packages build with the default; the
/// Flatpak sets `USBGUARD_GUI_LOCALEDIR=/app/share/locale` at build time.
const LOCALE_DIR: &str = match option_env!("USBGUARD_GUI_LOCALEDIR") {
    Some(dir) => dir,
    None => "/usr/share/locale",
};

/// Points gettext at the catalogues. Development builds use the ones
/// `build.rs` compiled from `po/`, so `cargo run` is translated without
/// installing anything.
///
/// The locale itself is selected by GTK, which calls `setlocale` while it
/// initializes — before any window, and so before any string is looked up.
/// (Calling it here would need `unsafe`, which the crate forbids.)
pub(super) fn init() {
    let dir = if cfg!(debug_assertions) {
        option_env!("USBGUARD_GUI_DEV_LOCALE_DIR").unwrap_or(LOCALE_DIR)
    } else {
        LOCALE_DIR
    };
    let _ = bindtextdomain(DOMAIN, dir);
    let _ = bind_textdomain_codeset(DOMAIN, "UTF-8");
    let _ = textdomain(DOMAIN);
}

/// Replaces `{name}` placeholders. Translations keep the placeholders, so a
/// language can move them wherever its grammar needs them.
pub(super) fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |text, (key, value)| {
            text.replace(&format!("{{{key}}}"), value)
        })
}

/// One line describing an access state, for the header, banner, and dialog.
pub(super) fn access_summary(state: &AccessState) -> String {
    match state {
        AccessState::Ok { .. } => gettext("Connected"),
        AccessState::BusUnavailable => gettext("No system D-Bus in this session"),
        AccessState::BridgeNotInstalled => gettext("The USBGuard D-Bus bridge is not installed"),
        AccessState::BridgeNotRunning { .. } => gettext("The USBGuard D-Bus bridge is not running"),
        AccessState::DeniedByBusPolicy => gettext("Refused by the D-Bus bus policy"),
        AccessState::DeniedByPolkit { .. } => gettext("Refused by Polkit"),
        AccessState::DeniedByIpcAcl => gettext("Refused by USBGuard’s access control"),
        AccessState::NoPolkitAgent => gettext("No Polkit authentication agent is running"),
        AccessState::DeniedUnattributed { .. } => gettext("Access denied"),
    }
}

/// An operation error, worded for a toast.
pub(super) fn app_error(err: &AppError) -> String {
    match err {
        AppError::Denied(state) => fill(
            &gettext("Not permitted: {reason}"),
            &[("reason", &access_summary(state))],
        ),
        AppError::Unreachable(detail) => fill(
            &gettext("USBGuard cannot be reached: {detail}"),
            &[("detail", detail)],
        ),
        // The detail comes from the daemon, in English.
        AppError::Rejected(detail) => fill(
            &gettext("USBGuard refused the request: {detail}"),
            &[("detail", detail)],
        ),
        AppError::Parse(e) => parse_error(e.kind),
        AppError::Cancelled => gettext("The operation was cancelled"),
        AppError::Stale => gettext("The ruleset changed in the meantime"),
    }
}

/// What is wrong with rule text, for the new-rule dialog.
pub(super) fn parse_error(kind: ParseErrorKind) -> String {
    match kind {
        ParseErrorKind::TooLong => gettext("The rule is too long"),
        ParseErrorKind::TooDeep => gettext("Parentheses are nested too deeply"),
        ParseErrorKind::Empty => gettext("The rule is empty"),
        ParseErrorKind::UnterminatedString => gettext("A quoted text is not closed"),
        ParseErrorKind::InvalidEscape => gettext("Invalid escape sequence in a quoted text"),
        ParseErrorKind::UnterminatedParameter => gettext("A parenthesis is not closed"),
        ParseErrorKind::UnexpectedEnd => gettext("The rule ends too early"),
        ParseErrorKind::UnexpectedToken => gettext("Unexpected text"),
        // Translators: allow, block, and reject are rule keywords: do not translate them.
        ParseErrorKind::InvalidTarget => gettext("Expected allow, block, or reject"),
        ParseErrorKind::UnknownAttribute => gettext("Unknown attribute"),
        ParseErrorKind::DuplicateAttribute => gettext("The same attribute appears twice"),
        ParseErrorKind::InvalidValue => gettext("Invalid value for this attribute"),
        ParseErrorKind::UnknownCondition => gettext("Unknown condition"),
        ParseErrorKind::InvalidConditionParameter => gettext("Invalid condition parameter"),
        ParseErrorKind::TooManyValues => gettext("Too many values in one set"),
    }
}

/// The expected shape of a value, for placeholders and error messages.
pub(super) fn value_hint(name: AttributeName) -> String {
    match name {
        AttributeName::Id => gettext("vendor:product, e.g. 1234:5678 or 1234:*"),
        AttributeName::WithInterface => gettext("class:subclass:protocol, e.g. 08:06:50 or 03:*:*"),
        AttributeName::ViaPort => gettext("port, e.g. 1-2 or 1-2.3"),
        AttributeName::Hash | AttributeName::ParentHash => gettext("hash as shown by usbguard"),
        AttributeName::Name
        | AttributeName::Serial
        | AttributeName::WithConnectType
        | AttributeName::Label => gettext("text"),
    }
}

/// An invalid row of the guided rule form.
pub(super) fn field_error(err: &FieldError) -> String {
    match &err.kind {
        FieldErrorKind::Duplicate(name) => fill(
            &gettext("“{attribute}” is already used above"),
            &[("attribute", name.keyword())],
        ),
        FieldErrorKind::InvalidValue { name, word } => fill(
            &gettext("“{value}” is not valid here: expected {shape}"),
            &[("value", word), ("shape", &value_hint(*name))],
        ),
        FieldErrorKind::Empty => gettext("Enter at least one value"),
        FieldErrorKind::SetOfText => gettext(
            "A set of text values needs the text mode, where each value is quoted separately",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::fill;

    #[test]
    fn placeholders_are_filled_wherever_they_are() {
        assert_eq!(fill("{b} then {a}", &[("a", "1"), ("b", "2")]), "2 then 1");
        assert_eq!(fill("no placeholders", &[("a", "1")]), "no placeholders");
    }
}
