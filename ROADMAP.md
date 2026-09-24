# USBGuardGUI Development Roadmap

This document outlines the **realistic, near-term** development plan for USBGuard GUI. Items
beyond this scope are tracked as ideas in
[GitHub Issues](https://github.com/onyks-os/USBGuardGUI/issues) rather than as committed
release dates.

The ordering below is deliberately **not** the dependency order of
[`docs/architecture.md`](docs/architecture.md). That document is organized by what depends on what;
this one is organized by what can be built, tested, and understood in isolation first. The two
disagree in one important place: the GTK/Tokio bridge (§5.3–5.4) is architecturally central and is
scheduled last, because it is the one piece that cannot be verified without everything around it
already working.

No milestone carries a date. The architecture document's own estimate assumes fluency in async
Rust; treat elapsed time as an output of the work, not an input to it.

---

## Current Status (v0.1.0 — unreleased)

Delivered:

- Complete technical architecture specification, and the project scaffold, CI, and governance.
- **Phase 0**: introspection, Polkit actions, bus policy, error shapes, signal and target
  mappings, and the IPC identity test, all on usbguard 1.1.4; packaging of Debian, Ubuntu, and
  Arch checked in containers (`docs/architecture.md` §13.4).
- **Phase 1**: the rule-language parser and renderer, with property tests and two cargo-fuzz
  targets (10⁶ inputs each, no findings).
- **Phase 2**: typed D-Bus client, mock bridge for tests, `--list-devices` / `--list-rules`.
- **Phase 3**: the probe sequence, denial attribution, per-distribution remedies, `--diagnose`.
- **Phase 4 / v0.1.0**: runtime bootstrap, channel bridge, coalescing worker, supervisor with
  reconnection, the window, GSettings, desktop file, AppStream metainfo, and icon. T1, T3, T4, T5,
  T6, and T15 pass against the mock. Memory: PSS within target; RSS within target only with the
  software renderer (`docs/architecture.md` §13.2).
- **v0.2.0**, code complete: device actions with the runtime/permanent choice, the policy view
  with safe removal and the disambiguation dialog, the new-rule dialog (guided and text modes),
  cancellable operations with the 20 s / 180 s notices, runtime parameters, preferences.
- **Later, done**: notifications with a quick action (§9.5), background mode and the tray icon
  (§9.6), start at login. Still to do: localization (§9.7) and Flathub submission.
- **Packaging**: `.deb` (built on Ubuntu 24.04, installs on Debian and Ubuntu), `.rpm` (installs
  on Fedora, pulling in `usbguard-dbus`), an Arch `PKGBUILD`, and a Flatpak manifest.

No release exists yet. Before tagging: the open Phase 0 items below, a manual pass over the
v0.2 actions on a real system, and screenshots for the AppStream metadata.

---

## Phase 0 — Verify the upstream contract

**Goal:** replace every assumption about USBGuard's D-Bus interface with an observation, before any
code depends on one.

No code is written in this phase. Every statement in `docs/architecture.md` marked **[P0]** is an
assumption; introspection settles it, and where introspection disagrees with the document, the
document is corrected.

| Item | Description |
| :--- | :---------- |
| **Interface introspection** | Full signatures of the three interfaces, captured with `gdbus introspect` and committed to `docs/dbus-introspection/<distro>-<version>.xml`. |
| **Signal observation** | `gdbus monitor` during real insertion and removal, to see what the daemon actually emits and in what order. |
| **Polkit action identifiers** | Read from the installed policy file rather than assumed. |
| **Bus policy** | The contents of `org.usbguard1.conf` as the distribution ships it — checkpoint A. |
| **Rule corpus** | A collection of real `usbguard list-rules` output, which becomes the fixture set for the parser in Phase 1. |

**Exit criterion:** the `[P0]` markers in `docs/architecture.md` are resolved, and the introspection
XML is committed.

---

## Phase 1 — The rule language, and the type system

**Goal:** a parser and renderer for the USBGuard rule language that round-trips every rule in the
Phase 0 corpus, with no daemon, no display, and no async.

This is scheduled first among the code phases because it is the only component that can be proven
correct in isolation: string in, struct out, struct back to string. It is also where the type system
is learned — `enum`, pattern matching, `Result` and `?`, `&str` against `String`, iterators, and
Cargo's integrated tests — without the compiler's ownership rules being tangled up with a runtime.

| Item | Description |
| :--- | :---------- |
| **Newtypes and enums** | `DeviceId`, `RuleId`, `UsbId`, `InterfaceType`, `Target`, `Persistence` (§6.1). Twenty lines that teach the type system better than a chapter of a book. |
| **Lexer and parser** | `src/rules/lexer.rs`, `src/rules/parser.rs` (§7). No `unwrap`, no panic, no unbounded allocation — every input originates outside the program. |
| **Canonical renderer** | `src/rules/render.rs`: quoting, escaping, and a round-trip check. |
| **Round-trip property tests** | `proptest` over generated rules, plus the Phase 0 corpus as a fixture set. |
| **Fuzz target** | `cargo fuzz` over the parser, wired into CI. Target: zero panics over 10⁶ inputs. |
| **Error taxonomy** | `ParseError` and the shape of `AppError` (§6.3). |

**Exit criterion:** `make verify` is green, parser line coverage is above 90%, and a short fuzz run
in CI finds nothing.

---

## Phase 2 — A command-line prototype

**Goal:** read the device list and the ruleset from the running daemon and print them. Async, but
the minimum amount, in a context where a mistake ends the process instead of corrupting a UI.

| Item | Description |
| :--- | :---------- |
| **D-Bus proxies** | `src/dbus/proxies.rs` (§8) — the only place raw signatures appear, generated from the Phase 0 XML. |
| **Typed client** | `src/dbus/client.rs`: the proxies wrapped in the domain types from Phase 1. |
| **Mock daemon** | A stub implementation of the three interfaces on a private bus, so the D-Bus tests never touch a live system daemon. |
| **`usbguard-gui --version`** | Version, plus the USBGuard version detected on the bus. |

**Exit criterion:** the prototype's device list matches `usbguard list-devices` on a real system.

---

## Phase 3 — Diagnostics

**Goal:** the program can explain, precisely, why it cannot reach the daemon.

A good exercise in modelling with enums, and the component that decides whether most users ever see
the program work at all — the D-Bus bridge is optional packaging on every reference distribution,
so "bridge not installed" will be the single most common first-run outcome.

| Item | Description |
| :--- | :---------- |
| **`AccessState`** | The nine-variant state type of §4.1. |
| **Probe sequence** | §4.2, in order, stopping at the first failure. Probe 6 stays deliberately empty: the only honest test of write access is a write. |
| **Denial attribution** | §4.3 — classifying which of the three checkpoints refused, with `DeniedUnattributed` as the honest fallback. |
| **Per-distribution remedies** | The install command for the bridge on Fedora, Debian, Ubuntu, and Arch. |
| **`usbguard-gui --diagnose`** | The probe sequence as a headless command, exercisable in CI without a display. |

**Exit criterion:** T8 and T9 of the resilience matrix pass — bridge absent and bridge stopped are
reported as *different* states, each with its own remedy.

---

## Phase 4 — v0.1.0: a window that lists devices, read-only

**Goal:** something that runs on the author's desktop and shows the truth about it.

Deliberately far smaller than the program described in `docs/architecture.md` §1.2. No tray, no
notifications, no rule editing, no device actions, no runtime-parameter changes — those are the
milestones after this one. A working read-only window is worth more than a half-finished complete
architecture.

| Item | Description |
| :--- | :---------- |
| **Runtime bootstrap** | §5.2 — the Tokio runtime alongside the GLib main loop, and the `clippy.toml` `disallowed-methods` lint that keeps them apart. |
| **The channel bridge** | §5.3 — immutable `UiEvent` values over an executor-agnostic channel. The hardest single piece in the project; approach it already knowing what `Send`, `Sync`, and `Arc` mean and why an `Rc` does not cross threads. |
| **Event worker and coalescing** | §5.4 — insertion bursts absorbed under a fixed latency ceiling. |
| **Supervision and reconnection** | §5.5 — backoff, and cache invalidation on return rather than trusting stale state. |
| **Window shell and device view** | §9.1, §9.2 — `AdwApplication`, the view stack, and the device list. |
| **Diagnostic panel** | §9 — the Phase 3 state surfaced in the interface, not just on stdout. |

**Exit criterion:** the window survives T1 (bridge stopped with the window open), T3 (a 40-device
insertion burst, against the mock), and T6 (a device name containing quotes, backslashes, and
non-UTF-8 bytes) — and idles under the memory target of §13.2.

---

## Next — v0.2.0: acting on policy

**Goal:** the window can change what it displays, with the runtime/persistent distinction made
explicit at every step.

| Item | Description |
| :--- | :---------- |
| **Device actions** | Allow, block, reject — each with an explicit choice between a runtime-only effect and a persisted rule, preselected to runtime-only. |
| **Policy view** | §9.3 — the ruleset in evaluation order. |
| **Safe rule removal** | §6.4 — rule text as identity, re-read immediately before use, `AlreadyGone` rather than removing the wrong rule. |
| **New-rule dialog** | §9.4, built on the Phase 1 parser and renderer. |
| **Cancellable operations** | §5.7 — a Polkit prompt the user leaves open for ten minutes must not time out. |
| **Runtime parameters** | `ImplicitPolicyTarget` and `InsertedDevicePolicy`, where the access model permits. |

---

## Later

Not committed, and in no particular order: localization (§9.7) and Flathub submission (§14.1).
Notifications, the tray, background mode, the Flatpak manifest, and native packages are done.

One upstream contribution is worth more than any of them: proposing
`removeRuleIfMatches(id, expected_text)` to USBGuard would eliminate the rule-removal race at its
root rather than narrowing it client-side.

---

## Explicitly Out of Scope

Stating what the project will *not* do is as valuable as stating what it will.

- **Editing `usbguard-daemon.conf` or the IPC access-control files.** Both are root-owned `0600`
  files. Editing them would require privilege escalation and would contradict the program's central
  property. The program diagnoses the configuration and prints the command; it does not run it.
- **Installing the Polkit rule.** An example is shipped under `packaging/` and never activated.
  Granting an unprivileged user the right to change USB policy is the administrator's decision.
- **Offline editing of `rules.conf`** without a running daemon. The file is unreadable to an
  unprivileged user, and writing it behind the daemon's back desynchronizes its in-memory ruleset
  from disk.
- **Managing `RuleFolder` entries as distinct objects.** The daemon exposes the effective ruleset
  already aggregated; there is no D-Bus accessor for the per-folder decomposition.
- **Replacing the `usbguard` CLI.** Operations with no D-Bus equivalent — `generate-policy`,
  `add-user`, `read-descriptor` — are not reimplemented.
- **Any privileged code path**: no setuid bit, no helper daemon, no capability acquisition. If a
  feature needs one, the feature does not happen.
- **Non-Linux platforms.** USBGuard is Linux-only.

---

## How to Influence the Roadmap

Open an issue describing the use case you are blocked on. Roadmap items are prioritized by the number
of users affected and by alignment with the project's stated goals, not by request order.
