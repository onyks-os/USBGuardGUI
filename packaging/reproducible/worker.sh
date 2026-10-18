#!/usr/bin/env bash
# Runs only inside the pinned builder. Inputs are read-only; /output is dedicated output.
set -euo pipefail
umask 022

case "${1:-}" in
    prepare)
        mkdir -p /tmp/prepare
        tar -xf /input/source.tar -C /tmp/prepare
        cd /tmp/prepare
        export CARGO_HOME=/tmp/cargo-download
        cargo vendor --locked --versioned-dirs /output/vendor > /output/vendor-config.toml
        dpkg-query -W -f='${Package}\t${Version}\n' | LC_ALL=C sort > /output/packages.txt
        { rustc -Vv; cargo -V; cc --version; ld --version; pkg-config --modversion gtk4 libadwaita-1; } > /output/toolchain.txt
        ;;
    first|second)
        if [[ $1 == first ]]; then
            work_dir=/tmp/repro-one
        else
            work_dir=/tmp/independent/repro-two
        fi
        mkdir -p "$work_dir/source"
        tar -xf /input/source.tar -C "$work_dir/source"
        cd "$work_dir/source"
        export CARGO_HOME="$work_dir/cargo-home" CARGO_TARGET_DIR="$work_dir/target"
        export CARGO_NET_OFFLINE=true
        # Keep the flags byte-identical between builds too: Cargo hashes flags
        # into crate identity. Both possible roots map to the same logical path.
        export RUSTFLAGS='--remap-path-prefix=/tmp/repro-one=/usr/src/build --remap-path-prefix=/tmp/independent/repro-two=/usr/src/build --remap-path-prefix=/dependencies=/usr/src/vendor -C target-cpu=x86-64 -C link-arg=-Wl,--build-id=sha1'
        export CFLAGS='-ffile-prefix-map=/tmp/repro-one=/usr/src/build -ffile-prefix-map=/tmp/independent/repro-two=/usr/src/build -ffile-prefix-map=/dependencies=/usr/src/vendor'
        cargo --config 'source.crates-io.replace-with="vendored-sources"' \
            --config 'source.vendored-sources.directory="/dependencies"' \
            build --frozen --release --target x86_64-unknown-linux-gnu --bin usbguard-gui
        install -m755 "$CARGO_TARGET_DIR/x86_64-unknown-linux-gnu/release/usbguard-gui" /output/usbguard-gui
        /output/usbguard-gui --help > /output/help.txt
        readelf -d /output/usbguard-gui > /output/elf-dynamic.txt
        mkdir -p target/locale/it/LC_MESSAGES
        msgfmt --check po/it.po -o target/locale/it/LC_MESSAGES/usbguard-gui.mo
        python3 /opt/reproducible/rpm.py
        python3 /opt/reproducible/deb.py
        bash /opt/reproducible/arch.sh
        # Reuse dependency compilation within this phase, never between phases.
        # Only the application locale prefix changes for /app in Flatpak.
        USBGUARD_GUI_LOCALEDIR=/app/share/locale cargo \
            --config 'source.crates-io.replace-with="vendored-sources"' \
            --config 'source.vendored-sources.directory="/dependencies"' \
            build --frozen --release --target x86_64-unknown-linux-gnu --bin usbguard-gui
        install -Dm755 "$CARGO_TARGET_DIR/x86_64-unknown-linux-gnu/release/usbguard-gui" /output/flatpak-bin/usbguard-gui
        python3 /opt/reproducible/flatpak.py
        cmp /output/flatpak-bin/usbguard-gui /output/flatpak-checkout/files/bin/usbguard-gui
        glib-compile-schemas --strict --dry-run /output/flatpak-checkout/files/share/glib-2.0/schemas
        /output/flatpak-checkout/files/bin/usbguard-gui --help > /output/flatpak-help.txt
        mkdir -p "$work_dir/extracted"
        cd "$work_dir/extracted"
        rpm2cpio /output/*.rpm | cpio -id --quiet
        cmp usr/bin/usbguard-gui /output/usbguard-gui
        glib-compile-schemas --strict --dry-run usr/share/glib-2.0/schemas
        test ! -s /output/rpm-scripts.txt
        usr/bin/usbguard-gui --help > /output/rpm-help.txt
        mkdir -p "$work_dir/deb-extracted" "$work_dir/deb-control"
        dpkg-deb --extract /output/*.deb "$work_dir/deb-extracted"
        dpkg-deb --control /output/*.deb "$work_dir/deb-control"
        test "$(find "$work_dir/deb-control" -type f | wc -l)" -eq 2
        cd "$work_dir/deb-extracted"
        md5sum --check "$work_dir/deb-control/md5sums"
        cmp usr/bin/usbguard-gui /output/usbguard-gui
        glib-compile-schemas --strict --dry-run usr/share/glib-2.0/schemas
        usr/bin/usbguard-gui --help > /output/deb-help.txt
        mkdir -p "$work_dir/arch-extracted"
        bsdtar -xf /output/*.pkg.tar.zst -C "$work_dir/arch-extracted"
        cd "$work_dir/arch-extracted"
        cmp usr/bin/usbguard-gui /output/usbguard-gui
        glib-compile-schemas --strict --dry-run usr/share/glib-2.0/schemas
        test ! -e .INSTALL
        usr/bin/usbguard-gui --help > /output/arch-help.txt
        ;;
    *) printf 'Expected prepare, first, or second.\n' >&2; exit 2 ;;
esac
