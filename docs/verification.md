# Release Verification

Every USBGuardGUI release is published with checksums, a Sigstore signature, and a CycloneDX
SBOM. This guide shows how to verify an artifact before installing it.

Each release carries the `.deb`, the `.rpm`, the Arch package (`.pkg.tar.zst`), the `.flatpak`
bundle, the bare binary, the `.crate`, and `sbom.json`; next to each one, its Sigstore bundle
(`<artifact>.sigstore.json`), and a `SHA256SUMS` file covering them all. The examples below use
version 0.1.1:

```bash
VERSION=0.1.1
gh release download "v$VERSION" --repo onyks-os/USBGuardGUI   # or download from the web page
```

## 1. Verify the Checksums

Download the artifact and `SHA256SUMS` from the
[Releases page](https://github.com/onyks-os/USBGuardGUI/releases), then:

```bash
sha256sum --check --ignore-missing SHA256SUMS
```

Expected output: `<artifact>: OK`.

## 2. Verify the Sigstore Signature

Release artifacts are signed keylessly through GitHub Actions OIDC — there is no long-lived private
key to steal. Install [`sigstore`](https://pypi.org/project/sigstore/) and verify:

```bash
python -m pip install sigstore

sigstore verify identity \
  --cert-identity "https://github.com/onyks-os/USBGuardGUI/.github/workflows/release.yml@refs/tags/v$VERSION" \
  --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
  <artifact>
```

`sigstore` finds the bundle `<artifact>.sigstore.json` next to the artifact by itself.

## 3. Verifying Signer Identity

The `--cert-identity` value must exactly match the release workflow path and the tag being verified.
A signature that verifies against a *different* identity is not a valid USBGuardGUI release,
even if the cryptography checks out.

## 4. Inspect the SBOM

```bash
jq '.components[] | {name, version, licenses}' sbom.json
```

Compare the component list against [`DEPENDENCIES.md`](../DEPENDENCIES.md). Any component present in
the SBOM but absent from that file should be reported as an issue.

## 5. Reproducing the Build

The unreleased pipeline adds a reproducibility gate for the **Linux x86_64 GUI
binary, unsigned RPM, DEB, Arch package and Flatpak bundle**. Published 0.1.0/0.1.1 artifacts predate this gate. The crate archive, SBOM, and signatures are outside this claim;
checksums and signing still establish their provenance, not byte reproducibility.

### Fixed build inputs

The recipe lives in [`packaging/reproducible/`](../packaging/reproducible/Containerfile):

| Input | Fixed value |
| :--- | :--- |
| Platform | `linux/amd64`, Rust target `x86_64-unknown-linux-gnu` |
| Ubuntu base image | `docker.io/library/ubuntu@sha256:008173c23f95b170204355c12626cb5a965d779a7e1283b09e9cffbb1bf33ca3` |
| Ubuntu archive | Snapshot `20261001T000000Z`, suites noble/noble-updates/noble-security |
| Compiler | Rust/Cargo 1.88.0, LLVM 20.1.5 |
| Rust archive integrity | SHA-256 in `packaging/reproducible/rust.sha256` |
| Crates | Committed `Cargo.lock`; vendored before compilation, verified by Cargo |
| Build profile | Release, default GUI feature, baseline `x86-64` CPU |
| Locale and timezone | `C.UTF-8`, `UTC` |
| Build time input | `SOURCE_DATE_EPOCH` from the source commit's timestamp |

The minimal Ubuntu image has no CA bundle: a fixed Ubuntu CA package is downloaded
over HTTPS by the engine and checked by SHA-256 before extraction. APT then verifies
the snapshot with Ubuntu's archive keyring. The Rust archive is also checked before
installation. The compiler pin is local to this recipe; development and the
stable/beta/MSRV test matrix retain their existing toolchain choices.

The [Ubuntu snapshot service](https://snapshot.ubuntu.com/) fixes native build
dependencies. Rust's [source path remapping](https://doc.rust-lang.org/rustc/remap-source-paths.html)
normalizes source and dependency paths. Remapping does not rewrite arbitrary
`env!` string values: release builds therefore no longer embed development-only
GSettings/catalogue directories from `OUT_DIR`.

### Run the comparison

On a clean checkout of the release tag, with Python 3.11+ and rootless Podman:

```bash
git clone --branch "v$VERSION" https://github.com/onyks-os/USBGuardGUI.git
cd USBGuardGUI
make test-reproducible
make reproducible
```

Use `CONTAINER_ENGINE=docker make reproducible` with a Docker daemon instead.
Internet access is needed to prepare the builder and vendor crates. Both actual
compilations use `--network=none`, `cargo build --frozen`, independent containers,
and no shared compiler cache. Their build roots differ (`/tmp/repro-one` and
`/tmp/independent/repro-two`); compiler flags map both to one stable logical path.

The script freezes a normalized source archive and the recipe/driver before
building. Source selection is restricted to Cargo manifests/lockfile, the build
script, `src/`, `data/`, `po/`, `assets/`, README, license, recognized Cargo config
files, the embedded Polkit example, Arch PKGBUILD, rule fixtures, and an optional root toolchain file. Local Cargo credentials and unrelated
untracked files are excluded. Archive paths, modes, ownership, ordering, and
mtimes are normalized without changing the checkout.

The checker compares the complete binary bytes, checks for leaked temporary
build paths, inspects the GTK/libadwaita dynamic dependencies, and executes
`--help`. It also compares the complete RPM bytes (headers and compressed payload),
verifies RPM digests, extracts each package, checks the installed ELF against the
verified binary, and runs its `--help`. Only success creates `report.json` and `SHA256SUMS`:

```bash
cat target/reproducible/report.json
(cd target/reproducible && sha256sum --check SHA256SUMS)
```

Results also include both binaries, RPMs, DEBs, Arch packages and Flatpak bundles, package manifests/dependencies/scriptlet queries, source/recipe archives, compiler information,
installed package versions, and logs. On failure, inspect `*.log` and retained
binary outputs; there is no passing report. Output directories are never reused:
choose another with `REPRO_ARGS="--output target/reproducible-another"`.

For intentional local edits, use an explicit opt-in:

```bash
make reproducible REPRO_ARGS="--worktree --output target/reproducible-local"
```

This includes modified and new source files, and records `modified_worktree: true`.
It proves equivalence for that archived snapshot, not for the unmodified commit
named in the report. The source archive's SHA-256 identifies the exact input.

### Local validation (2026-10-18)

Two complete GUI release builds passed in 1 min 29 s each, in separate
network-disabled containers with different build roots and no shared target cache.
Their binary SHA-256 was:

```text
82881271b4c01347f88c365654b2d1aedbacb2f93f452747bf9b849ea29ca247
```

The measured source archive SHA-256 was
`516956e1466d3c69b32557e962bed94a2173bb07f27b125a85dec3107035fe9c`;
the recipe digest was
`98aca06f26b4b5de0a764ea49ca91828ba2041b5a6a7314149c6d860deb0fe58`.
GTK 4.14.5 and libadwaita 1.5.0 were linked, both `--help` smoke checks passed,
and no temporary build path was found. The local result is preserved in
`target/reproducible-verified/report.json` with both artifacts and logs.

This validation explicitly used `--worktree`: it includes the current local
changes over commit `9817d3feacb76899e58e237d99cc84d765c640dd`. It does not claim
that the unmodified commit, or an older published release, has this binary hash.
The exact input is the archived snapshot identified above. CI/release workflow
changes have been validated locally but still require their first GitHub run.

### Check a downloaded bare binary

New releases stage the bare `usbguard-gui` binary, RPM and DEB exclusively from the successful
comparison job. They also publish `reproducible-build.json`,
`reproducible-source.tar`, `reproducible-recipe.tar`,
`reproducible-packages.txt`, and `reproducible-toolchain.txt`; the usual release
checksums and Sigstore signing cover them. Compare a downloaded binary against
your rebuilt result:

```bash
cmp downloaded-usbguard-gui target/reproducible/first/usbguard-gui
```

The report records the binary, source, and recipe digests, the source commit and
its timestamp, and the builder image ID used in that run. The local image ID is
a receipt, not a pullable image reference; rebuild the image from the frozen
recipe. Image layers themselves are not claimed reproducible.

The guarantee is scoped to this recipe and target. Builds on a different host
distribution without the pinned container need not match. The binary dynamically
links host libraries at runtime; their behavior is not covered by its hash.
Changing the pinned compiler, snapshot, base image, dependency lockfile, or code
requires rerunning the comparison. Availability of the pinned downloads remains
a prerequisite. Flatpak runtime updates require their own validation.

### RPM packaging

The same pinned Ubuntu snapshot supplies `rpmbuild`, Python, gettext, cpio and
RPM dependency generators; their exact versions are recorded in `packages.txt`.
[`rpm.py`](../packaging/reproducible/rpm.py) reads assets, modes and explicit
runtime dependencies from `Cargo.toml` and stages
the verified ELF without stripping it again. The worker compiles the Italian catalogue.
Automatic ELF requirements remain
enabled. No install scriptlets or active Polkit authorization rules are added.

The [RPM reproducibility macros](https://rpm.org/docs/6.0.x/manual/buildprocess.html#reproducibility)
fix BuildTime from `SOURCE_DATE_EPOCH`, clamp file timestamps and use a fixed
BuildHost. Staged file timestamps are all set to the epoch, ownership is root/root,
and the payload uses fixed gzip compression. The complete unsigned RPM must match;
no headers are discarded during comparison. Detached release signing remains a
separate step. `make package-rpm` still provides the ordinary host-dependent build;
`make reproducible` is the checked path used for release RPMs.

The extraction smoke check verifies payload integrity and CLI startup. It does not
install into the host or exercise its desktop triggers/USBGuard service. To test
on Fedora after a successful comparison, reinstall the generated package and
launch from the desktop (or `usbguard-gui`):

```bash
sudo dnf reinstall ./target/reproducible/first/usbguard-gui-*.rpm
usbguard-gui
```

Use `dnf install` instead if the package is not installed yet.

### Local RPM validation (2026-10-18)

The complete binary/RPM comparison passed for the archived modified worktree
with source SHA-256 `516956e1466d3c69b32557e962bed94a2173bb07f27b125a85dec3107035fe9c`.
Both independent RPMs have SHA-256:

```text
37bb1589da9d9c93146991b40afc9fbeaa9cf695f657092b019bad3739e883da
```

Recipe SHA-256: `b3e5c04dfeeb1336c822f075ef6aaf2ceff2c84e954ebe757524daa2c4b71923`.
The report and both outputs are in `target/reproducible-rpm-final/`.
The snapshot supplied RPM 4.18.2+dfsg-2.1build2 and Python 3.12.3-0ubuntu2.1.
Both extracted ELFs retained the binary hash recorded above; CLI help, strict
GSettings schema validation and the absence of scriptlets were checked.
The RPM header reports `reproducible.invalid` and build time `1791329941`.
CI/release execution on GitHub remains pending; the result is local evidence.

A separate temporary Fedora 44 container successfully installed the same RPM
with DNF, resolved its runtime dependencies, passed `rpm -V usbguard-gui`, ran
`usbguard-gui --help` and validated the installed schemas. The probe package was
byte-identical to the two final packages. Its log is
`target/reproducible-rpm-probe/fedora-install.log`. No host installation or daemon
restart was performed. Interactive GUI/Polkit testing remains a manual check.
The verified RPM was copied to `dist/usbguard-gui-0.1.1-1.x86_64.rpm`.

### DEB packaging

The same pinned image provides `dpkg-deb`, `dpkg-shlibdeps` and their library
symbols metadata. [`deb.py`](../packaging/reproducible/deb.py) stages the assets
and modes declared in Cargo's DEB metadata, keeps the verified ELF unchanged,
and combines explicit runtime requirements with calculated shared-library
requirements. It preserves the maintainer, description and Polkit recommendation.
The MIT license is included as the package copyright file.
No maintainer scripts or active Polkit rules are installed.

[dpkg-deb](https://manpages.debian.org/trixie/dpkg/dpkg-deb.1.en.html)
uses `SOURCE_DATE_EPOCH`, root ownership, xz level 9 and a single compression
thread; staging normalizes all file/directory timestamps and permissions.
[dpkg-shlibdeps](https://manpages.debian.org/bookworm/dpkg-dev/dpkg-shlibdeps.1.en.html)
reads the actual ELF and distribution symbols data: generated dependencies must
be present and are never replaced with a guessed list. This DEB targets Ubuntu
24.04 amd64 and distributions satisfying its declared dependencies; older Debian
versions are not implied compatible.

Both complete DEBs must match before a passing report is written. Each package
is also extracted, its `md5sums` checked, its ELF compared to the verified binary,
and its CLI and schema validated. Control members must consist only of `control`
and `md5sums`. Releases take the DEB exclusively from the successful comparison
job; `make package-deb` remains the ordinary host-dependent development path.

### Local DEB validation (2026-10-18)

Two independent network-disabled GUI builds produced identical complete DEBs,
RPMs and bare binaries for the archived modified worktree. The source SHA-256
remains `516956e1466d3c69b32557e962bed94a2173bb07f27b125a85dec3107035fe9c`.
The DEB SHA-256 is:

```text
5118891634fa2a75c2c3426b0ed982f2037c5f60c74d0e40dfaaa3ce3ed4b879
```

Recipe SHA-256: `a5d040e8f215c7d456a26b7b10f130c2cd4bdf53fc37b9c64e4eee1cebef5af8`.
The receipt, source/recipe archives and both results are in
`target/reproducible-deb-final/`. The pinned snapshot supplies dpkg/dpkg-dev
1.22.6ubuntu6.6 and xz-utils 5.6.1+really5.4.5-1ubuntu0.3.
Both extracted payloads passed checksums, ELF identity, strict schema validation
and CLI startup. The RPM and binary retained their previously recorded hashes.
This is evidence for the modified snapshot, not the unchanged base commit.
GitHub CI/release execution remains pending.
The checked DEB is also in `dist/usbguard-gui_0.1.1-1_amd64.deb`.

Installation in a temporary Ubuntu 24.04 container passed: APT resolved the
runtime dependencies, `dpkg --verify usbguard-gui` reported no file changes,
`usbguard-gui --help` succeeded and `gsettings list-schemas` found
`io.github.onyks_os.UsbguardGui`, verifying the distribution schema trigger.
`libglib2.0-bin` was added as a test tool to supply `gsettings`; it is not a new
application dependency. Log: `target/reproducible-deb-final/ubuntu-install-with-tools.log`.
The first minimal-container probe already passed installation/file/CLI checks,
but lacked the optional `gsettings` command; its log is retained separately.
No host installation or interactive desktop/Polkit test was performed.

### Arch packaging

The pinned Ubuntu snapshot supplies `makepkg`, `pacman`, fakeroot, libarchive
and zstd. [`arch.sh`](../packaging/reproducible/arch.sh) wraps the existing
[`PKGBUILD`](../packaging/arch/PKGBUILD): its original `package()` function stages
the already verified ELF, translations, desktop metadata, icons, schema, license
and inert Polkit example. Runtime and optional dependency declarations are
inherited from the same recipe. Cargo/PKGBUILD version mismatches fail early.
The original PKGBUILD is included in the measured source snapshot; its digest
is embedded in the wrapper whose digest makepkg records in `.BUILDINFO`.

Only packaging runs through makepkg: source download, compilation and Rust tests
are handled separately. `--nodeps` avoids querying Arch dependencies on the Ubuntu
builder; actual dependency resolution is checked in the separate Arch installation
probe. Stripping and debug splitting are disabled to retain the verified ELF.
The ordinary PKGBUILD still supports local native builds and aarch64; the checked
release artifact is specifically x86_64 and need not match a native Arch rebuild.

[makepkg](https://man.archlinux.org/man/makepkg.8.en) receives `SOURCE_DATE_EPOCH`,
a fixed packager, and single-threaded zstd level 19. It generates `.PKGINFO`,
`.BUILDINFO` and the compressed `.MTREE` manifest. Packaging uses a fixed
`/tmp/arch-package` location because makepkg records directories in `.BUILDINFO`;
the two Rust compilations still use different roots and no shared target cache.
The `.BUILDINFO` has no `installed` entries because its pacman database is empty.
The recorded environment is the Ubuntu-based builder, not an Arch build chroot;
the full Ubuntu package inventory and recipe accompany the comparison report.

The complete `.pkg.tar.zst` must match byte for byte, including metadata and
compressed payload. Each package is extracted, its ELF compared with the verified
binary, its schema checked and CLI started. No `.INSTALL` script is permitted.
Release artifacts come only from the successful comparison job. The previous
unfixed Arch release build no longer supplies a competing package.

### Local Arch validation (2026-10-18)

The full comparison passed for two independent offline GUI compilations and
packages. Arch archive SHA-256:

```text
cece846c58e59c3e1794b50dd8659c01839b3c0523d2631ae133bc7bc0cbbee8
```

Source snapshot SHA-256: `5a9a3000c1af8f473781925e629f47aeaa05a2bfaa4b9f3200244ee1ad5127e1`.
It now includes the original Arch PKGBUILD in addition to previous inputs.
Recipe SHA-256: `e611ed02684b4a047ddd50793a26f9048dd7a2640fff078ab67ac5a72851871d`.
The report and both artifacts are in `target/reproducible-arch-verified/`.
Both extracted ELFs retained the previously recorded binary hash and passed
CLI/schema checks; the RPM and DEB also retained their previous hashes.
The pinned snapshot supplied makepkg/pacman 6.0.2-6ubuntu2, fakeroot 1.33-1,
libarchive-tools 3.7.2-2ubuntu0.8 and zstd 1.5.5+dfsg2-2build1.1.
The report explicitly describes the modified worktree snapshot, not the unchanged
base commit. GitHub execution remains pending. The checked release package is
also in `dist/usbguard-gui-0.1.1-1-x86_64.pkg.tar.zst`.

Installation in a temporary updated Arch container passed: pacman resolved runtime
dependencies and installed the exact byte-identical probe package, `pacman -Qkk`
reported 29 total files / 0 altered files, CLI help succeeded and GSettings found
the application schema. Italian translation and inert Polkit example were present.
The minimal base image's `NoExtract` filters were disabled only inside the test
container so documentation and translations could be checked. The initial probe
retains its failure log for that excluded documentation directory.
Successful log: `target/reproducible-arch-probe/arch-install-full.log`.
No host installation or interactive desktop/Polkit test was performed.

### Flatpak packaging and runtime validation

The same pinned Ubuntu builder provides Flatpak, OSTree, PyYAML and SVG icon
validation. [`flatpak.py`](../packaging/reproducible/flatpak.py) reads the existing
manifest's identity, runtime, asset installation commands and permissions.
It skips the manifest's Cargo commands: the fixed compiler and vendored sources
build a separate executable with `USBGUARD_GUI_LOCALEDIR=/app/share/locale`.
Dependency compilation may be reused within one build phase; no compiler cache
is shared between the two phases. The Flatpak executable is compared separately
because its locale prefix intentionally differs from the native executable.
Ordinary SDK/flatpak-builder development builds need not match this release recipe.

Staged timestamps and the OSTree commit timestamp come from `SOURCE_DATE_EPOCH`.
The pinned `libfaketime` library freezes wall-clock time only for `build-bundle`,
because libostree stores a separate delta-generation timestamp in addition to the
commit timestamp. Monotonic clocks remain real. No bundle header is discarded or
edited during comparison; both complete outputs must match. See
[libfaketime's absolute-date mode](https://github.com/wolfcw/libfaketime#usage).
The [Flatpak export/bundle commands](https://docs.flatpak.org/en/latest/flatpak-command-reference.html)
produce a fresh repository and unsigned single-file bundle in each phase.
Comparison includes the complete bundle bytes and the application executable.
Each bundle is also imported into another fresh repository, checked out and
checked for ELF identity; CLI and schema checks run on the extracted content.
The imported commit and metadata are retained with the outputs. Flatpak's icon
validator runs inside the existing offline container with nested sandboxing
disabled; this does not alter the application's runtime permissions.

[`verify-flatpak-runtime.py`](../scripts/verify-flatpak-runtime.py) separately
creates a new private Flatpak installation and HOME/XDG directories under the
chosen output path. It installs the runtime commit in
[`flatpak-runtime.json`](../packaging/reproducible/flatpak-runtime.json), installs
the bundle, starts its CLI and checks schema discovery and installed assets in
the real sandbox. No success report is written on a mismatched commit or failed
check. A trusted local installed runtime repository can supply a temporary
runtime bundle; otherwise the test uses Flathub and explicitly deploys the pinned
commit. Existing personal application installations/remotes are not selected.

```bash
python3 scripts/verify-flatpak-runtime.py \
  target/reproducible/first/usbguard-gui-0.1.1.flatpak \
  --output target/flatpak-runtime-check
```

Runtime lock: `org.gnome.Platform/x86_64/50`, commit
`7e410fa0d429fcc34f2ddd0d9b6224f1435d8640fb9759801db4f02700542353`.
This is the test runtime pin; the distributed bundle refers to GNOME branch 50,
so normal users can receive its runtime security updates. Runtime bytes are not
included in the app bundle and are outside its byte-reproducibility claim.
The release workflow publishes the bundle only from the successful comparison
job, then gates publication on its installed-runtime validation job. GitHub
execution must still be checked separately from local results.

### Local Flatpak validation (2026-10-18)

The complete two-phase comparison passed for all x86_64 artifacts, including the
Flatpak bundle and its dedicated executable. Bundle SHA-256:

```text
8625a89f97d043f66c08360525f74733b5bb3e3f2ae7a4a3b3e1aee62567edc6
```

Flatpak executable SHA-256: `7f02ce726475eda15f4347872b0e8212101cdf1d4d293042fb572a8815c5cadc`.
OSTree app commit: `44b964bed7f751aa2f04525a57dd1679423eb2ecc48658e3fb280f5ee77b7aa3`.
The native executable, RPM, DEB and Arch hashes remain unchanged.
Source snapshot SHA-256: `31d2132c9f114d268e1a442316b8ff1d2faaee7e3164c419bc802d1e7d2f3b73` (now includes the Flatpak manifest).
Recipe SHA-256: `6ea7fa6195dd695c02d0890a240f4dc5facc30f991bbc0cd21b4085813d046eb`.
Receipt, source/recipe archives, both bundles/executables and logs:
`target/reproducible-flatpak-fixed/`. The source is the acknowledged modified
worktree, not the unchanged base commit. The pinned tools were Flatpak
1.14.6-1ubuntu0.1, OSTree 2024.5-1build2 and libfaketime 0.9.10-2.1.

A first failed comparison showed identical executables and OSTree app commits
but different delta-generation timestamps. Freezing only the exporter clock
resolved this; the checker continues to compare every byte. Earlier failed
runs retain their logs and have no passing comparison receipt. The pinned
Flatpak importer logs GLib/OSTree diagnostics for unsigned bundles without a
remote; imports return success and are independently checked by checkout/ELF
identity and the installed-runtime test.

The exact final bundle also passed sandboxed installation tests in private
Flatpak installations, both using an exported local runtime and downloading
from Flathub. Both tests enforced runtime commit
`7e410fa0d429fcc34f2ddd0d9b6224f1435d8640fb9759801db4f02700542353`,
started CLI help, discovered the app's GSettings schema, validated its XML and
found the Italian catalogue and inert Polkit example under `/app`.
Reports/logs: `target/flatpak-runtime-fixed/` and
`target/flatpak-runtime-network/`; their bundle digest matches the complete-build
receipt. A cached temporary runtime bundle avoids repeating the slow local
runtime export when testing again (`--runtime-bundle`).

The deliverable is `dist/usbguard-gui-0.1.1.flatpak`.
Personal application installations/remotes were not changed. Interactive GUI,
tray/portal integration and real host USBGuard/Polkit remain manual checks.
The GitHub workflows have been parsed and checked locally but have not run yet.

### Remote 0.1.2 validation (2026-10-24)

The earlier local reports above describe the archived 0.1.1 development tree.
The clean 0.1.2 commit `d6d984f08e0342f32a4ef7ec79e0debd4111dcc6` passed
[main CI](https://github.com/onyks-os/USBGuardGUI/actions/runs/40095494547) and the
[complete candidate release](https://github.com/onyks-os/USBGuardGUI/actions/runs/40103412714).
The candidate's downloaded checksums, all Sigstore signatures, source/recipe
archives and artifact-to-receipt hashes were independently verified.

The [candidate release](https://github.com/onyks-os/USBGuardGUI/releases/tag/v0.1.2-rc.1)
preserves the signed `reproducible-build.json`, `reproducible-flatpak-runtime.json`,
source/recipe archives, package/toolchain inventories and actual packages.
Its receipt records `modified_worktree: false`, `byte_identical: true` and
`compilation_network: none`. Native binary SHA-256:

```text
91e7f5ad1b159d72ee27e9c203c33bcb2ebe45e5750c7e36427c98b535054273
```

Flatpak bundle SHA-256:

```text
8ae25490dc571803bcb4f641ad92f92151bf4e73da3702538a7c910fa20e9f13
```

The installed-runtime report identifies that exact bundle and GNOME commit
`7e410fa0d429fcc34f2ddd0d9b6224f1435d8640fb9759801db4f02700542353`,
with successful CLI, schema and asset checks. Source and recipe archive digests
are available in the signed receipt. This evidence covers Linux x86_64; it does
not extend the claim to host libraries, runtime updates or interactive Polkit/tray behavior.

The stable `v0.1.2` tag targets the same source commit. Its
[release workflow](https://github.com/onyks-os/USBGuardGUI/actions/runs/40105357687)
repeats these gates before publishing and then runs the separate crates.io job.
Stable signatures use `refs/tags/v0.1.2` in the certificate identity; candidate
signatures use `refs/tags/v0.1.2-rc.1`.

## Post-0.1.2 zbus 6 validation (2026-10-24–25)

The migration candidate was built twice offline with Rust 1.89.0 in the pinned
Ubuntu builder. Native binary, RPM, DEB, Arch package, Flatpak bundle and the
Flatpak executable matched byte-for-byte. The receipt preserves the exact
modified-worktree source/recipe digests; these checkpoint packages still identify
as 0.1.2 and are distinct from the published stable 0.1.2 artifacts.

- [Reproducible-build receipt](validation/zbus6-checkpoint-2026-10-24/reproducible.json)
- [Installed pinned Flatpak runtime receipt](validation/zbus6-checkpoint-2026-10-24/flatpak.json)
- [Dependency audit](validation/zbus6-checkpoint-2026-10-24/audit.json)

The exact native executable exercised in all four real daemon/Polkit guests has
SHA-256 `e6f620ab6756d34849a15357b9967fe1b07541a0b509686f3d930484ff53cf82`.
Receipts record image SHA-256, native package SHA-256, kernel, USBGuard package,
installed executable SHA-256, authorization fixture and each successful case:

- [Ubuntu 24.04.5](validation/zbus6-checkpoint-2026-10-24/ubuntu.json)
- [Debian 13.7](validation/zbus6-clean-install-2026-10-25/debian.json)
- [Fedora 44](validation/zbus6-clean-install-2026-10-25/fedora.json)
- [Arch rolling](validation/zbus6-clean-install-2026-10-25/arch.json)

The desktop-quality candidate has a separate [reproducible build receipt](validation/desktop-quality-2026-10-25/reproducible.json)
and [Fedora 44 guest receipt](validation/desktop-quality-2026-10-25/fedora.json).
That guest run passed the four daemon/Polkit cases plus the new screen-reader
label assertion (5/5), using synthetic USB only. Its installed executable SHA-256
is `71b56efffc9d97c6ad9228d8ae03356453c24a40f604d7e6977d6e27ca9fbbbd`.

All four pass no-agent denial, real-agent cancellation, successful PAM-authenticated
runtime block without persistent-rule changes, and daemon restart/GUI reconnection.
The detailed [test scope and reproduction](testing.md#zbus-6-and-real-clean-system-validation-2026-10-2425)
distinguishes distribution files from the test-only authorization fixture and
virtual devices from physical hardware. Guest credentials, disk images and raw
logs stay under ignored `target/`; they are not release payloads. All VMs are
stopped after collection. Follow-up changes refine tests/tooling/documentation;
the stored local reproduction receipt remains evidence for its recorded snapshot,
while CI runs the reproduction gate again on the PR source.

## Whole-application edge cases (2026-10-25, unreleased)

| Failure / boundary | Expected result | Regression |
| :--- | :--- | :--- |
| Cancel with a full UI event queue | Outcome and invalidation eventually delivered; GTK returns immediately | `tests/unit/dbus_commands.rs` |
| Guided → Text with a valid preview | No abort; identical canonical text; invalid edits cannot submit | `tests/gui/rule_dialog.rs` |
| Parameter signal after IPC/bus denial | Controls remain disabled; unknown values remain unselected | `tests/gui/policy_view.rs` |
| Policy error contains `<` / `&` | Literal daemon text, never interpreted markup | `tests/gui/policy_view.rs` |
| Disable background while tray registration waits | Late handle stopped; no icon resurrected | `tests/gui/window.rs` |
| Authentication remains pending 20/180 seconds | Both actual GTK notices render; no automatic cancellation; Cancel refreshes | `gui_authentication_notices` |
| Malformed D-Bus signals followed by valid traffic | Bad payload ignored; next valid event still delivered | `tests/dbus.rs` |
| Rule set/input exactly at and just over limits | Boundary accepted; excess rejected before mutation | `tests/unit/rules_mod.rs` |
| Recycled notification device ID | Old quick action cannot authorize another identity | `tests/gui/notify.rs` |
| Coverage report omits GUI / rounds below 98% / duplicates files | Gate fails | `tests/test_coverage.py` |

Run `make coverage-app` for the complete measured application; `make verify`
remains the ordinary lint/unit/dependency gate. The GTK tests use a simulated
USBGuard bridge, notifications and watcher on private buses. They do not claim
real PAM authentication or physical hardware coverage; the clean-install evidence
above documents those separate checks.
