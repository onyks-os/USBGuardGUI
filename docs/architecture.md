# USBGuardGUI — Technical Architecture Specification

This document is the complete technical description of the software: what it
is, what it talks to, how it is structured internally, how it behaves under
failure, and how it is verified. It is written to be sufficient on its own —
a reader should be able to implement the program from this document plus the
upstream USBGuard manual pages.

Statements marked **[P0]** are assumptions about the upstream D-Bus contract
that must be confirmed by introspection against the target USBGuard build
before any code depends on them (procedure in §13.3). Where introspection
disagrees with this document, introspection wins and this document is
corrected.

---

## 1. Purpose and Scope

### 1.1 Objective

Provide a graphical interface for USBGuard that lets a desktop-session user
inspect and manage USB device authorization policy.

The program is an **unprivileged client**. It contains no setuid bit, no
helper daemon of its own, and no privileged code path. It reaches the USBGuard
daemon exclusively through the system D-Bus bus, and it never reads or writes
any file under `/etc`, `/var`, or `/sys`. Every privileged effect is produced
by the USBGuard daemon on its own authority, after the system's own
authorization layers (§3) have approved the request.

### 1.2 In Scope

- **Device view** — the devices known to the daemon, with authorization state,
  updated live from daemon signals, resilient to insertion bursts.
- **Policy view** — the active ruleset in evaluation order; appending and
  removing rules, with correct handling of rule-id volatility.
- **Device actions** — allow, block, reject, each with an explicit choice
  between a runtime-only effect and a persisted rule.
- **Runtime parameters** — read and, where permitted, modify
  `ImplicitPolicyTarget` and `InsertedDevicePolicy`.
- **Access diagnostics** — a module that distinguishes each distinct reason
  communication can fail and states the specific remedy for each (§4).
- **Notifications** — desktop notifications for newly inserted, not-yet-authorized
  devices, with quick actions where the notification server supports them.
- **Background mode and tray** — a `StatusNotifierItem` status icon where the
  desktop provides a watcher, with explicit, non-silent degradation where it
  does not.

### 1.3 Out of Scope

- **Editing `usbguard-daemon.conf` or the IPC access-control files.** Both live
  in root-owned `0600` files (verified: §2.1). Editing them would require
  privilege escalation and would contradict §1.1. The program diagnoses the
  configuration and tells the user exactly what to run; it does not perform it.
- **Installing the Polkit rule.** See §3.6.
- **Offline editing of `rules.conf`** without a running daemon. The file is not
  readable by an unprivileged user, and writing it behind the daemon's back
  desynchronizes the daemon's in-memory ruleset from disk.
- **Managing `RuleFolder` entries as distinct objects.** The daemon exposes the
  effective ruleset already aggregated through `listRules`; there is no D-Bus
  accessor for the per-folder decomposition.
- **Replacing the `usbguard` CLI.** Operations with no D-Bus equivalent
  (`generate-policy`, `add-user`, `read-descriptor`) are not reimplemented.

### 1.4 Target Environment

| Component | Requirement | Note |
|---|---|---|
| USBGuard daemon | 1.1.0 or later | Earlier versions have a different `appendRule` arity (§2.4.6) |
| USBGuard D-Bus bridge | matching version | Separate package (§2.1) — its absence is a distinct diagnostic state |
| GTK | 4.14 or later | Baseline for the widgets used in §9 |
| libadwaita | 1.5 or later | Adaptive layout, `AdwToastOverlay`, `AdwAlertDialog` |
| Session | Wayland or X11 | No compositor-specific code |
| Polkit | any version with a running session agent | §3.4 |

Reference distributions for verification: Fedora, Debian stable, Ubuntu LTS,
Arch. These are the four whose USBGuard packaging differs enough to matter.

### 1.5 Naming and Identity

These identifiers are fixed and appear consistently across the source tree,
the packaging, and the desktop integration files.

| Kind | Value |
|---|---|
| Cargo package and binary | `usbguard-gui` |
| Application ID (Flatpak, D-Bus, `.desktop`, metainfo) | `io.github.onyks_os.UsbguardGui` |
| GSettings schema ID | `io.github.onyks_os.UsbguardGui` |
| Icon name | `io.github.onyks_os.UsbguardGui` |
| Log/tracing target prefix | `usbguard_gui` |

The application ID uses `onyks_os`, not `onyks-os`: an application ID is also
a D-Bus well-known name, and D-Bus name elements admit `[A-Za-z0-9_-]` but
GNOME's convention — and `GApplication`'s validation of an application ID —
rejects a hyphen in this position. The underscore is the standard substitution.

---

## 2. The Upstream Contract

### 2.1 Component Topology

```text
  usbguard-gui  (this program, desktop user, unprivileged)
        │
        │  system D-Bus, name org.usbguard1
        ▼
  usbguard-dbus  (bridge, runs as root — SEPARATE PACKAGE)
        │
        │  USBGuard IPC (unix socket)
        ▼
  usbguard-daemon  (root, owns the policy and the ruleset)
        │
        │  netlink uevent + sysfs
        ▼
  kernel USB authorization
```

The bridge is packaged separately from the daemon on every reference
distribution. On Fedora the split is:

| Package | Provides |
|---|---|
| `usbguard` | `/usr/bin/usbguard-daemon`, `/usr/bin/usbguard`, `usbguard.service`, `/etc/usbguard/` |
| `usbguard-dbus` | `/usr/bin/usbguard-dbus`, `usbguard-dbus.service`, `/usr/share/dbus-1/system-services/org.usbguard1.service`, `/usr/share/dbus-1/system.d/org.usbguard1.conf`, `/usr/share/polkit-1/actions/org.usbguard1.policy` |

**Consequence for this program:** a system with USBGuard fully configured and
running can still be completely unreachable, because the bridge package is
simply not installed. This is not an edge case; it is the default state of a
stock Fedora install that has enabled USBGuard. The diagnostic module treats it
as a first-class state with its own remedy (§4), and it is distinguishable at
runtime without any filesystem access (§4.2, probe 2b).

`/etc/usbguard/usbguard-daemon.conf` and `/etc/usbguard/rules.conf` are mode
`0600`, owner `root` (verified on the reference system). No part of this
program attempts to read them; every statement the diagnostic panel makes about
their content is inferred from daemon behaviour, never from the files.

### 2.2 D-Bus Interfaces

Bus: **system**. Well-known name: `org.usbguard1`. The name is D-Bus
activatable through `org.usbguard1.service`, which matters for probe 2b.

#### `org.usbguard1` — object path `/org/usbguard1`

| Member | Signature | Semantics |
|---|---|---|
| `getParameter` | `(s) → s` | Read a runtime parameter by name (§2.5) |
| `setParameter` | `(s,s) → s` | Set a parameter; **returns the previous value** |
| `PropertyParameterChanged` | signal `(s,s,s)` | `name`, `value_old`, `value_new` |
| `ExceptionMessage` | signal `(s,s,s)` | `context`, `object`, `reason` — asynchronous daemon-side error |

#### `org.usbguard.Devices1` — object path `/org/usbguard1/Devices`

| Member | Signature | Semantics |
|---|---|---|
| `listDevices` | `(s) → a(us)` | `query` in rule-language syntax; returns `(device_id, device_rule)` pairs |
| `applyDevicePolicy` | `(uub) → u` | `id`, `target`, **`permanent`**; returns a rule id that is meaningful **only** when `permanent` is true |
| `DevicePresenceChanged` | signal `(uuusa{ss})` | `id`, `event`, `target`, `device_rule`, `attributes` |
| `DevicePolicyChanged` | signal `(uuusua{ss})` | `id`, `target_old`, `target_new`, `device_rule`, `rule_id`, `attributes` |

#### `org.usbguard.Policy1` — object path `/org/usbguard1/Policy`

| Member | Signature | Semantics |
|---|---|---|
| `listRules` | `(s) → a(us)` | `query` in rule-language syntax; returns `(rule_id, rule_text)` **in evaluation order** |
| `appendRule` | `(sub) → u` | `rule`, `parent_id`, **`temporary`**; returns the new rule id |
| `removeRule` | `(u) → ()` | Remove by rule id |

**[P0]** Every signature above is confirmed against the target build before
use. The named formal parameter of `listRules` has been renamed upstream over
time; the client binds **by position**, never by name, and the published online
documentation is known to lag the source.

### 2.3 Enumerations

Neither enumeration is a stable contract: both are the declaration order of a
C++ enum in the daemon's source. Both are verified empirically in Phase 0
(§13.3) and both are decoded through a total function with a catch-all arm.

**`event` field of `DevicePresenceChanged`** — `DeviceManager::EventType`:

| Value | Meaning |
|---|---|
| 0 | `Present` |
| 1 | `Insert` |
| 2 | `Update` |
| 3 | `Remove` |

**`target` fields** — `Rule::Target`, declaration order
`Allow, Block, Reject, Match, Device, Unknown, Empty, Invalid`. Only the first
three ever describe an actual device state:

| Value | Meaning |
|---|---|
| 0 | `Allow` |
| 1 | `Block` |
| 2 | `Reject` |
| ≥3 | not a device state |

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Allow,
    Block,
    Reject,
    /// Any value outside the three device states, kept verbatim.
    Other(u32),
}

