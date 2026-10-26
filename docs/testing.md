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

## Background memory check

On a Wayland or X11 desktop, run:

```bash
scripts/measure-memory.sh
```

It builds the debug GUI and demo bridge, runs both on a private session bus,
uses 20 invented USB devices and Cairo for a stable renderer baseline, then
reads RSS and PSS from the GUI process's `/proc/<pid>/smaps_rollup` after ten
minutes. It fails above 110 MiB RSS or 70 MiB PSS. No system USBGuard daemon,
USB policy, authentication, or user settings are touched. The run takes about
ten minutes; for a quick smoke check use `MEMORY_SECONDS=5`. Set
`USBGUARD_GUI_DEMO_DEVICE_COUNT` (7–200) or `MEMORY_RENDERER` to vary the load
or renderer; the RSS budget is enforced only for Cairo. These measurements are
specific to the local kernel, GTK build, and desktop libraries, so compare
them on the same machine and session type.

`USBGUARD_GUI_ISOLATED_INSTANCE=1` gives development runs a separate
GApplication ID so activating a demo cannot forward to the installed GUI. Pair
it with `USBGUARD_GUI_BUS=session` to use the demo bridge; the instance option
alone does not change which USBGuard bus the client connects to. Release builds
ignore the instance option.

**Measured 2026-10-25:** RSS 64,912 KiB (63.4 MiB), PSS 35,276 KiB (34.4 MiB),
after 600 seconds with 20 demo devices. Environment: Fedora 44, GNOME Shell
50.6 on Wayland, GTK 4.22.5, libadwaita 1.9.4, Linux 7.2.11, debug build,
Cairo renderer. Both limits passed. This is a repeatable local baseline, not
a GNOME/KDE comparison or a release-binary measurement.

## Native Wayland guest smoke check

The marked Fedora 44 guest also checks the installed candidate with Weston
15.0.1's headless backend, Pixman renderer and synthetic input seat. GTK uses
`GDK_BACKEND=wayland` with Cairo, a private session bus/runtime/configuration,
and no X11 fallback. This smoke check disables GTK accessibility; the separate
AT-SPI/Xvfb guest run verifies accessible names and keyboard focus.

The check allows up to 120 seconds for GTK startup under QEMU TCG and requires
the main `USBGuardGUI` toplevel's configuration acknowledgement, a buffer commit
on that same surface and its matching frame callback. The app must stay alive
for five more seconds. Registry discovery or an empty surface alone cannot
pass. The first exploratory launcher used a five-second deadline and stopped
before GTK finished starting; it was a test timeout, not evidence of a GUI crash.

For a guest already prepared and validated with the clean-install driver:

```sh
python3 scripts/verify-clean-install.py ssh --output target/clean-fedora \
  sudo dnf -y install weston
python3 scripts/verify-clean-install.py copy --output target/clean-fedora \
  scripts/clean-install-guest-wayland.py /home/usbguard-test/wayland-test.py
python3 scripts/verify-clean-install.py ssh --output target/clean-fedora \
  dbus-run-session -- python3 /home/usbguard-test/wayland-test.py
python3 scripts/verify-clean-install.py ssh --output target/clean-fedora \
  cat wayland-report.json
python3 scripts/verify-clean-install.py stop --output target/clean-fedora
```

The script refuses to run outside the marked VM or as root. Its report records
the installed executable, test-driver and protocol-log SHA-256 digests; raw
logs remain inside the disposable guest. Failed runs remove any stale passing
report. `make test-clean-install-driver` checks the VM guards and incomplete
protocol cases without starting a compositor or examining a live desktop.

The [Fedora Wayland receipt](validation/desktop-quality-2026-10-25/wayland.json)
matches the same candidate executable as the 5/5 GUI/PAM guest receipt. This is
a native Wayland rendering check; GNOME/KDE shell, tray, portal and interactive
session behavior remain separate milestone acceptance checks.

### Private GNOME/Plasma guest sessions

`scripts/clean-install-guest-desktop.py` runs only as the unprivileged test user
inside the marked disposable guest. Prepare the native package and synthetic
USB policy using the clean-install procedure first. Install GNOME Shell, KWin,
Plasma workspace, AT-SPI/Python bindings, dconf, software Mesa and the matching
desktop portal backends inside that guest. No host display, bus, device or
filesystem passthrough is needed.

