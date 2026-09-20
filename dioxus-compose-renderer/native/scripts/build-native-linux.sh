#!/usr/bin/env bash
# Builds the renderer as a Linux native shared library and stages dist/lib.
#
# UNTESTED ON LINUX as of 2026-09-20. This script encodes the expected upstream GraalVM
# Linux AWT layout, but no Linux native-image build or target run was available here.
# Verify it with the exact commands in linux-build-evidence.md.
set -euo pipefail
source "$(dirname "$0")/env-linux.sh"

[[ -f "$NATIVE_DIR/c/renderer_entry.c" ]] || die "missing $NATIVE_DIR/c/renderer_entry.c"

# Unlike Darwin, upstream GraalVM supports Linux AWT. Its Native Image feature registers
# awt_xawt and writes the dynamic AWT libraries and libjava/libjvm shims beside the image.
# Do not force-load libawt_xawt.a and do not add the macOS placeholder or JNI_OnLoad_osxui.
DIOXUS_COMPOSE_AUTOEXIT_MS=1 run_on_jvm ""
classpath="$(cat "$CLASSPATH_FILE")"
obj="$BUILD_DIR/obj"
lib="$DIST_DIR/lib"
rm -rf "$DIST_DIR" "$obj"
mkdir -p "$obj" "$lib"

cc -c -O2 -fPIC -o "$obj/renderer_entry.o" "$NATIVE_DIR/c/renderer_entry.c"

# The Host's dioxus_compose_host_* functions remain unresolved until the application loads
# the renderer (SPEC PR-2). $ORIGIN lets GraalVM's generated shims and AWT libraries find the
# renderer and one another in the staged lib directory.
(cd "$lib" && "$GRAALVM_HOME/bin/native-image" \
    --shared \
    -cp "$classpath" \
    -o "$LIBRARY_NAME" \
    --no-fallback \
    --features=org.thisisthepy.dioxus.compose.nativeimage.ImeReachabilityFeature \
    -Djava.awt.headless=false \
    -H:IncludeLocales=en,ko \
    -Os \
    -H:+UnlockExperimentalVMOptions \
    "-H:NativeLinkerOption=$obj/renderer_entry.o" \
    '-H:NativeLinkerOption=-Wl,--export-dynamic-symbol=dioxus_compose_renderer_run' \
    '-H:NativeLinkerOption=-Wl,--export-dynamic-symbol=dioxus_compose_renderer_request_frame' \
    '-H:NativeLinkerOption=-Wl,-rpath,$ORIGIN')

# These files are emitted by upstream Native Image when AWT is reachable. Fail here instead
# of shipping an image that later resolves its toolkit against a developer JDK.
for runtime_file in "$LIBRARY_NAME.so" libawt.so libawt_headless.so libawt_xawt.so \
                    libfontmanager.so libjava.so libjvm.so; do
    [[ -f "$lib/$runtime_file" ]] || die "Native Image did not emit $runtime_file" \
        "This upstream GraalVM Linux AWT layout is untested for the selected JDK build." \
        "Keep $BUILD_DIR and report: $GRAALVM_HOME/bin/native-image --version"
done

for exported_symbol in dioxus_compose_renderer_run dioxus_compose_renderer_request_frame; do
    nm -D "$lib/$LIBRARY_NAME.so" | grep -Eq " [TW] ${exported_symbol}$" || die \
        "$LIBRARY_NAME.so does not export $exported_symbol" \
        "Keep $BUILD_DIR and inspect the Native Image linker command."
done

# Skiko loads JAWT from <java.home>/lib. Native Image may emit it when it sees the load. If
# it does not, stage the matching library from the same GraalVM. This fallback is untested.
if [[ ! -f "$lib/libjawt.so" ]]; then
    [[ -f "$GRAALVM_HOME/lib/libjawt.so" ]] || die "libjawt.so was neither emitted nor found in GraalVM" \
        "Looked for $lib/libjawt.so and $GRAALVM_HOME/lib/libjawt.so."
    cp "$GRAALVM_HOME/lib/libjawt.so" "$lib/libjawt.so"
fi

skiko_jar="$(tr ':' '\n' <<< "$classpath" | grep "skiko-awt-runtime-linux-$SKIKO_ARCH" | head -1)"
[[ -n "$skiko_jar" ]] || die "no skiko-awt-runtime-linux-$SKIKO_ARCH jar on the runtime classpath" \
    "Check $CLASSPATH_FILE and the compose dependency in native/module.yaml."
skiko_library="libskiko-linux-$SKIKO_ARCH.so"
unzip -q -o -j "$skiko_jar" "$skiko_library" -d "$lib"
[[ -f "$lib/$skiko_library" ]] || die "$skiko_library was not present in $skiko_jar"

# Font lookup is delegated to the target system's fontconfig. Skia is bundled, fonts and
# fontconfig are not. Confirm that this build host has a Korean-capable fallback, then require
# the target to install equivalent packages such as fontconfig and fonts-noto-cjk.
cjk_font="$(fc-match -f '%{family}\n' 'sans:lang=ko' | head -1)"
[[ -n "$cjk_font" ]] || die "fontconfig found no Korean-capable sans font" \
    "On Ubuntu 24.04: sudo apt-get install fontconfig fonts-noto-cjk"
echo "fontconfig Korean fallback (build host only): $cjk_font"

mkdir -p "$DIST_DIR/include"
mv "$lib"/*.h "$DIST_DIR/include/" 2>/dev/null || true
rm -f "$lib"/*.md

echo "UNTESTED: build artifacts exist, but they have not been executed on Linux."
echo "Verify: DIOXUS_COMPOSE_AUTOEXIT_MS=5000 $NATIVE_DIR/scripts/smoke-test-linux.sh"
ls -la "$lib"