impl From<u32> for Target {
    fn from(v: u32) -> Self {
        match v {
            0 => Target::Allow,
            1 => Target::Block,
            2 => Target::Reject,
            n => Target::Other(n),
        }
    }
}
```

`Target::Other` is not defensive boilerplate. If a future daemon reorders the
enum, the interface renders "unknown" instead of confidently labelling a
blocked device as allowed — the only failure direction acceptable in a security
tool. The reverse conversion, used to build `applyDevicePolicy` calls, is
fallible and rejects `Other` at the type level, so an unknown state can never
be echoed back to the daemon as a command.

### 2.4 Invariants and Traps

These are properties of the upstream API that the architecture is shaped
around. Each one, ignored, produces a specific and plausible defect.

**2.4.1 — Two unrelated kinds of integer id.**
The *device id* is the daemon's internal table index for a device: the first
column of `usbguard list-devices`, the `id` field of the device signals, the
first argument of `applyDevicePolicy`. It has nothing whatsoever to do with the
device's `id` **attribute**, which is the `vendor:product` pair. The *rule id*
indexes the ruleset. Each gets a distinct newtype:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(pub u32);
```

Neither implements `From<u32>` implicitly at call sites that cross the D-Bus
boundary; construction is explicit and localized to the proxy wrapper layer.
This single decision removes the most likely defect class in the project.

**2.4.2 — Rule ids are volatile.**
They are reassigned whenever the ruleset is reloaded or modified, including by
another client or by the `usbguard` CLI. No cache may assume a rule id stays
valid across any interval. §6.4 defines the identity protocol that follows from
this.

**2.4.3 — `listDevices` and `listRules` return rule-language text.**
There is no structured accessor for name, serial, or vendor:product. Populating
the device table columns therefore *requires* a client-side parser of the rule
language (§7). Signals, by contrast, carry an already-structured `a{ss}`
attribute dictionary, so the hot path does not parse.

**2.4.4 — `DevicePresent` does not reach D-Bus clients.**
The upstream documentation states it explicitly: client connections are
established after present-device processing, so the signal is emitted before
any client can be listening. Initial state is obtainable **only** through
`listDevices`. The architecture never waits for it.

**2.4.5 — `DevicePresenceChanged` is not sufficient alone.**
It does not indicate whether the `target` it carries is final or whether a
policy decision is still to be applied. `DevicePolicyChanged` is what reports
the decision. Both must be subscribed, and merged (§5.4).

**2.4.6 — `appendRule` takes a parent id, not a position.**
`parent_id` names the rule *after which* the new rule is inserted. The value
`UINT32_MAX - 2` is the largest representable id and is the upstream convention
for "append at the end of the ruleset".

```rust
/// Conventional `parent_id` meaning "append at the end of the ruleset".
pub const RULE_PARENT_APPEND_LAST: u32 = u32::MAX - 2; // 4_294_967_293
```

**[P0]** The `temporary` parameter of `appendRule` does not exist in USBGuard
before 1.1.0. The introspected arity determines whether the program offers the
runtime-only option for rules at all; where the parameter is absent, the
control is hidden rather than sent and rejected.

**2.4.7 — The persistence booleans have opposite polarity.**
`Policy1.appendRule` takes `temporary`; `Devices1.applyDevicePolicy` takes
`permanent`. They express the same concept inverted. The rest of the program
never sees a bare boolean:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persistence {
    /// Written to the ruleset; survives a daemon restart.
    Permanent,
    /// In-memory only; lost on daemon restart.
    RuntimeOnly,
}

impl Persistence {
    /// For `Policy1.appendRule`, whose parameter is `temporary`.
    pub fn as_temporary(self) -> bool { matches!(self, Persistence::RuntimeOnly) }
    /// For `Devices1.applyDevicePolicy`, whose parameter is `permanent`.
    pub fn as_permanent(self) -> bool { matches!(self, Persistence::Permanent) }
}
```

The two accessors are the only places in the source where these booleans are
produced, and they are unit-tested against each other.

**2.4.8 — `applyDevicePolicy`'s return value is conditionally meaningless.**
It is the id of the rule the daemon created or modified, and only when
`permanent` was true. Otherwise it must be discarded, not stored. The wrapper
returns `Option<RuleId>` accordingly.

**2.4.9 — `listDevices` result order is unspecified.**
Sorting is a presentation decision made by the client (§9.2). Do not rely on
the returned order being stable between calls.

**2.4.10 — There is no signal for "the ruleset changed".**
`PropertyParameterChanged` covers runtime parameters only. Ruleset cache
invalidation is therefore inferential; §5.6 gives the complete rule.

### 2.5 Runtime Parameters

Reachable through `getParameter` / `setParameter`. The daemon accepts only a
closed set of names; an unknown name produces an error, so the program does not
enumerate speculatively.

| Name | Values | Meaning |
|---|---|---|
| `ImplicitPolicyTarget` | `allow`, `block`, `reject` | Treatment of a device matching no rule |
| `InsertedDevicePolicy` | `block`, `reject`, `apply-policy` | Treatment of a device inserted while the daemon runs |

`getParameter("ImplicitPolicyTarget")` doubles as the diagnostic probe (§4.2):
it is the cheapest call that exercises the full authorization path without
observing or modifying anything of consequence.

Other settings named in `usbguard-daemon.conf(5)` — `PresentDevicePolicy`,
`PresentControllerPolicy`, `AuthorizedDefault`, `RestoreControllerDeviceState`,
`DeviceManagerBackend`, `DeviceRulesWithPort`, `AuditBackend`, `RuleFile`,
`RuleFolder`, the `IPC*` family — are **startup configuration, not runtime
parameters**. They are not settable over D-Bus at all. The interface may
describe them in help text; it never offers to change them.

### 2.6 Rule Language

The complete grammar the parser (§7) must accept, as specified by
`usbguard-rules.conf(5)`.

```ebnf
rule            ::= target { attribute } [ condition-clause ] [ comment ]
partial-rule    ::=        { attribute } [ condition-clause ] [ comment ]
target          ::= "allow" | "block" | "reject"

attribute       ::= name single-value
                  | name [ set-operator ] "{" { value } "}"

set-operator    ::= "all-of" | "one-of" | "none-of"
                  | "equals" | "equals-ordered" | "match-all"
                                  (* absent ⇒ "equals" *)

condition-clause ::= "if" [ "!" ] condition
                   | "if" [ set-operator ] "{" { [ "!" ] condition } "}"

comment         ::= "#" { any-character-to-end-of-line }
```

**Attributes.** `id`, `hash`, `parent-hash`, `name`, `serial`, `via-port`,
`with-interface`, `with-connect-type`, `label`.

`label` is metadata: it is stored on the rule and is available for filtering,
and it takes no part in deciding whether a rule matches a device.

**Value forms.**

| Attribute | Value shape |
|---|---|
| `id` | `vendor:product`, two 16-bit hex numbers; `*` allowed as `*:*` or `1234:*` |
| `hash`, `parent-hash` | quoted string |
| `name`, `serial`, `label` | quoted string |
| `via-port` | quoted string; `usbN` for a root hub, otherwise `bus-port[.port…]` (e.g. `1-2`, `1-2.1`) |
| `with-interface` | `cc:ss:pp`, three 8-bit hex numbers; `*` permitted for subclass and protocol, and **if subclass is `*` then protocol must also be `*`** |
| `with-connect-type` | quoted string |

**Conditions.** `localtime(HH:MM[:SS][-HH:MM[:SS]])`, `allowed-matches(query)`,
`rule-applied`, `rule-applied(duration)`, `rule-evaluated`,
`rule-evaluated(duration)`, `random`, `random(p)`, `true`, `false`. A duration
is `HH:MM:SS`, `HH:MM`, or `SS`. Inside a condition set, the operators `equals`
and `equals-ordered` are both synonyms for `all-of`.

**Set-operator semantics on attributes** — the device's attribute set versus
the values written in the rule:

| Operator | Matches when |
|---|---|
| `all-of` | the device set contains all listed values |
| `one-of` | the device set contains at least one listed value |
| `none-of` | the device set contains no listed value |
| `equals` | the device set is exactly the listed set |
| `equals-ordered` | exactly the listed set, in the listed order |
| `match-all` | the device set is a subset of the listed values |

Note `match-all` in particular: it is easy to omit when transcribing the
attribute grammar from memory, and omitting it makes the parser reject valid
policies.

**Partial rules** — a rule without a target — are produced by some `usbguard`
CLI subcommands. The parser accepts them and represents the missing target as
such, rather than defaulting it.

**Query strings are not rules.** The `query` argument of `listDevices` and
`listRules` is written in the same syntax but is matched, not enforced, and it
additionally accepts `match` as a target — which is how "everything" is
expressed, and which is *not* valid inside `rules.conf`. The parser therefore
exposes two entry points with different target sets, and the one used for
policy text rejects `match`.

---

## 3. Access Model

An unprivileged process asking a root daemon to change USB authorization passes
through **three independent checkpoints**, each enforced by a different
component, each evaluating a possibly different identity, and each closed by
default in a stock configuration. "Runs without root" is a statement about this
program's own privileges; it is not a statement that the call will succeed.

```text
  usbguard-gui ──▶ [A] D-Bus bus policy ──▶ [B] Polkit ──▶ [C] USBGuard IPC ACL ──▶ daemon
                    enforced by:            enforced by:    enforced by:
                    dbus-broker             usbguard-dbus   usbguard-daemon
                    identity: caller uid    identity:       identity: the process
                                            caller's        connected to the IPC
                                            polkit subject  socket (§3.3)
