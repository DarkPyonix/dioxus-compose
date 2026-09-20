#!/usr/bin/env bash
# Builds the renderer as a native shared library and stages a self-contained lib/ directory:
#
#   build/native-image/dist/lib/
#     libdioxus_compose_renderer.dylib   the renderer (AWT, Skiko JNI, Compose, our code)
#     libskiko-macos-<arch>.dylib        Skia, loaded by Skiko by path
#     libjawt.dylib                      forwards JAWT_GetAWT into the renderer
#     libawt_lwawt.dylib                 placeholder libawt loads by path
set -euo pipefail
source "$(dirname "$0")/env.sh"

# env.sh validates the platform, the architecture, the Xcode tools and the NIK install.
arch="$HOST_ARCH"
skiko_arch="$SKIKO_ARCH"

for source_file in renderer_entry.c macos_awt_compat.c macos_main_thread.m \
                   jawt_forwarder.c lwawt_placeholder.c; do
    [[ -f "$NATIVE_DIR/c/$source_file" ]] || die "missing $NATIVE_DIR/c/$source_file"
done

# The classpath comes from a short JVM run so that it matches what the metadata describes.
DIOXUS_COMPOSE_AUTOEXIT_MS=1 run_on_jvm ""
classpath="$(cat "$CLASSPATH_FILE")"
obj="$BUILD_DIR/obj"
lib="$DIST_DIR/lib"
rm -rf "$DIST_DIR" "$obj"
mkdir -p "$obj" "$lib"

cc -c -O2 -arch "$arch" -o "$obj/renderer_entry.o" "$NATIVE_DIR/c/renderer_entry.c"
cc -c -O2 -arch "$arch" -o "$obj/macos_awt_compat.o" "$NATIVE_DIR/c/macos_awt_compat.c"
cc -c -O2 -arch "$arch" -o "$obj/macos_main_thread.o" "$NATIVE_DIR/c/macos_main_thread.m"

exported=(dioxus_compose_renderer_run dioxus_compose_renderer_request_frame
          dioxus_compose_jawt_get_awt JNI_OnLoad_osxui)
# The renderer calls the Host's dioxus_compose_host_* functions, which live in the Rust
# executable that loads this library. They are resolved at load time, so the link must
# tolerate them being undefined here (SPEC PR-2).
# The IME entry points (Java_sun_lwawt_macosx_CInputMethod_*) live in objects of the AWT
# toolkit archive that nothing else references, so the linker drops them and the image
# aborts the first time an input method touches a text field (SPEC §6).
awt_archive="$GRAALVM_HOME/lib/static/darwin-$([[ "$arch" == "arm64" ]] && echo aarch64 || echo amd64)/libawt_lwawt.a"
[[ -f "$awt_archive" ]] || { echo "error: missing $awt_archive" >&2; exit 1; }

linker_args=("-H:NativeLinkerOption=-Wl,-undefined,dynamic_lookup"
             "-H:NativeLinkerOption=-Wl,-force_load,$awt_archive"
             "-H:NativeLinkerOption=$obj/renderer_entry.o" "-H:NativeLinkerOption=$obj/macos_awt_compat.o"
             "-H:NativeLinkerOption=$obj/macos_main_thread.o"
             "-H:NativeLinkerOption=-Wl,-install_name,@rpath/$LIBRARY_NAME.dylib")
for symbol in "${exported[@]}"; do
    linker_args+=("-H:NativeLinkerOption=-Wl,-exported_symbol,_$symbol")
done

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
    "${linker_args[@]}")

skiko_jar="$(tr ':' '\n' <<< "$classpath" | grep "skiko-awt-runtime-macos-$skiko_arch" | head -1)"
[[ -n "$skiko_jar" ]] || die \
    "no skiko-awt-runtime-macos-$skiko_arch jar on the runtime classpath" \
    "Skia ships inside that jar and is staged next to the library." \
    "Check $CLASSPATH_FILE and the compose dependency in native/module.yaml."
unzip -q -o -j "$skiko_jar" "libskiko-macos-$skiko_arch.dylib" -d "$lib"
cc -dynamiclib -O2 -arch "$arch" -install_name @rpath/libjawt.dylib \
    -o "$lib/libjawt.dylib" "$NATIVE_DIR/c/jawt_forwarder.c"
cc -dynamiclib -O2 -arch "$arch" -install_name @rpath/libawt_lwawt.dylib \
    -o "$lib/libawt_lwawt.dylib" "$NATIVE_DIR/c/lwawt_placeholder.c"

# native-image leaves headers and build reports next to the library; keep lib/ runtime-only.
mkdir -p "$DIST_DIR/include"
mv "$lib"/*.h "$DIST_DIR/include/" 2>/dev/null || true
rm -f "$lib"/*.md

ls -la "$lib"
