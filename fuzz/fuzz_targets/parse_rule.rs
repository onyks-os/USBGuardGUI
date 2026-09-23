//! Arbitrary bytes into every parser entry point. The only acceptable
//! outcomes are `Ok` or a `ParseError` — never a panic, a hang, or an
//! unbounded allocation (docs/architecture.md §7.2).
//!
//! The lexer is covered here too: every entry point tokenizes the whole input
//! before parsing any of it, so a lexer crash cannot be masked by an earlier
//! parse error.

#![no_main]

use libfuzzer_sys::fuzz_target;
use usbguard_gui::model::DeviceId;
use usbguard_gui::rules::{parse_device, parse_partial, parse_query, parse_rule};

fuzz_target!(|data: &[u8]| {
    // D-Bus strings are UTF-8 by protocol, so that is what the daemon can
    // send; non-UTF-8 device bytes arrive as `\xHH` escapes inside it.
    let text = String::from_utf8_lossy(data);
    let _ = parse_rule(&text);
    let _ = parse_partial(&text);
    let _ = parse_query(&text);
    let _ = parse_device(DeviceId::new(0), text.into_owned());
});