```

### 3.1 Checkpoint A — D-Bus Bus Policy

`/usr/share/dbus-1/system.d/org.usbguard1.conf`, shipped by the bridge package,
decides which uids may send messages to the name `org.usbguard1` at all. It is
evaluated by the bus daemon before the bridge process is ever involved.

A denial here surfaces as `org.freedesktop.DBus.Error.AccessDenied` with no
Polkit prompt and no daemon-side trace. The usual shipped policy delegates the
real decision to Polkit rather than restricting by uid, so this checkpoint is
normally transparent — but a hardened or distribution-patched policy file can
close it, and it is the one denial the user cannot fix from a Polkit rule.
The program therefore keeps it as a distinct diagnostic outcome rather than
folding it into "permission denied".

### 3.2 Checkpoint B — Polkit

The bridge checks a Polkit action per method call, against the calling
process's Polkit subject. The actions are declared in
`/usr/share/polkit-1/actions/org.usbguard1.policy`, shipped by the bridge
package, and default to `auth_admin`.

**[P0]** The exact action identifiers are read from the installed policy file
at Phase 0 rather than assumed. The expected set is:

| Action | Used for |
|---|---|
| `org.usbguard.Devices1.listDevices` | populating the device view |
| `org.usbguard.Devices1.applyDevicePolicy` | allow / block / reject |
| `org.usbguard.Policy1.listRules` | populating the policy view |
| `org.usbguard.Policy1.appendRule` | adding a rule |
| `org.usbguard.Policy1.removeRule` | removing a rule |
| `org.usbguard1.getParameter` | reading runtime parameters, and the probe |
| `org.usbguard1.setParameter` | changing runtime parameters |

**Read operations are gated too.** With the shipped defaults, merely filling the
device table at startup raises an administrator password prompt. This has a
direct architectural consequence: initial synchronization cannot be silent and
automatic, or the user is asked for a password immediately on launch with no
context for the request. The program runs the probe of §4.2 first, and only
after the main window is on screen, so the prompt is attributable to a visible
application the user just started.

**Reference Polkit rule**, for `/etc/polkit-1/rules.d/70-usbguard-gui.rules` —
reads without authentication, writes with the user's own password, remembered:

```javascript
// Adjust "wheel" to the distribution's administrator group:
// wheel on Fedora/Arch, sudo on Debian/Ubuntu.
polkit.addRule(function(action, subject) {
    if (!subject.local || !subject.active || !subject.isInGroup("wheel")) {
        return;
    }
    var readOnly = [
        "org.usbguard.Devices1.listDevices",
        "org.usbguard.Policy1.listRules",
        "org.usbguard1.getParameter"
    ];
    var mutating = [
        "org.usbguard.Devices1.applyDevicePolicy",
        "org.usbguard.Policy1.appendRule",
        "org.usbguard.Policy1.removeRule",
        "org.usbguard1.setParameter"
    ];
    if (readOnly.indexOf(action.id) >= 0) {
        return polkit.Result.YES;
    }
    if (mutating.indexOf(action.id) >= 0) {
        return polkit.Result.AUTH_SELF_KEEP;
    }
});
```

`subject.local` and `subject.active` restrict the grant to a seat-local, active
session: a remote SSH session or a switched-away session gets the default
`auth_admin` instead. Polkit picks up rule changes without a restart.

### 3.3 Checkpoint C — USBGuard IPC Access Control

The daemon applies its own access control to its IPC socket, independently of
D-Bus and Polkit. It is configured either by the recommended
`IPCAccessControlFiles` directory or by the legacy `IPCAllowedUsers` /
`IPCAllowedGroups` settings, and it is expressed in sections and privileges:

| Section | Privileges | Governs |
|---|---|---|
| `Devices` | `modify` | changing device authorization, including the permanent form |
| | `list` | `listDevices` |
| | `listen` | `DevicePresenceChanged`, `DevicePolicyChanged` |
| `Policy` | `modify` | `appendRule`, `removeRule` |
| | `list` | `listRules` |
| `Exceptions` | `listen` | `ExceptionMessage` |
| `Parameters` | `modify` | `setParameter` |
| | `list` | `getParameter` |
| | `listen` | `PropertyParameterChanged` |

**Which identity is evaluated here is the decisive question, and it is not the
desktop user's in the D-Bus path.** The bridge is a distinct process running as
root; it is the bridge, not the GUI, that holds the IPC connection to the
daemon. The default `IPCAllowedUsers=root` therefore already satisfies this
checkpoint for every D-Bus client, and per-user IPC grants are what the
`usbguard` **CLI** needs, since the CLI connects to the socket directly as the
invoking user.

**[P0]** This is reasoning from the process topology, and it is confirmed by
test P0-5 (§13.3) before the interface tells any user what to do about it:
configure Polkit but *not* the IPC ACL, and observe whether calls succeed. Until
that test passes, the diagnostic panel presents the IPC ACL remedy as a
secondary possibility rather than as the primary instruction.

Where a per-user grant genuinely is required — a hardened deployment that has
narrowed even root's privileges through `IPCAccessControlFiles`, or a user who
also wants the CLI — this is the command, run as root, followed by a daemon
restart:

```bash
usbguard add-user "$USER" --devices=modify,list,listen \
                          --policy=list,modify \
                          --exceptions=listen \
                          --parameters=list
```

Granting `--policy=list` without `modify` is a legitimate, deliberate
configuration: it yields a working read-only interface. §3.5 describes how the
program represents it.

### 3.4 Polkit Agent

Every `auth_*` outcome requires a Polkit authentication agent running in the
session. GNOME, KDE, and most full desktops start one; minimal window managers
frequently do not. Without an agent the call fails with no prompt and no useful
error text. This is diagnosable — the session bus has no owner for
`org.freedesktop.PolicyKit1.Authority`'s agent registration — and is reported as
its own state rather than as a generic denial (test T12, §13.1).

### 3.5 Derived Capability Model

The program does not ask "am I allowed?" as a yes/no question. It maintains a
capability set, and the interface is a function of it. Controls whose capability
is absent are **insensitive with an explanatory tooltip**, never present and
failing on click.

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub list_devices: bool,
    pub modify_devices: bool,
    pub list_rules: bool,
    pub modify_rules: bool,
    pub get_parameters: bool,
    pub set_parameters: bool,
}
```

Capabilities are established by observation, not by prediction:

- `get_parameters` is set by the probe (§4.2) — the one capability known before
  any user action.
- The `list_*` capabilities are set on the first successful call, cleared on a
  denial.
- The `modify_*` capabilities start **optimistically true** and are cleared on
  the first denial, which is then reported. They are never probed, because the
  only probe for a write is a write (§4.2, probe 5).

A denial that clears a capability also records the checkpoint that produced it,
so the panel offers the correct remedy rather than a list of all three.

### 3.6 Packaging Decision: the Polkit Rule Is Not Installed

Neither the `.deb`, the `.rpm`, nor the Flatpak installs the rule of §3.2.

Installing it would silently open USB policy control to an entire
administrative group — a change to the system's security posture, made by a
package installation, on behalf of an administrator who did not ask for it. The
packages instead ship it as

```text
/usr/share/doc/usbguard-gui/70-usbguard-gui.rules.example
```

and the diagnostic panel displays its content with a copy-to-clipboard button
and the exact `install` command, so a user who wants it performs one conscious,
visible action. This is a deliberate trade of first-run convenience for
consent, and it is not revisited without a reason that outweighs it.

---

## 4. Diagnostics

### 4.1 Why This Is a Component and Not a Status Dot

