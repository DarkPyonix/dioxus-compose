#!/usr/bin/env bash
# Static contract tests for the Linux native-image scripts. These run on macOS too, while
# the build and smoke test themselves require Linux.
set -euo pipefail

scripts_dir="$(cd "$(dirname "$0")/.." && pwd)"
native_dir="$(cd "$scripts_dir/.." && pwd)"

for script in env-linux.sh build-native-linux.sh smoke-test-linux.sh collect-metadata-linux.sh; do
    bash -n "$scripts_dir/$script"
done

cc -c -O2 -o "${TMPDIR:-/tmp}/dioxus-compose-renderer-entry-test.o" \
    "$native_dir/c/renderer_entry.c"
cc -c -O2 -D__linux__ -o "${TMPDIR:-/tmp}/dioxus-compose-smoke-linux-test.o" \
    "$native_dir/c/smoke_host.c"

build="$scripts_dir/build-native-linux.sh"
grep -q 'libawt_xawt\.so' "$build"
grep -q 'libawt_headless\.so' "$build"
grep -q 'libfontmanager\.so' "$build"
grep -q 'libjawt\.so' "$build"
grep -q 'libskiko-linux-' "$build"
grep -q '#ifdef __linux__' "$native_dir/c/smoke_host.c"
grep -q 'WIDGET_TEXT_FIELD' "$native_dir/c/smoke_host.c"

if [[ "$(uname -s)" != "Linux" ]]; then
    if "$build" >"${TMPDIR:-/tmp}/dioxus-compose-linux-test.log" 2>&1; then
        echo "error: Linux build script unexpectedly ran on $(uname -s)" >&2
        exit 1
    fi
    grep -q 'requires Linux' "${TMPDIR:-/tmp}/dioxus-compose-linux-test.log"
fi

echo "linux build script contract: ok"
