//! Whatever parses must render to text that parses back to the same value:
//! `parse(render(parse(x))) == parse(x)` (DYNAMIC_ANALYSIS_POLICY.md).

#![no_main]

use libfuzzer_sys::fuzz_target;
use usbguard_gui::rules::parse_query;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(rule) = parse_query(&text) {
        let rendered = rule.to_string();
        let reparsed = parse_query(&rendered);
        assert_eq!(reparsed.as_ref(), Ok(&rule), "rendered as {rendered:?}");
    }
});