"Service unavailable" is not actionable, because there are at least seven
distinct reasons the program can fail to reach the daemon and the remedy
differs for each — install a package, start a unit, install a Polkit rule, edit
an IPC ACL, start a session agent. A single red indicator forces the user to
rediscover the taxonomy themselves. The diagnostic module is therefore a
first-class component with its own state type, its own probe sequence, and its
own panel in the interface.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessState {
    /// Reachable. `caps` says how much.
    Ok { caps: Capabilities },
    /// No system bus in this session — misconfigured container, usually.
    BusUnavailable,
    /// The bridge package is not installed: the name is neither owned
    /// nor activatable.
    BridgeNotInstalled,
    /// The name is activatable but unowned, and activation failed:
    /// usbguard-dbus.service is not running.
    BridgeNotRunning { daemon_running: Option<bool> },
    /// Rejected by the bus policy, before Polkit.
    DeniedByBusPolicy,
    /// Rejected by Polkit.
    DeniedByPolkit { action: Option<String> },
    /// Rejected by the daemon's IPC access control.
    DeniedByIpcAcl,
    /// Polkit would prompt, but no agent is running in this session.
    NoPolkitAgent,
    /// Denied, and the origin could not be attributed. Both remedies shown.
    DeniedUnattributed { detail: String },
}
```

### 4.2 Probe Sequence

Executed in order, stopping at the first failure. It runs **after** the main
window is presented, never during startup, so that any Polkit prompt it causes
is attributable to a window the user can see.

| # | Question | Method | Outcome |
|---|---|---|---|
| 1 | Is there a system bus? | `zbus::Connection::system()` | `BusUnavailable` |
| 2a | Does anything own `org.usbguard1`? | `org.freedesktop.DBus.NameHasOwner` | if yes, continue at 3 |
| 2b | Is the name at least *activatable*? | `org.freedesktop.DBus.ListActivatableNames` | absent ⇒ `BridgeNotInstalled`; present ⇒ `BridgeNotRunning` |
| 3 | Is a Polkit agent present? | session-bus check, §3.4 | recorded, not fatal — used to explain a later silent denial |
| 4 | Can we read anything? | `getParameter("ImplicitPolicyTarget")` | success ⇒ `Ok`; error ⇒ 5 |
| 5 | Which checkpoint denied it? | classification, §4.3 | one of the `Denied*` states |
| 6 | Can we write? | **not probed** | assumed until a real operation says otherwise (§3.5) |

Probe 2b is what makes `BridgeNotInstalled` detectable without touching the
filesystem: the D-Bus service file that makes a name activatable is installed by
the bridge package and by nothing else, so "not activatable and not owned"
identifies a missing package precisely.

Probe 6 is deliberately empty. The only honest test of write access is a write,
and a test write would modify the system's USB policy in order to find out
whether it is allowed to. The first real operation the user requests carries the
test, and its failure is reported in that operation's own context, where the
user has the surrounding intent to make sense of it.

### 4.3 Attributing a Denial

```rust
fn classify(err: &zbus::Error, agent_present: bool) -> AccessState {
    let zbus::Error::MethodError(name, detail, _) = err else {
        return AccessState::DeniedUnattributed { detail: err.to_string() };
    };
    let text = detail.as_deref().unwrap_or_default().to_ascii_lowercase();

    match name.as_str() {
        "org.freedesktop.DBus.Error.AccessDenied"
        | "org.freedesktop.DBus.Error.AuthFailed" => {
            if text.contains("not authorized") && !agent_present {
                AccessState::NoPolkitAgent
            } else if text.contains("ipc") || text.contains("access control") {
                AccessState::DeniedByIpcAcl
            } else if text.contains("rejected send message") {
                // Bus-level rejection: the message never reached the bridge.
                AccessState::DeniedByBusPolicy
            } else if text.contains("not authorized") || text.contains("polkit") {
                AccessState::DeniedByPolkit { action: None }
            } else {
                AccessState::DeniedUnattributed { detail: text }
            }
        }
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
            AccessState::BridgeNotRunning { daemon_running: None }
        }
        _ => AccessState::DeniedUnattributed { detail: text },
    }
}
```

Matching on error text is fragile by construction. It is treated as a
**user-experience optimization, never as a correctness requirement**: the
`DeniedUnattributed` arm is a complete, correct outcome that presents every
applicable remedy instead of guessing one, and every heuristic branch is
recalibrated against each supported USBGuard version. No control flow other
than which help text is shown depends on the attribution.

---

## 5. Process and Concurrency Architecture

### 5.1 Structure

```text
┌────────────────────────────────────────────────────────────────┐
│ Main thread — GLib MainContext (owns every GTK object)         │
│                                                                │
│   GTK4 / libadwaita widgets                                    │
│   Device model, rule model, capability state, diagnostic state │
│                                        ▲                       │
│                     glib::spawn_future_local                   │
└────────────────────────────────────────┼───────────────────────┘
                                         │
                          async_channel::bounded::<UiEvent>(64)
                                         │
┌────────────────────────────────────────┼───────────────────────┐
│ Tokio runtime (multi-thread, 2 workers, OnceLock)              │
│                                                                │
│   Supervisor ──▶ event worker (zbus)                           │
│      │              ├─ merged signal streams + coalescing      │
│      │              └─ initial snapshot                        │
│      └─ backoff, NameOwnerChanged wait                         │
│                                                                │
│   Command tasks (one per user action, individually abortable)  │
└────────────────────────────────────────┼───────────────────────┘
                                         │
                                  system D-Bus
```

Two flows, deliberately separate:

- **Events** (daemon → interface): one long-lived worker owning the signal
  streams, supervised and restarted.
- **Commands** (interface → daemon): one short-lived task per user action, with
  its own `JoinHandle` so it can be cancelled individually, reporting its
  outcome back over the same channel.

They share the zbus `Connection` — which is internally shared and cheap to
clone — but never share a task. A command that blocks waiting for a Polkit
prompt cannot stall event delivery, and a burst of events cannot delay a
command.

### 5.2 Runtime Bootstrap

GTK owns the main thread and its loop. A `tokio::spawn` issued from that thread
panics, because there is no runtime in its thread-local context. The runtime is
therefore created explicitly and reached through its handle.

```rust
// src/runtime.rs
use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The shared I/O runtime. Two workers suffice: the entire workload is
/// I/O-bound on a single D-Bus connection.
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all() // I/O reactor and timer are both required
            .thread_name("usbguard-io")
            .build()
            .expect("failed to build the Tokio runtime")
    })
}
```

**Two invariants, enforced mechanically rather than by discipline:**

1. `tokio::spawn` never appears in this source tree. Only `runtime().spawn(…)`.
2. No future that touches Tokio's reactor or timers is ever passed to
   `glib::spawn_future_local`, and no future that touches a GTK object is ever
   passed to `runtime().spawn`.

The first is a lint:

```toml
# clippy.toml
disallowed-methods = [
    { path = "tokio::spawn", reason = "use runtime::runtime().spawn() — GTK owns the main thread" },
]
```

The second is enforced by the type system as long as the only channel crossing
the boundary is runtime-agnostic (§5.3): GTK objects are `!Send`, so a future
holding one cannot be passed to `runtime().spawn` at all.

### 5.3 The Bridge Between the Two Loops

`glib::Sender` and `glib::MainContext::channel` were deprecated and removed.
The idiomatic replacement is `async_channel`, whose futures depend on neither
executor and therefore work identically under both. A single channel crosses the
boundary; there is no forwarding hop.

```rust
// src/app.rs (excerpt)
fn build_ui(app: &adw::Application) {
    let window = MainWindow::new(app);

    let (ui_tx, ui_rx) = async_channel::bounded::<UiEvent>(64);

    // Consumer: GTK side, main thread. Every widget touch happens here.
    glib::spawn_future_local({
        let window = window.clone();
        async move {
            while let Ok(event) = ui_rx.recv().await {
                window.apply(event);
            }
        }
    });

    // Producer: Tokio side.
    dbus::supervisor::start(ui_tx.clone());

    window.present();
    // The probe runs only once the window is on screen (§4.2).
    diagnostics::probe_after_present(ui_tx);
}
```

The channel is bounded at 64. Incoming events are already coalesced (§5.4), so
the worker emits at most a handful of messages per second even under a burst,
and saturation is not reachable in practice. If it were, backpressure would
apply to the coalescer — which continues to drain and merge the signal stream
while blocked on the send — and never to the shared connection used for
commands.

### 5.4 Event Worker and Coalescing

Under a burst — a hub with dozens of devices — the naive approach of delaying
each event by a fixed interval is pathological: latency grows linearly with the
number of queued events while the signal stream goes undrained, and backpressure
reaches the shared connection. The protection becomes the stall.

Correct coalescing **merges rather than delays**: the stream is drained
continuously, events accumulate in a map keyed by device id where a later state
supersedes an earlier one, and one aggregated update is emitted per window.

```rust
// src/dbus/worker.rs

/// Emit once no event has arrived for this long.
const QUIET_WINDOW: Duration = Duration::from_millis(80);
/// Emit regardless after this long, even under a continuous burst.
const MAX_LATENCY: Duration = Duration::from_millis(250);
/// Beyond this many pending devices, stop tracking deltas and resynchronize.
const RESYNC_THRESHOLD: usize = 40;

enum RawEvent {
    Presence { id: u32, event: u32, target: u32, rule: String, attrs: HashMap<String, String> },
    Policy   { id: u32, target_new: u32, rule_id: u32, rule: String, attrs: HashMap<String, String> },
    Exception { context: String, object: String, reason: String },
}

