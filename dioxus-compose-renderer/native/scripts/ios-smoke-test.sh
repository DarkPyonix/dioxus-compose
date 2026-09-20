#!/usr/bin/env bash
# Links the same minimal C host the desktop smoke test uses against the iOS static library,
# wraps it in an app bundle, and runs it on the simulator (after build-ios.sh).
#
# The host source is shared with the desktop test on purpose: `native/c/smoke_host.c` is the
# stand-in for the Rust Host, it implements the five `dioxus_compose_host_*` functions and
# calls `dioxus_compose_renderer_run` from `main`, and none of that changes on iOS. If the
# same file draws the same tree on both platforms, the C ABI really is the same (SPEC PR-2).
#
# Usage: ios-smoke-test.sh [--device-name "iPhone 16"] [--screenshot <path>]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NATIVE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_DIR="$(cd "$NATIVE_DIR/.." && pwd)"
BUNDLE_ID="org.thisisthepy.dioxus.compose.smoke"

die() {
    echo "error: $1" >&2
    shift
    local line
    for line in "$@"; do echo "       $line" >&2; done
    exit 1
}

device_name="iPhone 16"
screenshot=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --device-name) device_name="${2:-}"; shift 2 ;;
        --screenshot) screenshot="${2:-}"; shift 2 ;;
        -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
        *) die "unknown argument '$1'" ;;
    esac
done

archive="$PROJECT_DIR/build/ios/simulator/libdioxus_compose_renderer.a"
[[ -f "$archive" ]] || die "$archive not found" \
    "fix: run $SCRIPT_DIR/build-ios.sh first"
[[ -f "$NATIVE_DIR/c/smoke_host.c" ]] || die "missing $NATIVE_DIR/c/smoke_host.c"
command -v xcrun >/dev/null 2>&1 || die "xcrun not found" "fix: xcode-select --install"

sdk_path="$(xcrun --sdk iphonesimulator --show-sdk-path)" ||
    die "the iphonesimulator SDK is not installed" \
        "fix: install Xcode, then xcodebuild -downloadPlatform iOS"

app="$PROJECT_DIR/build/ios/DioxusComposeSmoke.app"
rm -rf "$app"
mkdir -p "$app"

echo "==> linking the smoke host against the renderer"
# -export_dynamic keeps the host's five functions in the executable's dynamic symbol table.
# The renderer resolves them with dlsym at run time (see IosHostConnection.kt), and without
# this the linker has no reason to list functions that nothing in the image references.
xcrun clang \
    -target arm64-apple-ios15.0-simulator \
    -isysroot "$sdk_path" \
    -O2 \
    -o "$app/DioxusComposeSmoke" \
    "$NATIVE_DIR/c/smoke_host.c" \
    "$archive" \
    -Wl,-export_dynamic \
    -lc++ -lsqlite3 -lz \
    -framework UIKit -framework Foundation -framework CoreGraphics -framework CoreText \
    -framework CoreFoundation -framework QuartzCore -framework Metal -framework MetalKit \
    -framework CoreServices -framework ImageIO -framework UniformTypeIdentifiers \
    -framework AudioToolbox -framework AVFoundation -framework CoreMedia -framework Security

cat > "$app/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>DioxusComposeSmoke</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleName</key><string>DioxusComposeSmoke</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSRequiresIPhoneOS</key><true/>
    <key>MinimumOSVersion</key><string>15.0</string>
    <key>UILaunchScreen</key><dict/>
    <!-- Compose for iOS refuses to start without it: it caps the frame rate at 60Hz. -->
    <key>CADisableMinimumFrameDurationOnPhone</key><true/>
    <key>UIDeviceFamily</key><array><integer>1</integer></array>
</dict>
</plist>
PLIST

device="$(xcrun simctl list devices available | awk -v name="$device_name" '
    $0 ~ "^    " name " \\(" { match($0, /\(([-0-9A-F]+)\)/, m); print m[1]; exit }
' 2>/dev/null || true)"
if [[ -z "$device" ]]; then
    device="$(xcrun simctl list devices available | grep -F "$device_name (" | head -1 |
        sed -E 's/.*\(([-0-9A-F]{36})\).*/\1/')"
fi
[[ -n "$device" ]] || die "no available simulator called '$device_name'" \
    "Pick one from: xcrun simctl list devices available" \
    "then pass it with --device-name."

echo "==> booting $device_name ($device)"
xcrun simctl boot "$device" 2>/dev/null || true
xcrun simctl bootstatus "$device" -b >/dev/null

echo "==> installing and launching"
xcrun simctl uninstall "$device" "$BUNDLE_ID" >/dev/null 2>&1 || true
xcrun simctl install "$device" "$app"
log="$PROJECT_DIR/build/ios/smoke-console.log"
xcrun simctl launch --console-pty --terminate-running-process "$device" "$BUNDLE_ID" \
    >"$log" 2>&1 &
launch_pid=$!

# The renderer never returns from run() on iOS (UIApplicationMain owns the process), so the
# test waits for the window to come up rather than for the process to exit.
for _ in $(seq 1 30); do
    grep -q "dioxus_compose_host_init" "$log" 2>/dev/null && break
    sleep 1
done
sleep 3

if [[ -n "$screenshot" ]]; then
    xcrun simctl io "$device" screenshot "$screenshot"
    echo "screenshot: $screenshot"
fi

kill "$launch_pid" 2>/dev/null || true
xcrun simctl terminate "$device" "$BUNDLE_ID" >/dev/null 2>&1 || true

echo
cat "$log"
grep -q "dioxus_compose_host_init" "$log" || die \
    "the renderer never called dioxus_compose_host_init" \
    "Console log: $log"
echo
echo "ok: the renderer ran on the simulator and called back into the host (SPEC PR-2)"
