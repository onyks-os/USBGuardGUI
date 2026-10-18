#!/usr/bin/env bash
# Trace the GUI against invented devices on a private session bus. No USB policy changes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly REPO_ROOT
TRACE_SECONDS="${TRACE_SECONDS:-60}"
if [[ ! "$TRACE_SECONDS" =~ ^[1-9][0-9]*$ ]]; then
    printf 'TRACE_SECONDS must be a positive integer.\n' >&2
    exit 2
fi
for tool in strace dbus-run-session gdbus timeout; do
    command -v "$tool" >/dev/null || { printf 'Missing tool: %s\n' "$tool" >&2; exit 1; }
done
for binary in "$REPO_ROOT/target/debug/usbguard-gui" "$REPO_ROOT/target/debug/examples/demo-bridge"; do
    [[ -x "$binary" ]] || {
        printf 'First run: cargo build --bin usbguard-gui --example demo-bridge\n' >&2
        exit 1
    }
done
OUTPUT_DIR="${1:-$REPO_ROOT/target/filesystem-trace}"
mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
readonly OUTPUT_DIR
printf 'Tracing for %s seconds into %s (private demo bus, memory settings).\n' "$TRACE_SECONDS" "$OUTPUT_DIR"
# Deliberately use development binaries: only they accept the session-bus override.
# The inner shell expands its own positional parameters and cleanup trap.
# shellcheck disable=SC2016
dbus-run-session -- bash -euo pipefail -c '
    repo_root=$1
    output_dir=$2
    duration=$3
    export USBGUARD_GUI_BUS=session GSETTINGS_BACKEND=memory GTK_A11Y=none
    export USBGUARD_GUI_LOG=debug GSK_RENDERER=cairo
    export XDG_CONFIG_HOME="$output_dir/config" XDG_CACHE_HOME="$output_dir/cache"
    export XDG_DATA_HOME="$output_dir/data"
    "$repo_root/target/debug/examples/demo-bridge" >"$output_dir/bridge.log" 2>&1 &
    bridge_pid=$!
    trap '\''kill "$bridge_pid" 2>/dev/null || true; wait "$bridge_pid" 2>/dev/null || true'\'' EXIT
    gdbus wait --session --timeout 5 org.usbguard1
    status=0
    timeout --signal=TERM --kill-after=5s "${duration}s" \
        strace -f -k --stack-trace-frame-limit=24 -s 256 -e trace=%file -o "$output_dir/gui.trace" \
        "$repo_root/target/debug/usbguard-gui" --background \
        >"$output_dir/gui.stdout" 2>"$output_dir/gui.stderr" || status=$?
    [[ $status == 124 ]] || { printf "GUI trace exited unexpectedly: %s\n" "$status" >&2; exit 1; }
    if ! grep -q "listDevices" "$output_dir/gui.stderr"; then
        printf "The GUI did not reach its device snapshot; increase TRACE_SECONDS.\n" >&2
        exit 1
    fi
    strace -f -k --stack-trace-frame-limit=24 -s 256 -e trace=%file -o "$output_dir/diagnose.trace" \
        "$repo_root/target/debug/usbguard-gui" --diagnose \
        >"$output_dir/diagnose.stdout" 2>"$output_dir/diagnose.stderr"
' usbguard-filesystem-trace "$REPO_ROOT" "$OUTPUT_DIR" "$TRACE_SECONDS"
printf 'Review gui.trace and diagnose.trace, including call stacks; library accesses count separately.\n'
