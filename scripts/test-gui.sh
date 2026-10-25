#!/bin/bash
# A private display, D-Bus, configuration and cache for each GTK test process.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$root/target"
test_root=$(mktemp -d "$root/target/gui-test.XXXXXX")
export XDG_CONFIG_HOME="$test_root/config" XDG_DATA_HOME="$test_root/data"
export XDG_CACHE_HOME="$test_root/cache" XDG_RUNTIME_DIR="$test_root/runtime"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
export GSETTINGS_BACKEND=memory GTK_A11Y=none GSK_RENDERER=cairo GDK_BACKEND=x11
export USBGUARD_GUI_BUS=session GIO_USE_VFS=local
export LC_ALL=en_US.UTF-8 LANGUAGE="${LANGUAGE:-en}"
unset WAYLAND_DISPLAY
cat > "$test_root/bus.conf" <<'BUS'
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <!-- No service directories: host portals, keyrings and agents cannot activate. -->
  <policy context="default">
    <allow own="*"/>
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
  </policy>
</busconfig>
BUS
Xvfb -displayfd 3 -nolisten tcp -screen 0 1280x900x24 3>"$test_root/display" >"$test_root/display.log" 2>&1 &
xvfb_pid=$!
cleanup() {
    kill "$xvfb_pid" 2>/dev/null || true
    wait "$xvfb_pid" 2>/dev/null || true
    rm -rf "$test_root"
}
trap cleanup EXIT
for _ in {1..100}; do
    if test -s "$test_root/display"; then break; fi
    if ! kill -0 "$xvfb_pid" 2>/dev/null; then cat "$test_root/display.log" >&2; exit 1; fi
    sleep 0.05
done
if ! test -s "$test_root/display"; then echo "Xvfb did not start" >&2; exit 1; fi
DISPLAY=":$(cat "$test_root/display")"
export DISPLAY
# The child expands bus variables after dbus-run-session has set them.
# shellcheck disable=SC2016
dbus-run-session --config-file "$test_root/bus.conf" -- /bin/bash -c \
    'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"; exec "$@"' bash "$@"