Copy both scripts, preserving the helper filename, and run each desktop in a
fresh session bus:

```sh
python3 scripts/verify-clean-install.py copy --output target/clean-fedora \
  scripts/clean-install-guest-wayland.py /home/usbguard-test/clean-install-guest-wayland.py
python3 scripts/verify-clean-install.py copy --output target/clean-fedora \
  scripts/clean-install-guest-desktop.py /home/usbguard-test/desktop-test.py
python3 scripts/verify-clean-install.py ssh --output target/clean-fedora \
  flock --wait 300 desktop-test.lock dbus-run-session -- python3 desktop-test.py --desktop gnome
python3 scripts/verify-clean-install.py ssh --output target/clean-fedora \
  flock --wait 300 desktop-test.lock dbus-run-session -- python3 desktop-test.py --desktop kde
python3 scripts/verify-clean-install.py ssh --output target/clean-fedora \
  cat desktop-gnome-report.json desktop-kde-report.json
python3 scripts/verify-clean-install.py stop --output target/clean-fedora
```

Each run uses private XDG directories/dconf, a virtual Wayland output and software
rendering. Accessible nodes are matched to the exact GUI PID. Checks cover a
rendered main frame, named controls and filtering, the native preferences switch
and persisted value, one background notice, same-process relaunch, and ten
filter/hide/relaunch cycles. GNOME tests the no-watcher path; Plasma registers
with its real watcher and exercises tray activation and menu Quit. The earlier
GUI/PAM fixture supplies the separate Tab/Shift+Tab keyboard assertions.
Window presence is observed with one `GetChildren` request to the exact
application's AT-SPI root. Widget traversal resumes only while a window is
present; a separate cached child-count/index pair can race with hiding.

The primary GUI runs with `G_DEBUG=fatal-criticals`; any critical/error in its
combined application log also rejects the run. A failed run removes the previous
passing report. Successful reports record the executable, driver, helper and
protocol digests, GTK/compositor versions and SELinux state. `--cycles` accepts
1–100; use the default ten for acceptance. `--binary` allows a diagnostic build
inside the guest without replacing the installed package; its path/hash are
recorded, so that run must not be presented as installed-package evidence.

The corrected diagnostic executable passed seven cases on
[GNOME Shell 50.6](validation/desktop-quality-2026-10-26/gnome.json) and
[KWin/Plasma 6.7.5](validation/desktop-quality-2026-10-26/kde.json), including
ten GNOME and thirty Plasma hide/relaunch cycles. These are diagnostic-release
executables in the guest home, rather than new installed-package receipts.
The [investigation](validation/desktop-quality-2026-10-26/investigation.md)
distinguishes the fixed dialog lifetime from GTK's stale-index AT-SPI edge case
and records the observer correction. Fatal criticals remain enabled.

## Reconnection regressions

The supervisor is also tested with a bridge whose name stays owned while reads
fail. Tokio virtual time verifies delays of 500 ms, 1, 2, 4, 8, 16, and 30 seconds
(capped thereafter), no premature reconnection event, and prompt shutdown when
the UI closes. A private `dbus-daemon` test covers real name-owner replacement,
new signal subscriptions after restart, and absence of spurious subsequent loss.

## Coverage gate