async fn event_loop(
    devices: &UsbGuardDevicesProxy<'_>,
    root: &UsbGuardProxy<'_>,
    ui_tx: &async_channel::Sender<UiEvent>,
) -> Result<(), WorkerError> {
    // Subscribe BEFORE the initial snapshot: anything emitted during
    // listDevices is then already buffered in the stream, not lost.
    let presence = devices.receive_device_presence_changed().await?;
    let policy   = devices.receive_device_policy_changed().await?;
    let except   = root.receive_exception_message().await?;

    let mut events = stream::select(
        stream::select(presence.map(into_raw_presence), policy.map(into_raw_policy)),
        except.map(into_raw_exception),
    );

    let snapshot = devices.list_devices("match").await?;
    ui_tx.send(UiEvent::DeviceSnapshot(parse_snapshot(snapshot))).await?;

    let mut pending: HashMap<DeviceId, DeviceDelta> = HashMap::new();
    let mut window_start: Option<Instant> = None;

    loop {
        let next = if pending.is_empty() {
            events.next().await
        } else {
            let elapsed = window_start.map(|t| t.elapsed()).unwrap_or_default();
            let budget = QUIET_WINDOW.min(MAX_LATENCY.saturating_sub(elapsed));
            match tokio::time::timeout(budget, events.next()).await {
                Ok(item) => item,
                Err(_) => { flush(&mut pending, &mut window_start, ui_tx).await?; continue; }
            }
        };

        let Some(raw) = next else { break }; // stream closed: clean shutdown

        match raw {
            RawEvent::Exception { context, object, reason } => {
                // Out of band: never coalesced, never dropped.
                ui_tx.send(UiEvent::DaemonException { context, object, reason }).await?;
                ui_tx.send(UiEvent::InvalidateRuleCache).await?;
            }
            other => {
                window_start.get_or_insert_with(Instant::now);
                merge_into(&mut pending, other);

                if pending.len() >= RESYNC_THRESHOLD {
                    pending.clear();
                    window_start = None;
                    let snap = devices.list_devices("match").await?;
                    ui_tx.send(UiEvent::DeviceSnapshot(parse_snapshot(snap))).await?;
                }
            }
        }
    }

    if !pending.is_empty() {
        flush(&mut pending, &mut window_start, ui_tx).await?;
    }
    Ok(())
}
```

**`merge_into` precedence**, in order:

1. A `Remove` supersedes any pending delta for that device id.
2. A `Policy` event's `target_new` supersedes a `Presence` event's `target` for
   the same id, since the policy decision is the later and final word (§2.4.5).
3. Attribute maps accumulate, with later keys winning.

A device inserted and removed within one window therefore never appears in the
table at all, which is the correct outcome rather than a flicker.

**The resynchronization threshold is the real burst defence.** Above forty
devices in flight, one `listDevices` costs less than forty incremental updates
and realigns with certainty rather than by induction over deltas. Its cost is
constant in the size of the burst, which is what makes the latency ceiling
meaningful.

**The latency ceiling makes the responsiveness target falsifiable**: worst-case
delay between a D-Bus signal and a model update is `MAX_LATENCY` plus render
time regardless of volume, and `QUIET_WINDOW` for an isolated event.

### 5.5 Supervision and Reconnection

A worker that exits on its first error leaves the interface silently frozen
showing stale state — the worst failure mode for a security tool, since the
displayed authorization no longer describes the system. The worker is wrapped:

```rust
pub fn start(ui_tx: async_channel::Sender<UiEvent>) {
    runtime().spawn(async move {
        let mut backoff = Duration::from_millis(500);
        const BACKOFF_MAX: Duration = Duration::from_secs(30);

        loop {
            match run_once(&ui_tx).await {
                Ok(()) => break, // the interface closed the channel
                Err(err) => {
                    let _ = ui_tx.send(UiEvent::ConnectionLost(err.to_string())).await;
                    if ui_tx.is_closed() { break; }
                }
            }

            // Wait for the service to come back rather than polling blindly.
            match wait_for_service(backoff).await {
                ServiceWait::Appeared => backoff = Duration::from_millis(500),
                ServiceWait::TimedOut => backoff = (backoff * 2).min(BACKOFF_MAX),
            }
        }
    });
}
```

`wait_for_service` subscribes to `NameOwnerChanged` on `org.freedesktop.DBus`,
filtered to `org.usbguard1`, capped at the current backoff. A service that comes
back is picked up almost immediately; a service that is absent for an hour
generates no traffic at all.

On every reconnection **both caches are discarded unconditionally**. The
ruleset may have changed while the program was disconnected, rule ids may have
been reassigned, and device ids may refer to different devices. Nothing observed
before the disconnection is carried across it.

### 5.6 Cache Invalidation

There is no ruleset-changed signal (§2.4.10), so invalidation is inferred. The
rule cache is marked stale on **every** one of:

| Trigger | Reason |
|---|---|
| a successful local `appendRule` / `removeRule` | own mutation |
| `DevicePolicyChanged` with a non-zero `rule_id` | another client made a permanent change |
| any `ExceptionMessage` | daemon-side state changed in a way not otherwise reported |
| reconnection to the bus | §5.5 |
| the policy view becoming visible | opportunistic; covers CLI edits that produced no signal |

The device cache is invalidated by the reconnection and resynchronization paths
only; device state is fully described by the signals.

### 5.7 Cancellable Operations

zbus does not impose a client-side reply deadline, so a call awaiting a Polkit
prompt waits as long as the human takes. The failure mode to guard against is
therefore not premature timeout but an interface waiting mutely and forever.

No arbitrary deadline is imposed on an operation whose duration is a person
typing a password. Instead every mutating operation is a task with a handle,
and the row that spawned it exposes its state:

```rust
pub struct PendingOperation {
    pub label: String,
    pub started: Instant,
    handle: tokio::task::JoinHandle<()>,
}

impl PendingOperation {
    /// Bound to the Cancel button in the pending row.
    pub fn cancel(self) { self.handle.abort(); }
}
```

| Elapsed | Interface |
|---|---|
| 0 s | spinner in the row, Cancel button |
| 20 s | "Waiting for authentication" |
| 180 s | warning suggesting no Polkit agent is running (§3.4) — **the operation is not cancelled** |

The operation is never auto-cancelled, because a slow agent and a user walking
to fetch a password are indistinguishable from here, and cancelling either one
is wrong.

**Cancellation is client-side only.** Aborting the task stops waiting for the
reply; it does not withdraw a request the daemon may already have executed. The
interface states this in as many words, and a cancellation is always followed by
a re-read, so what is displayed afterwards is the observed state and not the
presumed one.

---

## 6. Domain Model

### 6.1 Types Crossing the Boundary

```rust
/// One device as the interface knows it.
pub struct Device {
    pub id: DeviceId,
    pub target: Target,
    /// Canonical rule text as returned by listDevices.
    pub rule_text: String,
    pub attrs: DeviceAttributes,
    /// Set when a mutating operation on this device is in flight.
    pub pending: Option<PendingKind>,
}

/// Every field optional: a missing attribute is normal, not an error (§6.2).
#[derive(Default)]
pub struct DeviceAttributes {
    pub name: Option<String>,
    pub usb_id: Option<UsbId>,       // vendor:product
    pub serial: Option<String>,
    pub via_port: Option<String>,
    pub hash: Option<String>,
    pub parent_hash: Option<String>,
    pub with_interface: Vec<InterfaceType>,
    pub with_connect_type: Option<String>,
}

/// The aggregate produced by one coalescing window for one device.
pub struct DeviceDelta {
    pub id: DeviceId,
    pub removed: bool,
    pub target: Option<Target>,
    pub rule_text: Option<String>,
    pub attrs: DeviceAttributes,
}

/// The only message type crossing the executor boundary.
pub enum UiEvent {
    DeviceSnapshot(Vec<Device>),
    DeviceBatch(Vec<DeviceDelta>),
    RuleSnapshot(Vec<RuleHandle>),
    InvalidateRuleCache,
    ParameterChanged { name: String, value: String },
    DaemonException { context: String, object: String, reason: String },
    OperationFinished { op: OperationId, result: Result<OperationOutcome, AppError> },
    AccessStateChanged(AccessState),
    ConnectionLost(String),
    Reconnected,
}
```

`UiEvent` is the entire contract between the two halves of the program. It is
`Send`, contains no GTK type and no zbus type, and is defined in a module that
depends on neither — which is what keeps the invariants of §5.2 checkable by the
compiler rather than by review.

### 6.2 Attribute Optionality

The `a{ss}` dictionary carried by the signals contains, when the device
provides them: `id`, `name`, `serial`, `hash`, `parent-hash`, `via-port`,
`with-interface`. Every key is optional **individually**. A device with no
serial number is ordinary hardware, not a malformed event. Absence renders as an
explicit "—" in the corresponding cell; it never suppresses the row and never
produces an error.

### 6.3 Error Taxonomy

One error type crosses layers, and it distinguishes what the user can act on:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("access denied: {0:?}")]
    Denied(AccessState),
    #[error("the daemon is unreachable: {0}")]
    Unreachable(String),
    #[error("the daemon rejected the request: {0}")]
    Rejected(String),
    #[error("could not parse the rule language: {0}")]
    Parse(#[from] ParseError),
    #[error("the operation was cancelled")]
    Cancelled,
    #[error("the ruleset changed underneath this operation")]
    Stale,
}
```

`Denied` reopens the diagnostic panel at the relevant remedy. `Stale` triggers a
re-read and a re-presentation of the choice rather than a retry, because a retry
would act on an assumption the program has just been told is false.

### 6.4 Rule Identity and Safe Removal

Re-reading the ruleset immediately before `removeRule` in order to validate the
id does not make removal safe. It narrows the race window without closing it,
and what remains is a time-of-check-to-time-of-use race on a destructive
operation against system security policy: between the check and the call, the
ruleset can change, and the id can now name a different rule.

The identity is therefore the rule's **canonical text**, which is stable, and
the id is treated as a volatile reference resolved at the moment of use.

```rust
#[derive(Debug, Clone)]
pub struct RuleHandle {
    /// Volatile: valid only while the ruleset is unchanged.
    pub id: RuleId,
    /// Stable identity: the canonical text as returned by listRules.
    pub text: String,
    /// Evaluation-order position at the time of reading, for disambiguation.
    pub position: usize,
}

pub enum RemoveOutcome {
    Removed,
    /// No rule with this text exists any more: someone else removed it.
    AlreadyGone,
    /// Several rules share this text: the user must choose.
    Ambiguous { candidates: Vec<RuleHandle> },
}

async fn remove_rule(
    policy: &UsbGuardPolicyProxy<'_>,
    handle: &RuleHandle,
) -> Result<RemoveOutcome, AppError> {
    let current = policy.list_rules("match").await?;
    let matches: Vec<RuleHandle> = current
        .iter()
        .enumerate()
        .filter(|(_, (_, text))| *text == handle.text)
        .map(|(position, (id, text))| RuleHandle { id: RuleId(*id), text: text.clone(), position })
        .collect();

    match matches.as_slice() {
        []     => Ok(RemoveOutcome::AlreadyGone),
        [only] => { policy.remove_rule(only.id.0).await?; Ok(RemoveOutcome::Removed) }
        many   => Ok(RemoveOutcome::Ambiguous { candidates: many.to_vec() }),
    }
}
```

