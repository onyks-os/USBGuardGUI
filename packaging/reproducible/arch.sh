#!/usr/bin/env bash
# Package the independently built ELF with the existing Arch package() function.
set -euo pipefail
umask 022
source_dir=$PWD
arch_work_dir=${ARCH_WORK_DIR:-/tmp/arch-package}
arch_output_dir=${ARCH_OUTPUT_DIR:-/output}
# makepkg records its directories in .BUILDINFO: use a fixed packaging-only
# location. Rust compilation still uses two independent, different source roots.
mkdir -p "${arch_work_dir}/src"
cp "$source_dir/packaging/arch/PKGBUILD" "${arch_work_dir}/OriginalPKGBUILD"
version=$(sed -n 's/^pkgver=//p' "${arch_work_dir}/OriginalPKGBUILD")
manifest_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)
if [[ $version != "$manifest_version" ]]; then
    printf 'Arch and Cargo versions differ: %s != %s\n' "$version" "$manifest_version" >&2
    exit 1
fi
cp -a "$source_dir" "${arch_work_dir}/src/USBGuardGUI-$version"
install -Dm755 "$arch_output_dir/usbguard-gui" "${arch_work_dir}/src/USBGuardGUI-$version/target/release/usbguard-gui"
cat > "${arch_work_dir}/PKGBUILD" <<'PKGBUILD'
source ./OriginalPKGBUILD
# The offline Rust build is performed by worker.sh; only package() runs here.
unset -f prepare build check
source=()
sha256sums=()
arch=('x86_64')
options=('!strip' '!debug' '!lto')
PKGBUILD
printf '# Original PKGBUILD SHA-256: %s\n' "$(sha256sum "$arch_work_dir/OriginalPKGBUILD" | cut -d' ' -f1)" >> "$arch_work_dir/PKGBUILD"
cat "${ARCH_MAKEPKG_CONFIG:-/etc/makepkg.conf}" > "${arch_work_dir}/makepkg.conf"
cat >> "${arch_work_dir}/makepkg.conf" <<'CONFIG'
CARCH=x86_64
CHOST=x86_64-unknown-linux-gnu
PACKAGER='onyks <github.tgwp7@aleeas.com>'
PKGEXT='.pkg.tar.zst'
COMPRESSZST=(zstd -c -T1 -19)
OPTIONS=(strip docs !libtool !staticlibs emptydirs zipman purge !debug !lto)
BUILDENV=(!distcc !color !ccache !check !sign)
CONFIG
printf 'BUILDDIR=%q\nPKGDEST=%q\n' "$arch_work_dir" "$arch_output_dir" >> "$arch_work_dir/makepkg.conf"
cd "$arch_work_dir"
makepkg --config "${arch_work_dir}/makepkg.conf" --noextract --nodeps --noconfirm
bsdtar -tf "$arch_output_dir"/*.pkg.tar.zst > "$arch_output_dir/arch-files.txt"
for metadata in .PKGINFO .BUILDINFO; do
    bsdtar -xOf "$arch_output_dir"/*.pkg.tar.zst "$metadata" > "$arch_output_dir/arch-${metadata#.}.txt"
done