Install GTK/libadwaita development packages, gettext, dbus-daemon, Xvfb,
xdotool and the `en_US.UTF-8` locale, then run:

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.9.1 --locked --root target/tools
make coverage-app
make coverage  # same gate, plus target/llvm-cov/html/index.html
```

The Make targets automatically add `target/tools/bin` to their own PATH; no
interactive shell PATH change is needed for that installation. A missing plugin
fails before profile cleanup with the exact install commands. GUI dependencies
are also checked before Rust tests: `scripts/test-gui.sh --check`. Installed
Xvfb/xdotool are preferred; an unpacked bundle under
`target/coverage-tools/root` (or `USBGUARD_GUI_TEST_TOOLS_ROOT`) is also supported,
including its private library directory. These checks do not install anything. Coverage runs GTK contracts in English
independently of the caller's LANGUAGE; the separate French focus regression is
unchanged.

The application gate requires **at least 98% of production lines across all
application source files**, including GTK, the executable and the build without
GUI. It resets workspace profiles, runs ordinary tests and isolated graphical
processes, then writes `target/coverage-application.json`. Tests live outside
`src/`, so their own lines do not inflate the denominator. No production file is
excluded for being difficult to exercise. The four files containing only module
exports/constants/types have no executable lines; missing coverage for any executable
source file fails the checker. Percentages are compared as integers: a rounded
98.00% below the threshold fails. Checker regressions run with `make test-coverage`.

`scripts/test-gui.sh` gives each process a private Xvfb display, memory GSettings,
user directories and session/system bus addresses. Its bus has no service
activation directories, so host portals, agents and keyrings cannot start.
USBGuard, notifications and the tray watcher are simulated on that private bus;
these tests never mutate the host's USB authorization. GUI contracts, the existing
focus regression and the real executable run separately so GTK is initialized on
one thread per process. A separate **180-second wall-clock test** checks both
20/180-second notices and cancellation through the actual GTK row.

`make coverage-core` remains a faster headless check, requiring **more than 90%
per file** in `src/rules/*.rs`, `dbus/commands.rs` and `dbus/errors.rs`; it is not
whole-application coverage. CI now enforces the complete `coverage-app` gate and
uploads its JSON. Line coverage is not branch coverage or proof of no remaining
bugs; real authentication, packaging and cross-distribution checks below retain
their separate scope.

Measured on 2026-10-25 (Rust 1.99.0, GTK 4.22.5, libadwaita 1.9.4):
**4,841 / 4,928 production lines = 98.2346%** across 38 executable source files.
The [committed receipt](validation/application-coverage-2026-10-25.json) records
per-file results and source/test digests. Parser, cancellation and error
classification each reached 100% line coverage. The ordinary suite passed
144 tests including the doctest; four display/lifecycle tests passed separately,
plus the headless-only CLI case. Nine coverage-checker regressions and all
52 existing Python regressions passed. Property tests also passed with
`PROPTEST_CASES=4096` (eight properties). Rust 1.89 all-target/all-feature checking,
Clippy, formatting, ShellCheck, Ruff, audit and dependency-policy checks passed.

Historical measurements from the earlier reliability revision (before the
whole-application gate; inline test code was then included):


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
Remote CI and release reproduction are required before publishing the stable tag;
the successful runs are recorded below.

The first remote candidate run passed all three Rust toolchains, lint and the
core-coverage gate. Its dependency-audit action reported zero vulnerabilities
but failed when trying to create a separate check-run with a read-only token.
The replacement invokes pinned cargo-audit directly, preserving vulnerability
failures and informational warning logs without adding token write permissions.

## Remote 0.1.2 validation (2026-10-24)

[Main CI](https://github.com/onyks-os/USBGuardGUI/actions/runs/40095494547) passed
on clean commit `d6d984f08e0342f32a4ef7ec79e0debd4111dcc6`, including the audit
replacement, stable/beta/Rust 1.85 tests, the separate graphical regression,
release compilation and both independent offline builds of every package format.
CodeQL and documentation also passed. Every enforced core file remained above
90% line coverage (the lowest measured file was `dbus/commands.rs`, 90.91%).

The [release-candidate pipeline](https://github.com/onyks-os/USBGuardGUI/actions/runs/40103412714)
passed the same comparison and the installed Flatpak test, then published signed
artifacts. All downloaded checksums and all 14 Sigstore signatures (including
`SHA256SUMS`) were verified against the exact release workflow/tag identity.
Artifact hashes also matched the reproduction receipt and its clean source revision.
The Flatpak report confirmed the pinned GNOME commit and installed CLI/schema/assets.
The local Sigstore 3.6.6 verifier emitted an unsupported-root-key warning while
successfully validating every artifact with its supported trusted keys.
These checks do not substitute for interactive real-daemon/Polkit validation.

## Dependency license/source policy (2026-10-24)

`make dependency-policy` passed with cargo-deny 0.20.2 against the committed
application lockfile and all features/targets. Six real-tool regression tests
cover nine cases: forbidden licenses in normal, development, build and
non-host-target dependencies; optional dependencies; missing licenses; SPDX
AND/OR choices; and an unapproved Git source despite an exact revision pin.
Fixtures use disposable local packages and an isolated Cargo home. No fixture
compiles code or fetches an external dependency.

The updated `make verify` passed all 108 standard Rust tests, Clippy/rustfmt,
ShellCheck, the secret scan, cargo-audit and dependency-policy checks. Ruff and
TOML/workflow-YAML parsing also passed. The ignored graphical regression remains
a separate CI check. Markdownlint was unavailable locally and was reported as
skipped. The existing per-file core-coverage gate remains enabled in CI.

The application MSRV and reproducible toolchain are unchanged. Upstream metadata
for zbus 6.0.0/6.1.0 requires Rust 1.89, while published ksni 0.3.7 still requires
zbus 5; `DEPENDENCIES.md` records these coordinated-migration prerequisites.

## French feature increment (next 0.2.0 target)

The new French catalogue translates all 213 current messages. Seven gettext tests
check current-template completeness, plural counts 0/1/2/40, security/action
labels, rule-keyword preservation, actual rejection of a renamed placeholder,
localized desktop/AppStream metadata and native package catalogue declarations.
All 30 reproducibility tests also passed, including installed-Flatpak rejection
when a registered catalogue is missing or its language list is empty/invalid.

`make verify` passed the 108 standard Rust tests, lint, audit and dependency
policy gates. The separate GTK focus/authentication regression passed once in
English and once in French on a private D-Bus with memory settings and the local
display; its French run asserts translated action labels before exercising the
20 cancellation/refresh cycles. The initial C.UTF-8 test environment disabled
gettext translations, so the test uses en_US.UTF-8 with LANGUAGE selecting the
catalogue. CI explicitly generates that locale and uses Xvfb for both runs.
The first local virtual-display attempt could not run because xvfb-run was not
installed; it is a CI test dependency. Real host USBGuard policy was not changed.

The French catalogue is an initial AI-assisted translation; fluent-speaker
improvements remain welcome. Publication of 0.2.0 still requires the documented
release gates; this feature increment does not retroactively alter 0.1.2 assets.

The first remote lint run exposed a gettext-version difference: the runner's
msgfmt accepted a renamed Rust-format placeholder that the local tool rejected.
`scripts/check-translations.py` now checks compiled catalogues independently,
including every plural form. Both normal and reproducible builds invoke it;
the reproduction source archive explicitly includes the checker. Negative tests
cover renamed, removed, extra and plural-only placeholder changes and CLI failure.

The next remote graphical test correctly failed because the matrix runner lacked
msgfmt before the first Cargo build: build.rs had selected its documented
English-only fallback. The matrix now installs gettext before compilation, so
French must actually load for the graphical assertions to pass.

## zbus 6 and real clean-system validation (2026-10-24–25)

The post-0.1.2 implementation uses zbus 6.1 and Rust 1.89. The direct fixed
SNI/menu interfaces replace ksni, removing the deprecated standalone zvariant
stack. Private-bus tests verify wire signatures, native/Flatpak registration,
menu actions, both tooltip notification mechanisms, watcher restart and shutdown.
The 114 standard Rust tests and separate GTK cancellation regression pass;
coverage remains above 90% in every required core file. The dependency audit
has no vulnerabilities or informational warnings, and license/source checks pass.

Native package verification now also runs against real USBGuard, system D-Bus,
Polkit and PAM inside disposable QEMU guests:

| Guest | Kernel | USBGuard package | Agent | GUI/PAM cases |
| :---- | :----- | :--------------- | :---- | :------------ |
| Ubuntu 24.04.5 | 6.8.0-147-generic | 1.1.2+ds-6build2 | polkit-gnome | 4/4 passed, October 24 |
| Debian 13.7 | 6.12.113+deb13-amd64 | 1.1.3+ds-3 | mate-polkit | 4/4 passed, October 25 |
| Fedora 44 (baseline) | 6.19.10-300.fc44.x86_64 | 1.1.4-1.fc44 | mate-polkit | 4/4 passed, October 25 |
| Fedora 44 (desktop-quality candidate) | 6.19.10-300.fc44.x86_64 | 1.1.4-1.fc44 | mate-polkit | 5/5 passed, October 25; AT-SPI and Tab focus |
| Arch rolling | 7.2.11-arch1-1 | 1.1.4-11 | polkit-gnome | 4/4 passed, October 25 |

The daemon/Polkit suite starts the installed GTK app as the unprivileged test
user under Xvfb/Openbox, with memory settings and a software renderer. It
filters the synthetic USB disk, requests Block / This session only, and checks:

1. Without an agent, the request is denied, the device stays allowed and the GUI survives.
2. Dismissing the real agent's password dialog denies the request without changing the device.
3. A correct password passes real PAM authentication, blocks the device for the
   current session and leaves the persistent ruleset byte-for-byte unchanged.
4. Restarting both USBGuard services restores persistent allow, and the GUI
   reconnects and displays the refreshed state.
The desktop-quality candidate adds a fifth check: accessible names are exposed
for the filter, device list, details action and main menu; Tab leaves the filter
and Shift+Tab returns focus to it.

The guest driver performs the accessibility and keyboard checks only inside the
disposable Xvfb guest; the host desktop is not queried. The original October
24–25 runs predate these assertions and remain evidence for the four
daemon/Polkit cases above. A Fedora 44 run against the desktop-quality candidate
passed all five checks; its package and executable hashes are recorded in
[the desktop-quality validation receipts](validation/desktop-quality-2026-10-25/).

The October 25 driver additionally observes actual system-bus calls and denials
inside the VM. It asserts the synthetic device ID, block target and `permanent=false`,
so unchanged device state alone cannot make a missing GUI action pass. The host
verifier rejects incomplete/failed reports and mismatches between the installed
binary and the reproducible-build receipt; nine driver tests cover isolation,
input rejection, stale-report handling and receipt validation.

The VM marker guards root setup; no physical USB device or host filesystem is
passed through. Credentials stay in private ignored directories. Packaged
Polkit action and bus-policy files are recorded separately from the explicit
remote-test-user rule: reads are allowed and every write uses AUTH_SELF without
cached authorization. This verifies real authentication, not distribution-default
permission grants for a particular desktop seat. The app package never installs
that test rule. The GUI/PAM evidence covers these virtual devices, agents and
package versions; physical hotplug and other desktop environments are separate checks.

Distribution-specific setup findings:

- Debian's genericcloud kernel omits xHCI. The tested VM installed and booted the
  distribution's standard kernel; use the generic image for future fresh tests.
- Newly installed Polkit socket units need starting when sockets.target has
  already been reached. The Arch fixture starts the supplied helper socket;
  its helper keeps the distribution's non-SUID permissions.
- Fedora 44 supplies mate-polkit rather than polkit-gnome. SELinux stayed enabled;
  AT-SPI launcher/registry were started explicitly for the synthetic session.
- Reload the daemon after generating fixture rules even if package installation
  has already started it. Enable/start alone does not reload an active daemon.
- Detach the document portal's mount inside the private test runtime before
  removing it, so a cleanup error cannot mask an authentication result.

Reproduce with the guarded scripts, a hash-verified official image and a fresh
output directory (host tools: QEMU, qemu-img, genisoimage, OpenSSH and OpenSSL):

```sh
python3 scripts/verify-clean-install.py prepare --distro fedora \
  --image /path/to/verified-fedora.qcow2 --sha256 VERIFIED_IMAGE_SHA256 \
  --output target/clean-fedora
python3 scripts/verify-clean-install.py copy --output target/clean-fedora \
  scripts/clean-install-guest-setup.sh /home/usbguard-test/setup.sh
python3 scripts/verify-clean-install.py ssh --output target/clean-fedora \
  sudo bash /home/usbguard-test/setup.sh
python3 scripts/verify-clean-install.py validate --output target/clean-fedora \
  --package target/reproducible/first/usbguard-gui-0.1.2-1.x86_64.rpm
python3 scripts/verify-clean-install.py collect --output target/clean-fedora
python3 scripts/verify-clean-install.py stop --output target/clean-fedora
```

Select the native package produced by the measured build; its actual version may
change. A failed run must be investigated before accepting a receipt. This is
an explicit VM test, not part of `make verify` on a developer's live USBGuard system.

Committed receipts are in
[the Ubuntu checkpoint](validation/zbus6-checkpoint-2026-10-24/ubuntu.json) and
[the completed October 25 runs](validation/zbus6-clean-install-2026-10-25/).
See [verification scope](verification.md#post-012-zbus-6-validation-2026-10-2425)
for the exact candidate artifact identity and reproducibility evidence.
