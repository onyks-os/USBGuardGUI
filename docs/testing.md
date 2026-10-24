# Reliability validation

These notes describe the unreleased follow-up to 0.1.1, validated on 2026-10-18.
They distinguish automated mock coverage from checks requiring a real daemon,
Polkit agent, desktop, or distribution installation.

## Automated scenarios

Run the complete suite, including the GTK modules:

```bash
cargo test --locked --all-features
```

The D-Bus tests use private peer connections and invented device data. They
require local Unix sockets and `dbus-daemon`, but no USBGuard service, privileges, or display.

- **T2:** interrupt a permanent device write before execution and after execution
  but before its reply. Both outcomes must be `AppError::Inconclusive`. A fresh
  bridge/worker reads the actual device and rule state, and only one write is
  recorded. T1/T15 independently cover supervisor reconnection/cache invalidation.
- **T13:** hold an `applyDevicePolicy` call as an authentication prompt would,
  advance Tokio's clock by ten minutes, verify it is still pending and reads are
  responsive, then release it and check the actual result. No wall-clock wait
  or real Polkit prompt is involved. Unit tests cover notice transitions just
  before and at 20/180 seconds, and at ten minutes.
- **Cancellation:** cancel a pending D-Bus operation, then allow the mock daemon
  to execute it. The client reports cancellation/cache invalidation, observes
  the eventual daemon state, and emits no later success for the cancelled task.
- **Log privacy:** daemon errors include sentinel names, serials, and hashes.
  Warn/info logs omit them; debug retains the diagnostic detail. Binary tests
  verify a privacy notice for debug, trace, and module-specific filters,
  including `zbus=trace`.

The window requests fresh device and policy state when a write is inconclusive;
policy loading remains lazy when its page is hidden. Reconnection also triggers
fresh state. Reads/preflight failures remain ordinary errors, and explicit
refusals remain denials/rejections. No uncertain write is retried automatically.

Actual Polkit interactions, GTK timer rendering, and a restart of the real
USBGuard daemon behind its bridge still need manual integration checks.

## GTK focus regression

A real Fedora/GTK 4.22.5 crash after cancelling Polkit was reproduced in
`gui_device_actions_survive_denied_authentication_and_refresh`: focus a device
action and its popover, mark the operation pending, move focus to a transient
authentication window, refresh the row, and close the transient window. Before
the fix, the test segfaulted in `gtk_column_view_row_widget_get_column_view`
when GTK restored focus to a detached row, matching the user's kernel crash.

`DeviceView` now clears window focus only when it belongs to the old device
rows, before either replacing or removing them. Search/toolbar focus is retained.
The test repeats the sequence 20 times, also exercises reconnection reset, and
asserts that refreshing does not steal the search entry's focus.

The test is ignored by default because it needs a display. Run it separately:

```bash
GSETTINGS_BACKEND=memory GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  cargo test --locked --all-features --lib gui_device_actions -- --ignored --test-threads=1
```

CI runs it under Xvfb on stable Rust. The reproduction uses a transient GTK
window and synthetic devices, with no real USBGuard or authentication request.
The compositor-specific crash was observed locally on Wayland; headless Xvfb
also exercises the explicit focus-preservation assertions.

## Reconnection regressions

The supervisor is also tested with a bridge whose name stays owned while reads
fail. Tokio virtual time verifies delays of 500 ms, 1, 2, 4, 8, 16, and 30 seconds
(capped thereafter), no premature reconnection event, and prompt shutdown when
the UI closes. A private `dbus-daemon` test covers real name-owner replacement,
new signal subscriptions after restart, and absence of spurious subsequent loss.

## Coverage gate

