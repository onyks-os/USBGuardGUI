//! The USBGuard rule language: parser and renderer (docs/architecture.md §7).
//!
//! Needed twice over: on read, because `listDevices` and `listRules` return
//! rule text and nothing more structured (§2.4.3); on write, so an invalid rule
//! is rejected in the dialog, before any Polkit prompt.
//!
//! ```
//! use usbguard_gui::rules::parse_rule;
//!
//! let rule = parse_rule(r#"allow id 1111:0002 name "Example Receiver""#).unwrap();
//! assert_eq!(rule.to_string(), r#"allow id 1111:0002 name "Example Receiver""#);
//! ```

// The parsing path handles untrusted input: a panic here is a denial of
// service triggered by a USB descriptor (§7.2).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

pub mod builder;
mod lexer;
mod parser;
mod render;

pub use parser::{MAX_INPUT_LEN, MAX_SET_VALUES};
pub use render::render_checked;

use crate::model::{Device, DeviceId, ParseError, Rule};
use parser::Mode;

/// Parses policy text: the target must be `allow`, `block`, or `reject`.
///
/// # Errors
///
/// Returns a [`ParseError`] with the byte offset of the offending token.
pub fn parse_rule(input: &str) -> Result<Rule, ParseError> {
    parser::parse(input.as_bytes(), Mode::Rule)
}

/// Parses a partial rule — one with no target, as some `usbguard` CLI
/// subcommands produce.
///
/// # Errors
///
/// Returns a [`ParseError`] with the byte offset of the offending token.
pub fn parse_partial(input: &str) -> Result<Rule, ParseError> {
    parser::parse(input.as_bytes(), Mode::Partial)
}

/// Parses query text, which additionally accepts `match` as a target (§2.6).
///
/// # Errors
///
/// Returns a [`ParseError`] with the byte offset of the offending token.
pub fn parse_query(input: &str) -> Result<Rule, ParseError> {
    parser::parse(input.as_bytes(), Mode::Query)
}

/// Builds a [`Device`] from one `listDevices` entry. Never fails: rule text
/// the parser rejects still yields a row, with the raw text preserved.
#[must_use]
pub fn parse_device(id: DeviceId, rule_text: String) -> Device {
    let parsed = parse_rule(&rule_text);
    Device::from_parsed(id, rule_text, parsed.as_ref().map_err(|e| *e))
}

#[cfg(test)]
#[path = "../../tests/unit/rules_mod.rs"]
mod tests;
