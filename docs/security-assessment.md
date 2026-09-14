<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# USBGuardGUI — Security Assessment & Threat Model

This is the single source of truth for the USBGuardGUI threat model. Operational reporting
procedures live in [`SECURITY.md`](../SECURITY.md).

## Table of Contents

1. [Security Objectives](#1-security-objectives)
2. [Trust Boundaries & Assets](#2-trust-boundaries--assets)
3. [Threat Model (STRIDE)](#3-threat-model-stride)
4. [Known Limitations & Residual Risks](#4-known-limitations--residual-risks)
5. [Supply Chain Security](#5-supply-chain-security)
6. [Security Controls Summary](#6-security-controls-summary)
7. [Out of Scope](#7-out-of-scope)

---

## 1. Security Objectives

> **Status of this document.** The program is specified but not yet implemented. Every mitigation
> below is marked **Specified** rather than **Implemented** until the corresponding code exists and
> the test that proves it passes. This document is written first on purpose: the threat model is an
> input to the design, not a report on it.

USBGuardGUI is an unprivileged client that asks a root daemon to change USB device authorization.
It holds no privilege of its own and enforces no policy. Its security value is therefore narrow and
stateable: it must not acquire privilege, and it must not cause a privileged effect its user did not
ask for. Each objective below is falsifiable — an attacker, or a bug report, can demonstrate it
broken.

| # | Objective | Rationale |
| :- | :-------- | :-------- |
| O1 | The process acquires no privilege: no setuid bit, no file capabilities, no helper daemon, no privileged code path. | The entire design rests on this. If the program needs privilege, it is the wrong design. |
| O2 | The process reads and writes nothing under `/etc`, `/var`, or `/sys`. | A falsifiable restatement of O1 that a tool can check: `strace` filtered on those prefixes must report zero accesses. |
| O3 | No authorization change reaches the daemon that the user did not initiate and confirm for that specific device or rule. | The program's only dangerous capability is relaying a request. Relaying one nobody made is the worst outcome it can produce. |
| O4 | A rule removal removes the rule the user selected, or nothing. | Rule IDs are positional and shift under concurrent modification. "Nothing, with an error" is an acceptable outcome; "a different rule" is not. |
| O5 | No input reachable from the daemon causes a panic, a hang, or unbounded allocation. | Every byte of device and rule text originates outside the program's control. |
| O6 | Attacker-influenced text never becomes rule syntax. | A device name that closes a quote and appends a condition must render as a name, not as a rule fragment. |
| O7 | Device identifiers do not appear in default-level logs or in a crash report. | A log attached to a bug report must not be an inventory of the reporter's hardware. |
| O8 | The program contacts no network host. | Verifiable by denying the process all network access and observing no behavioural change. |

---

## 2. Trust Boundaries & Assets

### 2.1 Trust Boundaries

The program sits below every privilege boundary that matters. It is the *low* side of each one,
which is what makes the threat model tractable: there is no boundary this program defends, only
boundaries it crosses as a supplicant.

```text
[USB device] ──B1──▶ [usbguard-daemon, root] ──B2──▶ [usbguard-dbus bridge] ──B3──▶ [usbguard-gui, user]
                                                                                          │
                                                     [session bus: notifications, tray] ──B4
                                                                                          │
                                                                   [GSettings / dconf] ──B5
```

| Boundary | Untrusted side | Trusted side | Validation performed |
| :------- | :------------- | :----------- | :------------------- |
| B1 | The USB device, and every descriptor string it reports | `usbguard-daemon` | Upstream's concern, not this program's. The relevant consequence here is that device names, serials, and hashes are **attacker-chosen strings** by the time they reach B3. |
| B2 | — | — | Internal to upstream USBGuard. Listed because a change on either side of it changes what B3 delivers. |
| B3 | Everything arriving from `org.usbguard1`: device attributes, rule text, parameter values, signal payloads, error strings | `usbguard-gui` | Every field is parsed into a domain type before use; no field is interpolated into rule syntax without the canonical quoter; invalid UTF-8 and missing attributes degrade the row rather than failing the read; the parser is fuzzed. |
| B3′ | `usbguard-gui` (the request direction) | `usbguard-daemon` | **Three independent checkpoints outside this program**: the D-Bus bus policy (uid), Polkit (session subject), and the daemon's own IPC ACL. All three are closed by default in a stock configuration. The program's own validation is not a security control here — the daemon's is. |
| B4 | The notification server and the `StatusNotifierWatcher`, neither of which is guaranteed to exist or to be well-behaved | `usbguard-gui` | Both are optional. Absence degrades visibly; a misbehaving one can deny a feature but cannot reach the action path, because no notification reply is trusted to identify a device on its own. |
| B5 | — | `usbguard-gui` | GSettings holds presentation state only. Nothing read from it is security policy, and no value read from it can change what is sent across B3′. |

### 2.2 Protected Assets

| Asset | Confidentiality | Integrity | Availability |
| :---- | :-------------- | :-------- | :----------- |
| The USBGuard ruleset and device authorization state | Low — readable by anyone the daemon authorizes to read it | **Critical** — the program's only dangerous capability is changing it | Medium — degraded display is acceptable; a wrong display is not |
| Device identity (names, serial numbers, hashes) | **High** — an inventory of the user's hardware, and a fingerprint of the user | Low | Low |
| The user's Polkit authorization decision | Medium | **High** — an authorization obtained for one action must not be spent on another | Medium |
| The displayed state itself | Low | **High** — a view that disagrees with the daemon invites the user to make a decision on false information | Medium |
| Release artifacts | Low | **Critical** | Medium |

---

## 3. Threat Model (STRIDE)

For each component, enumerate threats under **S**poofing, **T**ampering, **R**epudiation,
**I**nformation disclosure, **D**enial of service, and **E**levation of privilege.

Severity is the impact on the objectives in §1, not a CVSS score. **Status** is `Specified` while
the mitigation exists only in `architecture.md`, and becomes `Implemented` when the code and the
test that proves it both exist.

### 3.1 D-Bus client and proxy layer (`src/dbus/`)

| STRIDE | Threat | Severity | Mitigation | Status |
| :----- | :----- | :------- | :--------- | :----- |
| S | A process other than the real bridge owns `org.usbguard1` and feeds the program a fabricated device list. | Medium | The system bus arbitrates well-known name ownership, and the bus policy governs who may own `org.usbguard1`. A local process that can take that name has already won at a lower layer. The program does not attempt to re-authenticate the daemon. | Accepted (see §4) |
| T | Device attributes or rule text crafted to break the display or the rule generator. | High | Every field is parsed into a domain type; the canonical renderer quotes and escapes everything; round-trip tests and a fuzz target cover the parser. | Specified |
| R | A change made through the GUI cannot be distinguished afterwards from one made through the CLI. | Low | The daemon is the audit point and logs its own changes; this program adds no audit surface and claims none. | Accepted |
| I | Error strings returned by the daemon are surfaced verbatim and may contain device identifiers. | Medium | Diagnostic text shown to the user is the program's own; daemon detail is attached only in the diagnostic panel, and never written to a default-level log. | Specified |
| D | An insertion burst from a hub with 40+ ports floods the event stream. | Medium | The event worker coalesces within a bounded latency window and resynchronizes rather than replaying per-event; T3 of the resilience matrix is the test. | Specified |
| E | — | — | The layer holds no privilege to elevate. Every privileged effect is the daemon's, behind three checkpoints this program does not control. | N/A |

### 3.2 Rule language parser and generator (`src/rules/`)

This is the component with the largest attack surface, because every byte it sees comes from outside
the program, and the component with the smallest blast radius, because it touches nothing but text.

| STRIDE | Threat | Severity | Mitigation | Status |
| :----- | :----- | :------- | :--------- | :----- |
| T | Rule injection: a device name containing a quote, a backslash, or a newline is interpolated into a generated rule and changes its meaning. | **Critical** | Rules are never produced by string concatenation. A rule is built as a typed structure and rendered by the canonical quoter; a round-trip check on the rendered output is a required test. | Specified |
| D | A pathological input causes a panic, an infinite loop, or unbounded allocation. | High | No `unwrap` and no panic in the parser, enforced by a Clippy `disallowed-methods` lint that fails CI; `cargo fuzz` over the parser with a target of zero panics in 10⁶ inputs; input length is bounded. | Specified |
| T | Non-UTF-8 bytes in a device name corrupt the displayed text or the generated rule. | Medium | Lossy decoding at the boundary, with the row shown with partial fields rather than dropped or panicking; T6 of the resilience matrix. | Specified |
| I | — | — | The parser holds no secret. | N/A |

### 3.3 Action path — device actions and rule modification (`src/ui/`, `src/dbus/commands.rs`)

| STRIDE | Threat | Severity | Mitigation | Status |
| :----- | :----- | :------- | :--------- | :----- |
| E | A Polkit authorization obtained for one action is used to perform a different one. | **Critical** | One authorization per method call, checked by the bridge, not by this program. The program issues exactly the call the user confirmed, with no batching and no deferred replay of a pending operation. | Specified |
| T | Rule-ID volatility: the ruleset shifts between reading and removing, and the wrong rule is removed. | **Critical** | Rules are held by canonical text, not by ID; the ruleset is re-read immediately before removal and the text re-matched; a mismatch yields `AlreadyGone` and aborts. Two textually identical rules produce a disambiguation dialog listing evaluation positions, never a guess. T4 and T5 are the tests. | Specified |
| T | An optimistic UI update makes the user believe a change took effect that the daemon rejected. | High | No optimistic updates. The view changes only on a daemon signal or a confirmed reply; an operation interrupted by a daemon restart is reported inconclusive and the state is re-read (T2). | Specified |
| S | A crafted notification reply causes an action on a device other than the one the notification was about. | High | Notification actions carry an opaque token resolved against the program's own live device map; a token that no longer resolves opens the window instead of acting. | Specified |
| D | A Polkit prompt the user leaves open for ten minutes causes the operation to time out and the UI to hang. | Medium | Operations are cancellable and have no client-side timeout shorter than the Polkit interaction; the target is zero timeouts on waits up to ten minutes (T13). | Specified |
| R | — | Low | The daemon records the change. | Accepted |

### 3.4 Session-bus integrations — notifications and tray (`src/notify.rs`, `src/tray.rs`)

| STRIDE | Threat | Severity | Mitigation | Status |
| :----- | :----- | :------- | :--------- | :----- |
| S | A hostile process registers as the `StatusNotifierWatcher` or the notification server and impersonates the program's UI. | Medium | Any process in the user's session can already do this to any application; it is a session-bus property, not a flaw here. The consequence is bounded: neither integration can cause an action, because the action path re-resolves every token against live state and every privileged effect still passes Polkit. | Accepted |
| I | A notification body displays a device name on a lock screen or a shared display. | Medium | Notification bodies carry the device's presentation name only, never its serial number or hash, and the notification can be disabled entirely with `notify-inserted`. | Specified |
| D | The tray's D-Bus name registration fails under Flatpak because it depends on the in-sandbox pid. | Low | Failure degrades to background mode, which remains fully functional, with a one-time visible notice rather than silence. | Specified |

### 3.5 Configuration, logging, and diagnostics (`src/config.rs`, `src/dbus/diagnostics.rs`)

| STRIDE | Threat | Severity | Mitigation | Status |
| :----- | :----- | :------- | :--------- | :----- |
| I | A log or a `--diagnose` output pasted into a public bug report reveals the user's hardware. | High | Device names, serial numbers, and hashes are never logged above `debug`, and the `debug` level says so in its own output. `--diagnose` reports access state and remedies, never a device list. | Specified |
| T | A tampered GSettings value changes what the program sends to the daemon. | Low | GSettings holds presentation state only. No key affects which request is sent or whether it is confirmed — a claim that is checkable by reading §2 of `interfaces.md` against the request path. | Specified |
| E | The diagnostic probe triggers a Polkit prompt during startup that the user cannot attribute to anything. | Medium | The probe sequence runs **after** the window is presented, never during startup, so any prompt it causes belongs to a window the user can see. The write-access probe is deliberately empty: the only honest test of write access is a write, and a test write would change the system's USB policy to find out whether it may. | Specified |

---

## 4. Known Limitations & Residual Risks

Five of these cannot be closed without a change to upstream USBGuard. All of them are detectable at
runtime, and each maps to a diagnostic state or to a test in the resilience matrix — which is the
property that matters: the program fails in a way it can explain.

| Risk | Severity | Why it is accepted | Compensating control |
| :--- | :------- | :----------------- | :------------------- |
| **The rule-removal race is irreducible client-side.** Between re-reading the ruleset and issuing the removal, another client can change it. | High | There is no compare-and-swap in the daemon's API. Closing it needs an upstream `removeRuleIfMatches(id, expected_text)`; proposing it is the highest-value contribution this project can make to USBGuard. | The window is narrowed to the round-trip of a single call, and the failure mode is biased toward refusing: a mismatch aborts with `AlreadyGone` rather than removing anything. |
| **Denial attribution depends on daemon error strings.** Which of the three checkpoints refused is inferred from text, not from a structured code. | Medium | It is the only hook the API offers. Strings change between USBGuard versions. | A `DeniedUnattributed` state that shows *both* remedies rather than guessing, keeping this a user-experience problem instead of a correctness one. Recalibrated per supported USBGuard version. |
| **The numeric target mapping is not a stable contract.** It derives from the declaration order of a C++ enum. | Medium | Upstream has never promised it. | A `Target::Other` variant confines the consequence to degraded display rather than a wrong authorization label. Detected by the Phase 0 introspection procedure. |
| **Rule text as identity assumes stable canonicalization.** If a future daemon normalizes rule text differently, held handles stop matching. | Medium | Text is the only stable identity the API exposes; IDs are positional. | Handles are re-read immediately before use, so the failure mode is `AlreadyGone` on a rule that still exists — confusing, but never destructive. |
| **Any process in the user's session can impersonate the session-bus services** the program uses. | Medium | This is a property of the session bus, shared by every desktop application. Defending it is out of this program's reach. | The action path never trusts a session-bus reply as sufficient authority: tokens are re-resolved against live state, and every privileged effect still passes Polkit. |
| **The program cannot verify that its D-Bus peer is the genuine bridge.** | Medium | Well-known name ownership is arbitrated by the bus, under the bus policy. A local process able to take `org.usbguard1` has already defeated a lower layer. | Bus policy (checkpoint A) is treated as a distinct diagnostic state rather than folded into "permission denied", so a hardened or altered policy is visible rather than silent. |
| **The tray's D-Bus name under Flatpak depends on the in-sandbox pid.** | Low | The workaround is empirical, not guaranteed. | Failure degrades to background mode with a visible notice. |
| **The bridge is optional packaging on every reference distribution.** | Low (security) / High (usability) | Not this project's decision to make. | `BridgeNotInstalled` is a first-class diagnostic state with a per-distribution install command — the single message most likely to determine whether a user ever sees the program work. |

---

## 5. Supply Chain Security

### 5.1 Release Artifact Integrity

- Every release ships a `SHA256SUMS` file covering all artifacts.
- Artifacts are signed with Sigstore keyless signing via the release workflow.
- A CycloneDX SBOM is attached to each release.
- Verification instructions: [`docs/verification.md`](verification.md).

### 5.2 Dependency Monitoring

- Dependabot monitors the manifests and the GitHub Actions workflows.
- `make audit` runs the vulnerability scanner in CI on every pull request.
- Remediation thresholds: [`SCA_POLICY.md`](../SCA_POLICY.md).

### 5.3 Trusted Code Paths

- All GitHub Actions are pinned to full commit SHAs.
- `GITHUB_TOKEN` permissions are declared read-only at workflow level and elevated per job only when
  required.
- Branch protection requires review and passing status checks before merge.

---

## 6. Security Controls Summary

| Control | Type | Where implemented | Verified by |
| :------ | :--- | :---------------- | :---------- |
| Input validation | Preventive | `src/rules/` — parser, lexer, canonical renderer; `src/dbus/client.rs` — every field parsed into a domain type at the boundary | Round-trip property tests, `cargo fuzz`, resilience tests T5 and T6 |
| Least privilege | Preventive | The whole design: no setuid, no capabilities, no helper daemon, no access to `/etc`, `/var`, or `/sys` | `strace` filtered on those three prefixes must report zero accesses (§13.2) |
| Fail-closed defaults | Preventive | `default-persistence` defaults to runtime-only; no optimistic UI updates; `AlreadyGone` aborts rather than guessing; `DeniedUnattributed` shows both remedies rather than the likelier one | Resilience tests T2, T4, T5 |
| No-panic discipline | Preventive | `clippy.toml` `disallowed-methods` lint over the parser and the runtime boundary | `cargo clippy --all-targets -- -D warnings` in CI |
| Identifier redaction | Preventive | `src/` logging policy: device names, serials, and hashes never above `debug` | Review, plus a test asserting a default-level log contains no identifier |
| Static analysis | Detective | CodeQL, linter | CI |
| Dependency scanning | Detective | Dependabot, `make audit` | CI |
| Fuzzing | Detective | `make fuzz` | CI (weekly) |

---

## 7. Out of Scope

Stating these protects both users and maintainers. None of the following is a vulnerability in this
project; a report about one will be closed with a pointer to this section.

- **Weaknesses in USBGuard itself** — the daemon, the D-Bus bridge, the shipped bus policy, or the
  IPC access-control implementation. This project is a client of that contract. That the daemon
  authorizes too much, or enforces a rule incorrectly, belongs
  [upstream](https://github.com/USBGuard/usbguard). That *this program* misreads or misrepresents
  the daemon's answer does not.
- **An administrator's decision to grant access.** Installing a Polkit rule that lets an
  unprivileged user change USB policy is a deliberate act with understood consequences. The example
  rule ships inert under `/usr/share/doc/` and is never installed. That it *can* be enabled is the
  design.
- **An already-compromised session.** A process running as the user can read the same D-Bus
  interfaces, drive the same Polkit prompts, and impersonate the same session-bus services. Nothing
  this program does adds or removes capability there.
- **Physical attacks on the machine**, including BadUSB-class devices. Defending against those is
  USBGuard's purpose; this program is one way to configure it, not an additional layer.
- **Denial of service by the daemon or the bus** — a stopped service, a refused connection, a
  hanging call. These are diagnosed and reported, not defended against.
- **Kernel and USB-stack vulnerabilities**, and anything reachable before the daemon makes an
  authorization decision.
- **Non-Linux platforms.** USBGuard is Linux-only, and so is this.