Install the development tools, then run:

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.9.1 --locked
make coverage-core
```

This executes the headless suite and writes `target/coverage-headless.json`.
`scripts/check-coverage.py` requires **more than 90% line coverage per file** in
`src/rules/*.rs`, `src/dbus/commands.rs`, and `src/dbus/errors.rs`. Missing or
empty coverage for a required file fails the gate. CI runs the same target.
The existing `make coverage` produces an HTML report with all features enabled.

The headless report does not measure GTK workflows. A high parser percentage
must not be presented as whole-application coverage. Standard LLVM reports also
count inline unit-test code; the JSON preserves the detailed regions for review.
Property-generated inputs can slightly change the measured percentages between runs.

Measured results for this revision:

| Check | Result |
| :--- | :--- |
| Complete suite with GTK modules | 108 tests passed, including the doctest; 1 display test ignored by default |
| Graphical regression run separately on Wayland | Passed (20 focus/denial cycles, reset, search focus) |
| Rule parser line coverage | 228/245 (93.06%) |
| Other rule-language files | 97.80–99.32% |
| Cancellable operations | 90/99 (90.91%) |
| Error classification | 149/161 (92.55%) |
| Entire measured headless source | 2533/3007 (84.24%); GTK workflows excluded |
| Clippy, rustfmt, ShellCheck, Python lint, translation checks | Passed |
| Rust 1.85 headless compatibility | `cargo +1.85 check --locked --offline --no-default-features` passed |

The coverage checker was also exercised with modified reports: a missing file,
an empty file, and exactly 90% each returned failure. The HTML counterpart of
the measured headless report is in `target/coverage-core/html/index.html`.

## Filesystem measurement

Build development binaries with symbols, then run on a graphical session:

```bash
cargo build --locked --bin usbguard-gui --example demo-bridge
scripts/trace-filesystem.sh
```

The script requires `strace` with stack-trace support and `--stack-trace-frame-limit`,
`dbus-run-session`, `gdbus`, and `timeout`. It starts a **private session bus**
with the demo bridge, redirects development builds to that bus, uses in-memory
GSettings and isolated XDG config/cache/data directories, and traces background
GUI startup for 60 seconds plus a completed `--diagnose` invocation. The GUI
must reach a `listDevices` snapshot or the script fails. Slow tracing hosts can
increase the duration, for example `TRACE_SECONDS=120 scripts/trace-filesystem.sh`.
No USBGuard policy or system configuration is modified.

Outputs remain under `target/filesystem-trace/`, including call stacks and logs.
They are local artifacts, not committed fixtures. Inspect syscalls naming `/etc`,
`/var`, or `/sys` together with their stacks; correlate unfinished/resumed calls
by thread ID rather than counting their lines as independent calls.

Observed on Fedora 44, GTK 4.22.5, libadwaita 1.9.4, strace 7.2, Wayland,
software renderer, debug build:

| Origin in the stack | GUI calls naming system paths | Diagnostic calls |
| :--- | ---: | ---: |
| Dynamic loader | 3 | 2 |
| SELinux | 3 | 3 |
| GnuTLS initialization | 3 | 3 |
| GLib/GIO configuration/theme lookup | 11 | 0 |
| Fontconfig | 380 | 0 |
| XKB configuration lookup | 1 | 0 |
| GTK configuration/theme lookup | 12 | 0 |
| libc allocator CPU discovery | 1 | 0 |
| **Total** | **414** | **8** |

Examples include `/etc/ld.so.cache`, `/etc/fonts/`, `/etc/gtk-4.0/`, Flatpak
exported themes under `/var/lib/flatpak/`, and libc's read of
`/sys/devices/system/cpu/online`. No direct first-party system-path file
operations, opens for writing, or filesystem mutations were observed.
The GUI read seven demo devices and diagnostics reported `connected`.

This validates background startup and diagnostics in this environment. It is
not a syscall sandbox, a claim about every workflow, or a guarantee that the
entire process makes zero system accesses. Other renderers, distributions,
fonts, libraries, native installation paths, and user actions can change the
trace. New first-party filesystem operations should be checked against the
same objective and attributed separately from library behavior.

## Reproducibility tooling

`make test-reproducible` runs 30 Python unit tests without a container. They cover
source normalization, required embedded resources, exclusion of Cargo credentials,
freezing recipe inputs during a build, changed/empty/missing binaries, leaked
build paths, failed subprocesses, explicit worktree opt-in, and refusing existing
output directories. RPM tests verify payload bytes, modes, timestamps, dependencies,
missing/empty/extra/different packages, digest verification and query recording. Mocked orchestration verifies that failure leaves no passing
report and that successful metadata identifies the exact source and recipe.

Measured statement coverage of `scripts/reproducible-build.py`: **96%** with
Coverage.py 7.15.4. The RPM staging/build driver has **98%** coverage; its
remaining line is the CLI entry point. The remaining lines are the command-line entry wrapper and
the extra missing-GUI rejection branch. The actual two-container compilation
and comparison is an additional integration check; its commands and scope are
in [verification.md](verification.md#5-reproducing-the-build). Normal Rust tests
remain green (108 tests plus the separately-run graphical regression).

The actual offline comparison passed on 2026-10-18: both GUI binaries had
SHA-256 `82881271b4c01347f88c365654b2d1aedbacb2f93f452747bf9b849ea29ca247`.
Inputs were the explicitly acknowledged modified worktree, captured as an
immutable source archive; this is not an assertion about the old base commit.
The report is in `target/reproducible-verified/report.json`; full provenance
and scope are documented in [verification.md](verification.md#local-validation-2026-10-18).

RPM integration validation also passed for two independent offline builds on
2026-10-18. Complete package SHA-256:
`37bb1589da9d9c93146991b40afc9fbeaa9cf695f657092b019bad3739e883da`.
The receipt is `target/reproducible-rpm-final/report.json`. Each package was
extracted, its ELF compared to the verified binary, its GSettings schema checked,
and its CLI started. No install scriptlets were present.

The same package was installed with DNF in an isolated Fedora 44 container:
file verification (`rpm -V`), installed CLI startup and schema validation passed.
See `target/reproducible-rpm-probe/fedora-install.log`. This checks native
installation, not an interactive desktop/Polkit session.

DEB tests cover staged bytes/modes/checksums, unsafe destinations, generated and
explicit dependencies, multiline descriptions, timestamp normalization, build
flags, metadata queries, and refusal to publish a success report when DEBs differ.

Measured statement coverage for `deb.py`: **98%** (only its CLI entry point is
uncovered). Combined packaging/checker coverage is **97%** with Coverage.py 7.15.4.

The complete final DEB comparison passed on 2026-10-18 with SHA-256
`5118891634fa2a75c2c3426b0ed982f2037c5f60c74d0e40dfaaa3ce3ed4b879`. Receipt:
`target/reproducible-deb-final/report.json`. RPM and bare-binary hashes are
unchanged; extracted DEB checksums, ELF identity, CLI and schema checks passed.

Ubuntu 24.04 installation validation passed with APT, `dpkg --verify`, installed
CLI startup and GSettings schema discovery. The container explicitly installed
`libglib2.0-bin` to provide the test utility `gsettings`. Log:
`target/reproducible-deb-final/ubuntu-install-with-tools.log`.
Interactive desktop/Polkit behavior was not exercised by this packaging test.

Arch tests execute the adapter with the real PKGBUILD `package()` and substituted
packaging commands, checking staged assets, modes, license, inert Polkit example,
unchanged binary bytes, inherited dependencies, disabled stripping/debug splitting,
metadata queries and mismatched-version rejection. Full-driver tests reject differing
Arch packages without writing a passing report. Separate real-makepkg builds and
installation probes complement these tests.

The final Arch integration comparison passed on 2026-10-18. Complete package
SHA-256: `cece846c58e59c3e1794b50dd8659c01839b3c0523d2631ae133bc7bc0cbbee8`.
Receipt: `target/reproducible-arch-verified/report.json`. Both extracted payloads
passed ELF identity, schema and CLI checks. Core Python tooling retains 96% checker
coverage, 98% DEB/RPM builder coverage and 97% combined statement coverage. The new
shell adapter is exercised by the positive/negative unit test and real makepkg runs.

Installation in a temporary updated Arch container passed: pacman resolved runtime
dependencies and installed the exact byte-identical probe package, `pacman -Qkk`
reported 29 total files / 0 altered files, CLI help succeeded and GSettings found
the application schema. Italian translation and inert Polkit example were present.
The minimal base image's `NoExtract` filters were disabled only inside the test
container so documentation and translations could be checked. The initial probe
retains its failure log for that excluded documentation directory.
Successful log: `target/reproducible-arch-probe/arch-install-full.log`.
No host installation or interactive desktop/Polkit test was performed.

Flatpak tests exercise manifest identity/command validation, permission reuse,
staged and imported ELF integrity, normalized timestamps, export/import commands,
private HOME/XDG/Flatpak directories, runtime commit enforcement, failed CLI or
subprocess rejection and failure logs. Full-driver tests reject different bundles
and different Flatpak executables without writing a passing report. Unit tests
now require PyYAML, matching the parser installed in the pinned builder and CI.

Measured statement coverage: **98%** for the Flatpak exporter and **98%** for the
private runtime verifier (only their CLI entry points are uncovered). Combined
Python reproducibility-tool coverage remains **97%**. Regression tests assert
that only the bundle exporter receives the fixed wall clock, with real monotonic
clocks, and that UTC commit timestamps use the format required by Flatpak.

The final complete Flatpak build comparison passed on 2026-10-18. Bundle SHA-256:
`8625a89f97d043f66c08360525f74733b5bb3e3f2ae7a4a3b3e1aee62567edc6`. Receipt:
`target/reproducible-flatpak-fixed/report.json`. Both app executables and OSTree
commits were equal; the whole bundle became equal after freezing the separate
OSTree delta-generation clock. No bytes are ignored or patched for comparison.
Private installed-runtime tests passed for this exact bundle both with a cached
local runtime and with a Flathub download, enforcing the pinned runtime commit
and checking actual sandbox CLI/schema/assets. Reports:
`target/flatpak-runtime-fixed/report.json` and
`target/flatpak-runtime-network/report.json`. Native hashes remained unchanged.
Interactive desktop/Polkit and remote GitHub execution were not exercised.

## 0.1.2 candidate validation (2026-10-24)

The local `make verify` gate passed after integrating the remote main updates
and updating Tokio to 1.53.3. Rust stable passed 108 standard tests; the separate
GTK authentication/focus regression also passed on the graphical session with a
private D-Bus and in-memory settings. Rust 1.85 headless tests passed outside the
execution sandbox, which otherwise denies the private test sockets. Thirty Python
packaging/reproducibility tests passed, as did Ruff, ShellCheck, rustfmt and Clippy.
Generated desktop/AppStream files were checked for repeatable regeneration.
Markdownlint was not installed and the make target reported that check as skipped.

The updated audit has zero vulnerability entries and no unsoundness warning;
three informational unmaintained notices remain for the zbus 5 transitive stack.
The dependency inventory records their scope and the planned coordinated migration.
Remote CI and release reproduction are required before publishing the stable tag.

The first remote candidate run passed all three Rust toolchains, lint and the
core-coverage gate. Its dependency-audit action reported zero vulnerabilities
but failed when trying to create a separate check-run with a read-only token.
The replacement invokes pinned cargo-audit directly, preserving vulnerability
failures and informational warning logs without adding token write permissions.
