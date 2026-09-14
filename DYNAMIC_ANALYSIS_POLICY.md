# Dynamic Analysis Policy

USBGuardGUI uses property-based testing and fuzzing to exercise parsing and validation code
paths with generated inputs.

## Scope

The fuzzer exercises the following attack surfaces:

Everything reachable from data the program did not produce itself. In practice that means
everything arriving over D-Bus from the USBGuard daemon — device attributes, rule text, parameter
values, signal payloads, and error strings — all of which ultimately originate in a USB device's own
descriptors and are therefore attacker-chosen.

**Primary fuzz targets** (`fuzz/`):

- `rules::parser::parse_rule` — the rule-language parser. The single highest-value target: arbitrary
  bytes in, a typed structure out. Goal: zero panics, hangs, or unbounded allocations over 10⁶
  inputs.
- `rules::lexer::tokenize` — fuzzed separately so that a lexer crash is not masked by the parser
  rejecting the input earlier.
- `rules::render::render_rule` — fed structures produced by the parser, to prove the round-trip
  property `parse(render(parse(x))) == parse(x)` rather than just the absence of crashes.

**Property-tested surfaces** (`proptest`, in-tree tests):

- `rules::render` quoting and escaping — the injection-resistance property: no input string, however
  crafted, may render into text that re-parses as more than one field. This is the check that stops
  a device name from becoming rule syntax.
- `model::ids` and `model::target` parsing — `DeviceId`, `RuleId`, `UsbId`, `InterfaceType`,
  `Target`. Out-of-range and unknown values must map to the defined fallback variant, never panic.
- `dbus::client` attribute decoding — missing attributes, empty strings, and invalid UTF-8 must
  produce a partially populated device rather than an error or a panic.

**Deliberately not fuzzed:**

- The D-Bus wire layer itself. `zbus` owns deserialization from the wire; fuzzing it here would be
  fuzzing a dependency. What *is* fuzzed is everything downstream of the point where `zbus` hands
  over typed values.
- Anything in `ui/`. It has no parser, and a fuzzer cannot drive a GTK main loop meaningfully. UI
  robustness against hostile content is covered by the resilience matrix instead — specifically the
  device whose name contains quotes, backslashes, and non-UTF-8 bytes.

## Process for Vulnerabilities Found by the Fuzzer

1. **Triage**: a crash or logic error found by the fuzzer is assigned `P1` (Critical) priority in the
   issue tracker.
2. **Remediation**: a fix **must** be merged within **7 days** of confirmation.
3. **Verification**: after the fix, the fuzzer **must** run successfully on the updated codebase with
   a high example count (for example `max_examples=10000`) to confirm the issue is resolved. The
   failing input is added to the regression corpus.
4. **Documentation**: every confirmed vulnerability gets an entry in [`CHANGELOG.md`](CHANGELOG.md)
   under a `### Security` subsection, describing the issue and the fix.

## Assertions

Assertions are enabled during all test and fuzzing runs. Optimization flags that strip assertions are
**never** used in CI.

## Automation

The fuzzing workflow (`.github/workflows/fuzzing.yml`) runs automatically:

- On every pull request targeting `main`.
- Weekly, every Sunday at midnight UTC, via scheduled cron.
