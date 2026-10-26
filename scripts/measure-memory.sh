#!/usr/bin/env bash
# Repeatable background memory check against an invented USBGuard daemon.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
duration=${MEMORY_SECONDS:-600}
device_count=${USBGUARD_GUI_DEMO_DEVICE_COUNT:-20}
renderer=${MEMORY_RENDERER:-cairo}
max_pss_kb=${MAX_PSS_KB:-71680}
max_rss_kb=${MAX_CAIRO_RSS_KB:-112640}

if [[ ! $duration =~ ^[0-9]+$ || $duration -lt 1 ]]; then
    printf 'MEMORY_SECONDS must be a positive integer\n' >&2
    exit 2
fi
if [[ ! $device_count =~ ^[0-9]+$ || $device_count -lt 7 || $device_count -gt 200 ]]; then
    printf 'USBGUARD_GUI_DEMO_DEVICE_COUNT must be between 7 and 200\n' >&2
    exit 2
fi
if [[ -z ${DISPLAY:-} && -z ${WAYLAND_DISPLAY:-} ]]; then
    printf 'A graphical session is required (DISPLAY or WAYLAND_DISPLAY)\n' >&2
    exit 2
fi
for tool in cargo dbus-run-session gdbus; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'Required tool not found: %s\n' "$tool" >&2
        exit 2
    fi
done

cargo build --locked --bin usbguard-gui --example demo-bridge

export USBGUARD_GUI_BUS=session
export USBGUARD_GUI_ISOLATED_INSTANCE=1
export USBGUARD_GUI_DEMO_DEVICE_COUNT=$device_count
export REPO_ROOT=$repo_root
export MEMORY_SECONDS=$duration
export MEMORY_RENDERER=$renderer
export MAX_PSS_KB=$max_pss_kb
export MAX_CAIRO_RSS_KB=$max_rss_kb
export GSETTINGS_BACKEND=memory
export GSK_RENDERER=$renderer
export GIO_USE_VFS=local

test_root=$(mktemp -d "$repo_root/target/memory-check.XXXXXX")
trap 'rm -rf "$test_root"' EXIT
cat >"$test_root/bus.conf" <<'BUS'
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <!-- Keep portals, keyrings and agents on the user's real session bus. -->
  <policy context="default">
    <allow own="*"/>
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
  </policy>
</busconfig>
BUS

# The inner script intentionally expands variables in the child shell.
# shellcheck disable=SC2016
dbus-run-session --config-file "$test_root/bus.conf" -- bash -c '
set -euo pipefail
demo_pid=
gui_pid=
cleanup() {
    [[ -z $gui_pid ]] || kill "$gui_pid" 2>/dev/null || true
    [[ -z $demo_pid ]] || kill "$demo_pid" 2>/dev/null || true
    [[ -z $gui_pid ]] || wait "$gui_pid" 2>/dev/null || true
    [[ -z $demo_pid ]] || wait "$demo_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

"$REPO_ROOT/target/debug/examples/demo-bridge" >/dev/null 2>&1 &
demo_pid=$!
"$REPO_ROOT/target/debug/usbguard-gui" --background >/dev/null 2>&1 &
gui_pid=$!

# Allow the demo bridge to register and the GUI to complete its initial snapshot.
ready=false
for _ in $(seq 1 30); do
    if ! kill -0 "$demo_pid" 2>/dev/null || ! kill -0 "$gui_pid" 2>/dev/null; then
        printf "Demo bridge or GUI exited during startup\n" >&2
        exit 1
    fi
    if gdbus call --session --dest org.usbguard1 --object-path /org/usbguard1 \
        --method org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1; then
        ready=true
        break
    fi
    sleep 1
done
if [[ $ready != true ]]; then
    printf "Demo bridge did not register on the private session bus\n" >&2
    exit 1
fi
sleep 5
remaining=$MEMORY_SECONDS
printf "Measuring idle memory for %s seconds with %s demo devices...\n" \
    "$MEMORY_SECONDS" "$USBGUARD_GUI_DEMO_DEVICE_COUNT"
while (( remaining > 0 )); do
    interval=60
    (( remaining < interval )) && interval=$remaining
    sleep "$interval"
    remaining=$((remaining - interval))
    printf "Memory measurement: %s seconds remaining\n" "$remaining"
done

rollup="/proc/$gui_pid/smaps_rollup"
if [[ ! -r $rollup ]]; then
    printf "GUI process exited before the measurement\n" >&2
    exit 1
fi
read -r rss_kb pss_kb < <(awk "/^(Rss|Pss):/ { if (\$1 == \"Rss:\") rss=\$2; else pss=\$2 } END { print rss, pss }" "$rollup")
printf "renderer=%s devices=%s idle_seconds=%s RSS=%s KiB PSS=%s KiB\n" \
    "$MEMORY_RENDERER" "$USBGUARD_GUI_DEMO_DEVICE_COUNT" "$MEMORY_SECONDS" "$rss_kb" "$pss_kb"

if (( pss_kb > MAX_PSS_KB )); then
    printf "PSS exceeded the %s KiB budget\n" "$MAX_PSS_KB" >&2
    exit 1
fi
if [[ $MEMORY_RENDERER == cairo ]] && (( rss_kb > MAX_CAIRO_RSS_KB )); then
    printf "Cairo RSS exceeded the %s KiB budget\n" "$MAX_CAIRO_RSS_KB" >&2
    exit 1
fi
'