A race window between `list_rules` and `remove_rule` remains, and it is
irreducible with this API: the daemon offers no conditional removal. What this
design changes is the **direction of failure**. If the rule is already gone that
is detected and reported, instead of another rule that inherited the id being
deleted. If several rules share the text, the user is asked — with each
candidate's evaluation-order position, which `listRules` provides and which is
what makes them distinguishable — instead of one being picked arbitrarily.

Textually duplicated rules are unusual but legitimate, which is why `Ambiguous`
is a real branch and not an assertion.

Eliminating this race properly requires an upstream addition — a
`removeRuleIfMatches(id, expected_text)` — and that is the change with the best
effort-to-benefit ratio to propose to the USBGuard maintainers (§15).

---

## 7. Rule Language Parser and Generator

A first-class component, needed for two independent reasons.

**On read**, `listDevices` and `listRules` return rule-language text (§2.4.3).
Without a parser there are no Name, Vendor, VID:PID, Serial, or Port columns —
only an opaque string per row.

**On write**, local validation before sending means a syntactically invalid rule
is rejected in the dialog, immediately and with a caret at the offending token,
rather than after a Polkit prompt and a round trip to the daemon.

### 7.1 Surface

```rust
/// Policy text: targets allow | block | reject only.
pub fn parse_rule(input: &str) -> Result<Rule, ParseError>;
/// A rule with no target, as produced by some CLI subcommands.
pub fn parse_partial(input: &str) -> Result<PartialRule, ParseError>;
/// Query text: additionally accepts `match` (§2.6).
pub fn parse_query(input: &str) -> Result<Query, ParseError>;

/// Renders a rule to canonical text with correct quoting and escaping.
impl std::fmt::Display for Rule { /* … */ }

pub struct ParseError {
    pub kind: ParseErrorKind,
    /// Byte offset into the input, for caret placement in the dialog.
    pub offset: usize,
}
```

The grammar to accept is §2.6 in full: three targets; the nine attributes; all
six set operators including `match-all`; the `if` clause with negation and
grouping; every condition form; partial rules; and `#` comments.

Quoting and escaping are not an afterthought. Device names and serial numbers
routinely contain quotes and backslashes, and occasionally bytes that are not
valid UTF-8, because they come from a descriptor the device itself supplies.

### 7.2 Robustness Requirements

These are non-negotiable, because the input originates from hardware an
attacker may control.

- **No `panic!`, `unwrap()`, `expect()`, or slice indexing on the parsing
  path.** A malformed descriptor must not be able to terminate the program. The
  module carries `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]`.
- **An unparseable rule still produces a row.** Known fields are filled in, the
  rest are marked unavailable, and the raw text is preserved and shown. Hiding a
  device because its descriptor could not be understood is the single worst
  failure available to a security tool: the user concludes nothing is connected.
- **Explicit bounds** on input length and on nesting depth of condition sets,
  checked before recursion rather than discovered by stack exhaustion.
- **Non-UTF-8 input is handled, not rejected.** The parser operates on bytes;
  text destined for display is converted lossily at the presentation boundary,
  never at the parsing boundary.
- **Round-trip verification on generation.** Every rule the program generates is
  re-parsed and compared before it is sent. A generator that produces text the
  parser rejects is a bug caught locally instead of by the daemon.
- **A fuzzing target** (`cargo-fuzz`) over `parse_rule` is part of the module's
  definition of done, together with a corpus of real policies collected from the
  reference distributions during Phase 0.

---

## 8. D-Bus Proxy Layer

`zbus`'s `#[proxy]` macro generates the client side from the interface
declaration. These declarations are the single place where raw upstream types
appear; everything above them uses the domain types of §6.

```rust
// src/dbus/proxies.rs
use std::collections::HashMap;
use zbus::proxy;

/// Root interface: runtime parameters and asynchronous daemon exceptions.
#[proxy(
    interface = "org.usbguard1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1"
)]
pub trait UsbGuard {
    /// name: "ImplicitPolicyTarget" | "InsertedDevicePolicy"
    fn get_parameter(&self, name: &str) -> zbus::Result<String>;
    /// Returns the PREVIOUS value.
    fn set_parameter(&self, name: &str, value: &str) -> zbus::Result<String>;

    #[zbus(signal)]
    fn property_parameter_changed(
        &self, name: &str, value_old: &str, value_new: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn exception_message(
        &self, context: &str, object: &str, reason: &str,
    ) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.usbguard.Devices1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1/Devices"
)]
pub trait UsbGuardDevices {
    /// `query` is rule-language syntax: "match" for every device,
    /// "allow" for the allowed ones only. Result order is unspecified.
    fn list_devices(&self, query: &str) -> zbus::Result<Vec<(u32, String)>>;

    /// The returned rule id is meaningful only when `permanent` is true.
    fn apply_device_policy(
        &self, id: u32, target: u32, permanent: bool,
    ) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn device_presence_changed(
        &self, id: u32, event: u32, target: u32,
        device_rule: &str, attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn device_policy_changed(
        &self, id: u32, target_old: u32, target_new: u32,
        device_rule: &str, rule_id: u32, attributes: HashMap<String, String>,
    ) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.usbguard.Policy1",
    default_service = "org.usbguard1",
    default_path = "/org/usbguard1/Policy"
)]
pub trait UsbGuardPolicy {
    /// Returns (rule_id, rule_text) in evaluation order.
    /// Bound by position: the formal parameter name has changed upstream.
    fn list_rules(&self, query: &str) -> zbus::Result<Vec<(u32, String)>>;

    /// Third parameter is `temporary`, NOT `permanent` (§2.4.7).
    /// Use RULE_PARENT_APPEND_LAST to append at the end (§2.4.6).
    fn append_rule(
        &self, rule: &str, parent_id: u32, temporary: bool,
    ) -> zbus::Result<u32>;

    fn remove_rule(&self, id: u32) -> zbus::Result<()>;
}
```

A thin wrapper sits above these and is what the rest of the program calls. It
converts `u32` to `DeviceId` / `RuleId`, `Target` and `Persistence` to and from
their wire encodings, `zbus::Error` to `AppError`, and it is the only module
allowed to name a raw `u32` id.

---

## 9. User Interface

### 9.1 Window Shell

An `AdwApplicationWindow` with an `AdwViewStack` over two pages, Devices and
Policy, and an `AdwToastOverlay` for transient outcomes.

The header bar carries a **three-state connection indicator** — connected,
connected read-only, disconnected — which is a button opening the diagnostic
panel of §4. The read-only state is not a degraded rendering of "connected": it
is a distinct, legitimate, and common configuration (§3.3), and it is labelled
as such.

Below the header, an `AdwBanner` appears for conditions that persist and need an
action: the bridge is not installed, the connection was lost and is being
retried, write access was denied.

The diagnostic panel is a dialog, not a page: it lists the checkpoints of §3
with the outcome of each, the specific remedy for the first failing one, and the
exact command or file content to apply it, with a copy button.

### 9.2 Device View

A column view over the device model.

| Column | Source | Notes |
|---|---|---|
| State | `Target` | Allowed / Blocked / Rejected / Unknown, with an icon and an accessible label — never colour alone |
| Name | parsed `name` attribute | "—" when absent |
| ID | parsed `id` attribute | `vendor:product` |
| Serial | parsed `serial` | "—" when absent |
| Port | parsed `via-port` | |

The daemon's device id is available — in the row's detail expander and in the
tooltip — but is not a primary column. Placed next to `vendor:product` it reads
as a second identifier of the same kind, which it is not (§2.4.1).

Default sort is by port, which groups devices by physical topology and is
stable; `listDevices` order is not (§2.4.9). Sorting is a client concern
throughout.

Per-row actions: **Allow**, **Block**, **Reject**, each opening a choice between
*this session only* and *permanently*, phrased in those terms rather than as
"temporary/permanent", and mapping to `Persistence` (§2.4.7). A row with an
operation in flight shows a spinner and a Cancel button in place of its actions
(§5.7). Reject is confirmed through an `AdwAlertDialog`, because it removes the
device from the system and requires physical re-insertion to undo.

Filter entry over name, id, and serial. A "show blocked only" toggle.

### 9.3 Policy View

The ruleset in evaluation order. **The order is semantically load-bearing** —
the daemon applies the first matching rule — so it is preserved visually,
numbered, and never re-sorted by a column header. That is the difference between
this list and the device list, and it is stated in the view's own description
text.

Each row shows the rule's canonical text in a monospace face, its position, and
its `label` attribute when present. Removal follows §6.4 in full, including the
`Ambiguous` dialog, which lists candidates by position.

### 9.4 New Rule Dialog

Guided composition: target, then attributes added one at a time from the closed
list of §2.6, each with a value entry validated for its own shape (hex pairs for
`id`, `cc:ss:pp` for `with-interface`, and so on).

Three properties this dialog must have:

1. **A live preview** of the canonical text that will be sent. The user sees the
   rule, not a form that stands for one.
2. **Local validation before sending** (§7). An invalid rule never reaches the
   daemon and never causes a Polkit prompt.
3. **Explicit position**: at the end of the ruleset via
   `RULE_PARENT_APPEND_LAST`, or after a rule the user selects — expressed as
   "after «…»", since the underlying API takes a parent id and not an index
   (§2.4.6).

Plus the persistence choice, hidden entirely where the daemon's `appendRule`
does not accept it (§2.4.6, **[P0]**).

An escape hatch: a raw-text mode for users who would rather type the rule, with
the same validation applied.

### 9.5 Notifications

