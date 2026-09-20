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
# aborts the first time an input method touches a text field (SPEC §6). Forcing the whole
# archive in also brings the accessibility entry points (Java_sun_lwawt_macosx_CAccessib*)
# and the Objective-C side that AppKit drives, which NFR-8 needs.
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

# Forcing the archive in is not enough for the accessibility classes (SPEC NFR-8, section 7).
# AppKit never names them: the Objective-C side maps a Java role to a class name and looks the
# class up with NSClassFromString, so nothing in the image refers to GroupAccessibility,
# ButtonAccessibility or the rest by symbol, and the link drops them as dead code. The lookup
# then returns nil, allocating from a nil class gives a nil child, and AppKit aborts the
# process with "object cannot be nil" the moment anything reads the window's children.
#
# Listing the classes by hand would rot, so the list is read back out of the archive that
# defines them and every one is made a root of the link. They are small, and keeping them is
# the whole of the accessibility tree below the window.
a11y_classes=()
while IFS= read -r class_symbol; do
    a11y_classes+=("-H:NativeLinkerOption=-Wl,-u,$class_symbol")
done < <(nm -g "$awt_archive" 2>/dev/null |
    awk '$2 == "S" && $3 ~ /^_OBJC_CLASS_\$_[A-Za-z]+Accessibility$/ { print $3 }' | sort -u)
[[ ${#a11y_classes[@]} -gt 0 ]] || die \
    "no Objective-C accessibility classes found in $awt_archive" \
    "NFR-8 needs them linked in by name; without them VoiceOver aborts the process."
linker_args+=("${a11y_classes[@]}")

# Heap and GC settings for NFR-3 (SPEC 5.2 levers 1 and 2). `-R:` options are baked in as
# the image's runtime defaults. Measure with native/scripts/measure-memory.sh.
#
# Measured 2026-09-20 (M1, smoke test window): pinning the maximum does not move the
# footprint. At the default (80% of RAM), at 64MB and at 24MB the MALLOC_SMALL region is
# 14MB in all three runs, because the Serial GC's adaptive policy already sizes the heap to
# the live set rather than to the maximum. The Java heap is the 2.5MB untagged VM_ALLOCATE
# region, not the 14MB of MALLOC_SMALL, which is Skia's native allocation.
#
# The cap stays because it bounds the worst case rather than the steady state: without it a
# runaway allocation may grow to gigabytes before the collector reacts. 64MB is many times
# the live set, so collections stay in the young generation and do not lengthen frames
# (5.1's budget is 8.33ms). Do not lower it to buy footprint; it does not buy any.
memory_args=("-R:MaxHeapSize=64m"
             "-R:MaxHeapFree=4m"
             "-R:MaximumYoungGenerationSizePercent=25")

# Graphics (SPEC 5.2 lever 3) is deliberately not configured here, and this records why so
# that the next person does not spend another build finding out.
#
# The graphics surfaces are the largest block of the footprint (about 22MB of the measured
# total), and Skiko does expose the two knobs 5.2 asks for: `skiko.buffering=DOUBLE` drops
# the Metal drawable count from three to two, and `skiko.gpu.resourceCacheLimit` caps Skia's
# GPU resource cache. Measured on the JVM (2026-09-20, M1), DOUBLE is worth about 1.9MB:
# IOSurface falls from 9584KB in 9 regions to 7696KB in 7.
#
# They cannot be set from this script. Skiko reads them through System.getProperty at run
# time, and passing `-D` to native-image only sets the property for the build JVM: a shared
# library has no command line, so nothing carries the value into the image. A rebuild with
# `-Dskiko.buffering=DOUBLE` measured byte for byte identical to one without it. This
# GraalVM has no option that bakes a runtime system property either, and
# `--initialize-at-build-time` for SkikoProperties is not a substitute: its static
# initialiser snapshots the whole System.getProperties() table, which would freeze the build
# machine's java.home and user.home into the shipped artifact.
#
# The fix belongs in native/src/RuntimeLayout.kt, whose configureRuntimeLayout already sets
# skiko.library.path and skiko.data.path at run time before Skiko initialises. Adding the
# two properties there (guarded on getProperty being null, so an operator can override) is
# the supported way to get this 1.9MB. That file is owned by another engineer.

# Locale and reachability (SPEC 5.2 levers 4 and 5) are already as small as they can safely
# go. `-H:IncludeLocales=en,ko` is the minimum the product supports and ko is not removable:
# Korean IME is a SPEC 6 requirement. Neither shows up in the footprint anyway. Locale data,
# image code and read-only image heap land in __TEXT and clean __DATA, which the physical
# footprint does not count; only the 7.6MB of dirty __DATA does. Shrinking reachable code
# mostly shrinks the 65MB on disk, not the resident cost.
(cd "$lib" && "$GRAALVM_HOME/bin/native-image" \
    --shared \
    -cp "$classpath" \
    -o "$LIBRARY_NAME" \
    --no-fallback \
    --features=org.thisisthepy.dioxus.compose.nativeimage.ImeReachabilityFeature \
    --features=org.thisisthepy.dioxus.compose.nativeimage.AccessibilityReachabilityFeature \
    -Djava.awt.headless=false \
    -H:IncludeLocales=en,ko \
    -Os \
    -H:+UnlockExperimentalVMOptions \
    "${memory_args[@]}" \
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
