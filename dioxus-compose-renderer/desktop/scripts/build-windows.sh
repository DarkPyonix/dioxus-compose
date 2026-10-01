#!/usr/bin/env bash
# Builds the Windows renderer as a Kotlin/Native static library, and everything the Host
# links beside it into one MSVC executable:
#
#   build/windows/x64/
#     libdioxus_compose_renderer.a   the renderer (Compose, skiko's Kotlin half, the
#                                    interpreter, our code), MinGW, rewritten for an MSVC link
#     libdioxus_compose_renderer_api.h
#     gcc/                           the GCC runtime the renderer object carries: libstdc++,
#                                    libgcc, libgcc_eh, winpthread (libstdc++ rewritten too)
#     native/dxc-windows-native.lib  MSVC objects: the Win32 window, the MinGW bridge and the
#                                    embedded ICU loader, linked whole
#     skiko/                         skiko's C++ half and the prebuilt Skia libraries, MSVC
#
# Kotlin/Native's only Windows target is MinGW and the application is MSVC. The two halves
# meet in C calls only, and two MinGW conventions are rewritten so an MSVC link keeps their
# meaning: static constructors move to .CRT$XCU, and per-function unwind data becomes an
# associative COMDAT of its function (scripts/fix-mingw-objects.py says why each matters).
#
# Runs on macOS: Kotlin/Native cross-compiles to mingwX64, and the MSVC objects are compiled
# with clang in MSVC mode against the runtime and SDK cargo-xwin lays out.
#
# Needs: scripts/build-skiko-windows.sh and scripts/build-compose.sh --target mingwX64 to
# have published skiko and Compose for mingwX64 to the local Maven repository first.
#
# Usage: build-windows.sh [--release]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPO_DIR="$(cd "$PROJECT_DIR/.." && pwd)"
LIBRARY_NAME="libdioxus_compose_renderer"

die() {
    echo "error: $1" >&2
    shift
    local line
    for line in "$@"; do echo "       $line" >&2; done
    exit 1
}

optimization="-g"
build_type="debug"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) optimization="-opt"; build_type="release"; shift ;;
        -h|--help) sed -n '2,27p' "$0"; exit 0 ;;
        *) die "unknown argument '$1'" "usage: build-windows.sh [--release]" ;;
    esac
done

konan_target="mingw_x64"
amper_platform="mingwX64"

KOTLIN_WRAPPER="$PROJECT_DIR/kotlin"
[[ -x "$KOTLIN_WRAPPER" ]] || die "$KOTLIN_WRAPPER is missing or not executable"

SKIKO_OUT="${DXC_SKIKO_BUILD:-$REPO_DIR/.scratch/skiko-build}/out/windows-x64"
[[ -f "$SKIKO_OUT/skiko-bridges.lib" ]] || die "no skiko C++ half at $SKIKO_OUT" \
    "Run dioxus-compose-renderer/scripts/build-skiko-windows.sh first."

BUILD_DIR="$PROJECT_DIR/build"
OUT_DIR="$BUILD_DIR/windows/x64"
LOG_DIR="$BUILD_DIR/windows-logs"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/gcc" "$OUT_DIR/native" "$OUT_DIR/skiko" "$LOG_DIR"

# 1. The module, with the debug log that names every klib the compile resolved. The link step
# has to be handed exactly what the compile used, not a list kept by hand.
build_log="$LOG_DIR/$amper_platform-build.log"
echo "==> kotlin build -m windows -m staticlib-windows ($amper_platform)"
rm -rf "$PROJECT_DIR/build/tasks/_windows_compile${amper_platform}Debug"
(cd "$PROJECT_DIR" && "$KOTLIN_WRAPPER" --log-level=debug build -m windows -m staticlib-windows -p "$amper_platform") \
    >"$build_log" 2>&1 || { tail -40 "$build_log" >&2; die "the windows module did not compile" "Full log: $build_log"; }

klib="$PROJECT_DIR/build/tasks/_windows_compile${amper_platform}Debug/windows.klib"
[[ -f "$klib" ]] || die "the compiler produced no klib at $klib" "Full log: $build_log"

libraries_file="$LOG_DIR/$amper_platform-libraries.txt"
awk -v target="-target=$konan_target" '
    /^[A-Z]+ / {
        if (wanted) { for (i = 1; i <= count; i++) print libraries[i] }
        wanted = 0; count = 0
        collecting = /Native metadata compilation args/
        next
    }
    collecting && $0 == target { wanted = 1 }
    collecting && /^-library=/ { libraries[++count] = substr($0, 10) }
    END { if (wanted) { for (i = 1; i <= count; i++) print libraries[i] } }
' "$build_log" | grep -v "/build/tasks/" | sort -u > "$libraries_file"
[[ -s "$libraries_file" ]] || die "could not read the resolved klib list for $konan_target out of $build_log" \
    "The Kotlin Toolchain changed its debug output; update the awk block in this script."