Desktop notifications for a newly inserted device that is not authorized, via
`notify-rust`, with Allow / Block quick actions.

Interactive actions require the `actions` capability of the notification
server, which is **queried with `get_capabilities` before sending**. Where it is
absent, the notification is informational and activating it raises the window
with the device selected. A notification carrying buttons that the server
silently drops is worse than one that never claimed to have them.

Under Flatpak, `org.freedesktop.portal.Notification` is preferred when
available, falling back to the session bus.

Notifications are rate-limited and coalesced on the same window as §5.4: a hub
insertion produces one summary notification, not thirty.

### 9.6 System Tray and Background Mode

`ksni` implements `StatusNotifierItem`. It performs **no fallback of any kind**:
where no watcher exists, registration simply fails.

Detection is therefore explicit — query the session bus for an owner of
`org.kde.StatusNotifierWatcher` before attempting registration. Where there is
none, which is GNOME without an AppIndicator extension, the program enters
background mode without an icon, keeps notifications working, and states this
once in its preferences. It does not log an error per attempt.

`ksni` brings a second D-Bus stack alongside zbus. The duplication is acceptable
but is a real cost in build time and binary size, and if it grows, implementing
`StatusNotifierItem` directly on zbus is the alternative.

### 9.7 Accessibility and Presentation

- Authorization state is conveyed by icon *and* text, never by colour alone.
- Every icon-only button has an accessible label and a tooltip.
- The device and policy views are keyboard navigable end to end; row actions are
  reachable without a pointer.
- Rule text is rendered in a monospace face and is selectable.
- All user-visible strings pass through `gettext`, with translator comments on
  anything ambiguous out of context. Rule-language keywords are **not**
  translated: they are syntax.

---

## 10. Configuration, Logging, and Command Line

### 10.1 Persisted Settings

Stored through GSettings under `io.github.onyks_os.UsbguardGui`. The full set:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `start-in-background` | boolean | `false` | Start without presenting the window |
| `notify-inserted` | boolean | `true` | Notify on unauthorized insertion |
| `default-persistence` | enum | `runtime-only` | Preselection in the device action popover |
| `window-width`, `window-height` | integer | | Geometry |
| `window-maximized` | boolean | `false` | |
| `show-blocked-only` | boolean | `false` | Device filter |
| `tray-notice-shown` | boolean | `false` | Whether §9.6's one-time notice was given |

`default-persistence` defaults to the non-persisting option deliberately: the
less destructive default for a security tool is the one whose effect disappears
on restart.

No configuration of this program has any effect on the daemon. Nothing here is
security policy.

### 10.2 Logging

`tracing` with an `EnvFilter`, default level `warn`, controlled by
`USBGUARD_GUI_LOG`. Spans wrap each D-Bus call and each coalescing window,
which is also how the latency metrics of §13.2 are measured.

**Device names, serial numbers, and hashes are never logged above `debug`**, and
the `debug` level states in its own documentation that it records device
identifiers. A default-level log file from a bug report must not be a list of
the hardware the user owns.

### 10.3 Command Line

| Flag | Effect |
|---|---|
| `--background` | Start without presenting the window (overrides the setting) |
| `--diagnose` | Run the probe sequence of §4.2, print the result to stdout, exit |
| `--version` | Version, plus the USBGuard version detected on the bus |

`--diagnose` exists so a user reporting a problem can paste one command's output
instead of describing a dialog, and so the checks are exercisable in CI without
a display.

Single-instance behaviour comes from `adw::Application` with
`ApplicationFlags::HANDLES_COMMAND_LINE`; a second launch raises the running
window.

---

## 11. Source Tree

```text
usbguard-gui/
├── Cargo.toml
├── Cargo.lock                  # committed: this ships as a binary
├── clippy.toml                 # the disallowed-methods lint of §5.2
├── build.rs                    # compiles GResource, schema, translations
├── src/
│   ├── main.rs                 # argument handling, application setup
│   ├── app.rs                  # AdwApplication, the channel wiring of §5.3
│   ├── runtime.rs              # §5.2
│   ├── config.rs               # GSettings wrapper
│   ├── model/
│   │   ├── mod.rs
│   │   ├── ids.rs              # DeviceId, RuleId, UsbId, InterfaceType
│   │   ├── target.rs           # Target, Persistence
│   │   ├── device.rs           # Device, DeviceAttributes, DeviceDelta
│   │   ├── rule.rs             # Rule, PartialRule, RuleHandle
│   │   ├── event.rs            # UiEvent — depends on neither GTK nor zbus
│   │   └── error.rs            # AppError
│   ├── rules/
│   │   ├── mod.rs
│   │   ├── lexer.rs
│   │   ├── parser.rs           # §7 — no unwrap, no panic
│   │   └── render.rs           # canonical text, quoting, round-trip check
│   ├── dbus/
│   │   ├── mod.rs
│   │   ├── proxies.rs          # §8, the only place raw signatures appear
│   │   ├── client.rs           # typed wrapper over the proxies
│   │   ├── worker.rs           # §5.4
│   │   ├── supervisor.rs       # §5.5
│   │   ├── commands.rs         # §5.7
│   │   └── diagnostics.rs      # §4
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── window.rs           # shell, view stack, apply(UiEvent)
│   │   ├── device_view.rs
│   │   ├── policy_view.rs
│   │   ├── rule_dialog.rs
│   │   ├── diagnostic_dialog.rs
│   │   └── preferences.rs
│   ├── notify.rs               # §9.5
│   └── tray.rs                 # §9.6
├── data/
│   ├── io.github.onyks_os.UsbguardGui.desktop.in
│   ├── io.github.onyks_os.UsbguardGui.metainfo.xml.in
│   ├── io.github.onyks_os.UsbguardGui.gschema.xml
│   ├── icons/
│   ├── resources.gresource.xml
│   └── ui/                     # GtkBuilder templates
├── packaging/
│   ├── io.github.onyks_os.UsbguardGui.yaml     # Flatpak manifest
│   └── 70-usbguard-gui.rules.example           # §3.6 — shipped, not installed
├── po/
├── fuzz/                       # §7.2
├── tests/
└── docs/
    ├── architecture.md
    └── dbus-introspection/     # §13.3 output, versioned
```

The dependency direction is strictly one way: `model` depends on nothing in the
program; `rules` depends on `model`; `dbus` depends on `model` and `rules`;
`ui` depends on all three. `model` and `rules` are testable without a bus and
without a display, which is what makes the majority of the test suite runnable
in CI.

---

## 12. Build and Dependencies

Rust 2024 edition. The MSRV is recorded in `Cargo.toml` as `rust-version` and
raised only deliberately.

| Crate | Purpose |
|---|---|
| `gtk4` | GTK 4 bindings; feature-gated to the GTK version of §1.4 |
| `libadwaita` | Adwaita widgets |
| `zbus` | D-Bus client, the `#[proxy]` macro of §8 |
| `tokio` | The I/O runtime of §5.2 — features `rt-multi-thread`, `time`, `macros` |
| `async-channel` | The executor-agnostic bridge of §5.3 |
| `futures-util` | Stream combinators for §5.4 |
| `notify-rust` | §9.5 |
| `ksni` | §9.6 |
| `tracing`, `tracing-subscriber` | §10.2 |
| `thiserror` | §6.3 |
| `gettext-rs` | §9.7 |

Exact versions are resolved with `cargo add` at project initialization and
pinned by the committed `Cargo.lock`; they are not transcribed here, where they
would go stale silently. What this document fixes is the **choice** of crate and
the reason for it.

Build-time system dependencies: `gtk4` and `libadwaita` development packages,
`glib` development files, `pkg-config`, a C toolchain. On Fedora:
`gtk4-devel libadwaita-devel glib2-devel pkgconf-pkg-config gcc`.

`build.rs` compiles the GResource bundle, the GSettings schema, and the
translation catalogues.

**Continuous integration** runs, on every push: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings` (which is what enforces §5.2 and
§7.2), `cargo test`, `cargo llvm-cov` for the coverage target of §13.2, and a
short `cargo fuzz run` over the parser. The D-Bus-dependent tests run against
the mock daemon of §13.1, never against a live system daemon.

---

## 13. Verification

### 13.1 Resilience Test Matrix

Every scenario is a required test. Those marked *mock* run in CI against a
stub implementation of the three interfaces on a private bus; the rest are
manual, on the reference distributions, before a release.

| ID | Scenario | Required outcome |
|---|---|---|
| T1 | `systemctl stop usbguard-dbus` with the window open | Disconnected within 2 s, no crash, automatic reconnection when it returns |
| T2 | `systemctl restart usbguard` during an operation awaiting Polkit | Operation reported inconclusive, state re-read, no optimistic update |
| T3 | Hub with 40+ devices inserted at once *(mock)* | No freeze, resynchronization triggered, latency within the ceiling |
| T4 | Rule removed from the CLI while selected in the window | `AlreadyGone`, comprehensible message, no other rule removed |
| T5 | Ruleset containing two textually identical rules *(mock)* | Disambiguation dialog listing evaluation positions |
| T6 | Device whose name contains quotes, backslashes, non-UTF-8 bytes *(mock)* | Row shown with partial fields, no panic, no corrupted text |
| T7 | Launch under Flatpak on GNOME, KDE, and Sway | Tray where supported, background mode elsewhere, no console errors |
| T8 | `usbguard-dbus` package not installed | `BridgeNotInstalled`, with the install command for the running distribution |
| T9 | Bridge installed, service stopped | `BridgeNotRunning`, distinguished from T8 |
| T10 | Polkit left at `auth_admin` defaults | No prompt before the window is on screen; prompt only on explicit action |
| T11 | IPC ACL granting `Policy=list` without `modify` | Read-only mode; write controls insensitive, not failing |
| T12 | No Polkit agent in the session | `NoPolkitAgent`, with the cause named |
| T13 | Polkit prompt left open for 5 minutes | No timeout, no spurious error, informative notice after 180 s |
| T14 | Bus policy denying the name | `DeniedByBusPolicy`, distinguished from a Polkit denial |
| T15 | Daemon restarted while the window is idle | Both caches discarded, full resynchronization, no stale rows |

The mock daemon is also what makes T3, T5, and T6 reproducible: they are all
either impractical or destructive to stage with real hardware.

A further manual scenario, not automatable: run the daemon with
`DeviceManagerBackend=umockdev` against recorded device descriptors, which is
the supported upstream way to exercise device handling without physical
hardware.

### 13.2 Targets

| Property | Target | How measured |
|---|---|---|
| Isolated event latency | < 150 ms | `tracing` span from signal receipt to model update, median of 100 events |
| Burst latency | ≤ 250 ms + render | Guaranteed by `MAX_LATENCY` (§5.4); verified in T3 |
| Event loss under burst | zero | T3: final displayed state compared against `usbguard list-devices` |
| Polkit resilience | zero timeouts on waits ≤ 10 min | T13 |
| Idle memory | RSS < 110 MB, PSS < 70 MB | `smem`, after 10 minutes in background with 20 devices |
| System filesystem access | zero | `strace` filtered on `/etc`, `/var`, `/sys` |
| Parser robustness | zero panics over 10⁶ fuzz inputs | `cargo-fuzz` |
| Parser coverage | > 90% of lines | `cargo-llvm-cov` |

The memory figure is PSS-based on purpose. GTK4 and libadwaita share a
substantial fraction of their pages with every other GTK application in the
session, so RSS overstates what this program actually costs a running desktop,
and PSS is the honest number to hold ourselves to.

### 13.3 Phase 0: Upstream Verification Procedure

Run before any code depends on §2, and again on every change to the supported
USBGuard version. Output is committed to `docs/dbus-introspection/<distro>-<version>.xml`.

```bash
# 1. Full signatures of the three interfaces.
gdbus introspect --system --dest org.usbguard1 --object-path /org/usbguard1 --xml
gdbus introspect --system --dest org.usbguard1 --object-path /org/usbguard1/Devices --xml
gdbus introspect --system --dest org.usbguard1 --object-path /org/usbguard1/Policy --xml

# 2. Actual call behaviour.
busctl --system call org.usbguard1 /org/usbguard1/Devices \
       org.usbguard.Devices1 listDevices s "match"
busctl --system call org.usbguard1 /org/usbguard1/Policy \
       org.usbguard.Policy1 listRules s "match"

# 3. Real signals during insertion and removal.
gdbus monitor --system --dest org.usbguard1

# 4. The Polkit action identifiers, read rather than assumed.
cat /usr/share/polkit-1/actions/org.usbguard1.policy

# 5. The bus policy, which is checkpoint A.
cat /usr/share/dbus-1/system.d/org.usbguard1.conf
```

**P0-1 — Interface signatures.** Confirm every signature in §2.2, in particular
the arity of `appendRule` (§2.4.6) and the argument order of
`DevicePolicyChanged`.

**P0-2 — Target mapping.** Apply each of `allow`, `block`, and `reject` to a
test device and compare the textual output of `usbguard list-devices` against
the numeric `target` observed on `gdbus monitor` for that same device. This is
what validates §2.3.

**P0-3 — Event mapping.** Insert, modify, and remove a device while monitoring,
and record which numeric `event` accompanies each.

**P0-4 — `DevicePresent` absence.** Confirm that connecting a client while
devices are already present yields no `DevicePresent` signal, and that
`listDevices` is the only source of initial state (§2.4.4).

**P0-5 — Which identity checkpoint C evaluates.** Configure the Polkit rule of
§3.2 for a user who is **not** granted anything by the daemon's IPC access
control, and attempt `listDevices` over D-Bus. Success confirms that the IPC ACL
is satisfied by the bridge's own root identity and that per-user IPC grants are
a CLI concern; failure means §3.3 is wrong and both the diagnostic wording and
the recommended setup change. Nothing the interface tells a user about the IPC
ACL is written before this test has an answer.

**P0-6 — Rule corpus.** Collect the real `rules.conf` shipped or generated on
each reference distribution, plus the output of `usbguard generate-policy`, as
the parser's test corpus and fuzzing seed (§7.2).

---

## 14. Packaging

### 14.1 Flatpak

```yaml
# packaging/io.github.onyks_os.UsbguardGui.yaml
app-id: io.github.onyks_os.UsbguardGui
runtime: org.gnome.Platform
# 48 is the floor implied by §1.4; build against the newest on Flathub.
runtime-version: '48'
sdk: org.gnome.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable
command: usbguard-gui

build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/usbguard-gui/cargo

finish-args:
  - --socket=wayland
  - --socket=fallback-x11
  - --share=ipc
  - --device=dri

  # The only system-bus access required.
  - --system-talk-name=org.usbguard1

  # Notifications: portal first, session bus as fallback.
  - --talk-name=org.freedesktop.portal.Notification
  - --talk-name=org.freedesktop.Notifications

  # Tray: the watcher must be reachable, and the item name must be ownable.
  - --talk-name=org.kde.StatusNotifierWatcher
  # SNI names have the form org.kde.StatusNotifierItem-<pid>-<n>, which is not
  # a dotted subtree, so Flatpak's wildcard does not cover them. Inside the
  # sandbox the process usually receives a low pid; a few variants are declared.
  # Verify per environment (test T7): a missing tray under Flatpak is almost
  # always this.
  - --own-name=org.kde.StatusNotifierItem-2-1
  - --own-name=org.kde.StatusNotifierItem-2-2
  - --own-name=org.kde.StatusNotifierItem-3-1

modules:
  - name: usbguard-gui
    buildsystem: simple
    build-options:
      env:
        CARGO_NET_OFFLINE: 'true'
    build-commands:
      - cargo --offline fetch --manifest-path Cargo.toml
      - cargo --offline build --release
      - install -Dm755 target/release/usbguard-gui /app/bin/usbguard-gui
      - install -Dm644 data/io.github.onyks_os.UsbguardGui.desktop
          /app/share/applications/io.github.onyks_os.UsbguardGui.desktop
      - install -Dm644 data/io.github.onyks_os.UsbguardGui.metainfo.xml
          /app/share/metainfo/io.github.onyks_os.UsbguardGui.metainfo.xml
      - install -Dm644 packaging/70-usbguard-gui.rules.example
          /app/share/doc/usbguard-gui/70-usbguard-gui.rules.example
    sources:
      - type: dir
        path: .
      # Produced by flatpak-builder-tools/cargo/flatpak-cargo-generator.py
      - cargo-sources.json
```

No `--filesystem` permission of any kind is requested, consistently with §1.1.
Preferences live in the sandbox's own data directory.

The application ID must correspond to a domain or account the publisher
controls; `io.github.onyks_os` corresponds to the GitHub account and is what
Flathub's verification process checks.

### 14.2 Native Packages

`cargo-deb` and `cargo-generate-rpm`. Contents: the binary, the `.desktop`
file, the AppStream metainfo, icons, the GSettings schema, translations, and the
Polkit rule **as an example under `/usr/share/doc/`, not active** (§3.6).

Dependencies: a hard dependency on `usbguard`, and — this is the part it is easy
to get wrong — a hard dependency on the **bridge** package (`usbguard-dbus`),
because without it the program cannot function at all (§2.1). `polkit` is a
recommendation.

The post-install script modifies no system configuration. Its only action is the
GSettings schema recompilation that packaging conventions require.

---

## 15. Open Risks

1. **The numeric target mapping is not a stable contract.** It derives from the
   declaration order of a C++ enum. `Target::Other` (§2.3) limits the
   consequence to degraded display rather than a wrong authorization label, but
   an upstream reordering requires a new release. Detected by P0-2.

2. **Rule text as identity assumes stable canonicalization.** If a future daemon
   normalizes the text `listRules` returns differently, handles held across a
   long session stop matching. Mitigated by re-reading immediately before use
   (§6.4); the failure mode is `AlreadyGone` on a rule that still exists, which
   is confusing but not destructive.

3. **Denial attribution depends on error strings.** It is the only hook this API
   offers, and it must be recalibrated per USBGuard version. The
   `DeniedUnattributed` branch (§4.3) is what keeps this a user-experience
   concern rather than a correctness one.

4. **The tray's D-Bus name under Flatpak depends on the in-sandbox pid.** The
   declarations in §14.1 are an empirically derived workaround, not a
   guarantee. Where it fails, background mode remains fully functional.

5. **The removal race is irreducible client-side** (§6.4). Proposing
   `removeRuleIfMatches(id, expected_text)` upstream would eliminate it at the
   root and is the highest-value contribution to make to USBGuard from this
   project.

6. **The bridge is optional packaging on every reference distribution.** A large
   share of first-run failures will be `BridgeNotInstalled`, and the quality of
   that one message and its per-distribution install command determines whether
   most users ever see the program work.

None of risks 1 through 5 can be closed without upstream change. All six are
detectable at runtime, and each maps to a diagnostic state or a test above,
which is the property that matters: the program fails in a way it can explain.