konan_home=""
for candidate in "$HOME"/Library/Caches/JetBrains/Kotlin/extract.cache/*kotlin-native-prebuilt-*-macos-aarch64*.d \
                 "$HOME"/.cache/JetBrains/Kotlin/extract.cache/*kotlin-native-prebuilt-*-linux-x86_64*.d; do
    [[ -x "$candidate/bin/konanc" ]] && konan_home="$candidate"
done
[[ -n "$konan_home" ]] || die "no Kotlin/Native compiler in the toolchain cache" \
    "It is unpacked by the first 'kotlin build' of a native module."

# 2. The static library. The entry points are the main module and the renderer a library, so
# the generated header holds the two boundary functions and nothing else.
output="$OUT_DIR/$LIBRARY_NAME"
entry_source="$PROJECT_DIR/staticlib-windows/src/WindowsEntryPoints.kt"
[[ -f "$entry_source" ]] || die "missing $entry_source"
echo "==> konanc -produce static ($konan_target, $build_type)"
library_args=("-library=$klib")
while IFS= read -r line; do library_args+=("-library=$line"); done < "$libraries_file"
"$konan_home/bin/konanc" \
    -produce static \
    -target "$konan_target" \
    "$optimization" \
    -module-name dioxus_compose_renderer \
    -opt-in kotlin.experimental.ExperimentalNativeApi \
    "${library_args[@]}" \
    "$entry_source" \
    -o "$output" >"$LOG_DIR/$amper_platform-link.log" 2>&1 ||
    { tail -40 "$LOG_DIR/$amper_platform-link.log" >&2; die "konanc did not produce the static library"; }
archive="$output.a"
[[ -f "$archive" ]] || die "konanc produced no $archive" "Full log: $LOG_DIR/$amper_platform-link.log"
for generated in "$OUT_DIR"/*.h; do
    [[ -f "$generated" && "$generated" != "$OUT_DIR/${LIBRARY_NAME}_api.h" ]] && mv "$generated" "$OUT_DIR/${LIBRARY_NAME}_api.h"
done

# 3. The GCC runtime the renderer object was compiled against, from Kotlin/Native's own MinGW
# toolchain: the same files Kotlin/Native links statically into a MinGW executable of its own.
mingw=""
for candidate in "$HOME"/.konan/dependencies/msys2-mingw-w64-x86_64-*; do
    [[ -f "$candidate/x86_64-w64-mingw32/lib/libwinpthread.a" ]] && mingw="$candidate"
done
[[ -n "$mingw" ]] || die "no Kotlin/Native MinGW toolchain under ~/.konan/dependencies" \
    "Kotlin/Native downloads it the first time it compiles for mingwX64."
gcc_lib="$(ls -d "$mingw"/lib/gcc/x86_64-w64-mingw32/*/ | sort -V | tail -1)"
cp "$gcc_lib/libstdc++.a" "$gcc_lib/libgcc.a" "$gcc_lib/libgcc_eh.a" "$OUT_DIR/gcc/"
cp "$mingw/x86_64-w64-mingw32/lib/libwinpthread.a" "$OUT_DIR/gcc/"

# 4. The rewrite. libstdc++ has constructors of its own (its exception emergency pool among
# them), so it is rewritten with the renderer.
python3 "$PROJECT_DIR/scripts/fix-mingw-objects.py" "$archive"
python3 "$PROJECT_DIR/scripts/fix-mingw-objects.py" "$OUT_DIR/gcc/libstdc++.a"

# 5. The MSVC objects of our own: the window, and the two from the skiko build.
llvm_bin=""
for candidate in $(ls -d "$HOME"/.konan/dependencies/llvm-*-essentials*/bin 2>/dev/null | sort -V -r); do
    [[ -x "$candidate/clang" ]] && { llvm_bin="$candidate"; break; }
done
[[ -n "$llvm_bin" ]] || die "no Kotlin/Native LLVM under ~/.konan/dependencies"
xwin="${XWIN_DIR:-}"
if [[ -z "$xwin" ]]; then
    for candidate in "$HOME/Library/Caches/cargo-xwin/xwin" "$HOME/.cache/cargo-xwin/xwin"; do
        [[ -d "$candidate/crt/include" ]] && { xwin="$candidate"; break; }
    done
fi
[[ -n "$xwin" ]] || die "no MSVC runtime and Windows SDK headers found" \
    "Run 'cargo xwin build --target x86_64-pc-windows-msvc' once, or set XWIN_DIR."
"$llvm_bin/clang" --driver-mode=cl --target=x86_64-pc-windows-msvc /O2 /MT /c /nologo \
    -imsvc "$xwin/crt/include" -imsvc "$xwin/sdk/include/ucrt" \
    -imsvc "$xwin/sdk/include/um" -imsvc "$xwin/sdk/include/shared" \
    "/I$PROJECT_DIR/desktop/c" \
    "$PROJECT_DIR/desktop/c/win32_window.c" "/Fo$OUT_DIR/native/win32_window.obj" ||
    die "desktop/c/win32_window.c did not compile in MSVC mode"
cp "$SKIKO_OUT/mingw_bridge.obj" "$SKIKO_OUT/embedded_icu.obj" "$OUT_DIR/native/"
# One library holding the three, which the Host links whole. An object named on a link line
# would do, but a build script's link arguments stop at its own package, and a library it
# names does not: it travels in the rlib to every application. Whole, because nothing calls
# into the ICU loader or the initialiser the bridge registers, and a member nothing calls is
# a member the linker leaves out.
"$llvm_bin/llvm-ar" rcs "$OUT_DIR/native/dxc-windows-native.lib" "$OUT_DIR"/native/*.obj
cp "$SKIKO_OUT/skiko-bridges.lib" "$OUT_DIR/skiko/"
cp "$SKIKO_OUT"/skia/*.lib "$OUT_DIR/skiko/"

# 6. A static library missing an entry point links fine and fails at run time, so the two
# boundary symbols are checked here rather than in the application that links it.
symbols_file="$LOG_DIR/$amper_platform-symbols.txt"
nm -g "$archive" >"$symbols_file" 2>/dev/null || true
for symbol in dioxus_compose_renderer_run dioxus_compose_renderer_request_frame; do
    grep -q " T $symbol\$" "$symbols_file" ||
        die "$archive does not export $symbol" \
            "Check the @CName annotations in staticlib/src/IosEntryPoints.kt."
done

echo
echo "$OUT_DIR"
du -sh "$OUT_DIR"/* | sed 's|'"$OUT_DIR"'/||'
