#!/usr/bin/env bash
# Builds skiko for the Windows renderer: the Kotlin half for Kotlin/Native and the C++ half
# for MSVC, which are two different builds because the two halves have to match two
# different ABIs.
#
# skiko publishes no Kotlin/Native target for Windows. `patches/skiko/` adds one that
# compiles the Kotlin half only, and this publishes it to the local Maven repository as
# `org.jetbrains.skiko:skiko-mingwx64` under the pinned version, where the Compose build
# finds it. The C++ half, the bridges skiko's Kotlin calls by name, is compiled here in
# MSVC mode against JetBrains' prebuilt Windows Skia, with the same defines skiko's own
# Windows build uses: a class laid out differently on the two sides of a call is a crash
# nobody can trace from either side.
#
# Usage: build-skiko-windows.sh [--clean]
#
# The work directory is .scratch/skiko-build in this repository. Set DXC_SKIKO_BUILD to put
# it elsewhere, and only somewhere the owner has agreed to.
#
# Output, in <work>/out/windows-x64/:
#   skiko-bridges.lib     skiko's C++ half, MSVC, static runtime
#   mingw_bridge.obj      what the MinGW object needs from MinGW's runtime, answered by MSVC's
#   embedded_icu.obj      Skia's ICU loader, with the ICU data compiled into it
#   skia/                 the prebuilt Skia libraries the bridges link against
#
# And in the local Maven repository: skiko-mingwx64, and skiko's root metadata extended to
# name it (extend-skiko-root.py says why).
#
# Needs: git, unzip, a JDK 17 (JAVA_HOME), Kotlin/Native's LLVM (in ~/.konan after any
# Kotlin/Native build), and the MSVC runtime and Windows SDK as cargo-xwin lays them out
# (run `cargo xwin build --target x86_64-pc-windows-msvc` once in any crate).
set -euo pipefail

UPSTREAM="https://github.com/JetBrains/skiko.git"
REVISION="9a5b398bb2044fff7e7a84fbfd6f4b803e4427c0"
PUBLISHED_AS="0.144.6"
SKIA_RELEASE="m144-22f58c9fd4"
SKIA_URL="https://github.com/JetBrains/skia/releases/download/$SKIA_RELEASE/Skia-$SKIA_RELEASE-windows-Release-x64.zip"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RENDERER_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(cd "$RENDERER_DIR/.." && pwd)"
PATCH_DIR="$RENDERER_DIR/patches/skiko"
NATIVE_DIR="$RENDERER_DIR/windows/native"
WORK="${DXC_SKIKO_BUILD:-$REPO_DIR/.scratch/skiko-build}"

die() {
    echo "error: $1" >&2
    shift
    for line in "$@"; do echo "       $line" >&2; done
    exit 1
}

clean=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --clean) clean=1; shift ;;
        -h|--help) sed -n '2,27p' "$0"; exit 0 ;;
        *) die "unknown argument '$1'" "usage: build-skiko-windows.sh [--clean]" ;;
    esac
done

# The tools. Each is looked for where its own installer puts it, and a missing one is
# named with the step that provides it rather than left to fail inside a compiler.
[[ -n "${JAVA_HOME:-}" && -x "$JAVA_HOME/bin/java" ]] ||
    die "JAVA_HOME does not name a JDK" "skiko's Gradle build needs a JDK 17; point JAVA_HOME at one."

llvm_bin=""
for candidate in $(ls -d "$HOME"/.konan/dependencies/llvm-*-essentials*/bin 2>/dev/null | sort -V -r); do
    if [[ -x "$candidate/clang" && -x "$candidate/llvm-ar" ]]; then llvm_bin="$candidate"; break; fi
done
[[ -n "$llvm_bin" ]] ||
    die "no Kotlin/Native LLVM with clang and llvm-ar under ~/.konan/dependencies" \
        "Build any Kotlin/Native target once and Kotlin/Native downloads it."
CLANG="$llvm_bin/clang"
LLVM_AR="$llvm_bin/llvm-ar"

xwin="${XWIN_DIR:-}"
if [[ -z "$xwin" ]]; then
    for candidate in "$HOME/Library/Caches/cargo-xwin/xwin" "$HOME/.cache/cargo-xwin/xwin"; do
        [[ -d "$candidate/crt/include" ]] && { xwin="$candidate"; break; }
    done
fi
[[ -n "$xwin" && -d "$xwin/crt/include" && -d "$xwin/sdk/include/ucrt" ]] ||
    die "no MSVC runtime and Windows SDK headers found" \
        "Run 'cargo xwin build --target x86_64-pc-windows-msvc' once, or set XWIN_DIR."

[[ $clean -eq 1 ]] && rm -rf "$WORK"
mkdir -p "$WORK"

# 1. skiko at the pin, patched. The checkout is nobody's to edit: it is reset every run.
checkout="$WORK/skiko"
if [[ ! -d "$checkout/.git" ]]; then
    git clone --quiet --no-checkout "$UPSTREAM" "$checkout"
fi
git -C "$checkout" cat-file -e "$REVISION^{commit}" 2>/dev/null || git -C "$checkout" fetch --quiet origin "$REVISION"
git -C "$checkout" -c advice.detachedHead=false checkout --quiet --force "$REVISION"
git -C "$checkout" clean --quiet -fdx -e skiko/build -e skiko/dependencies -e .gradle
for patch in "$PATCH_DIR"/*.patch; do
    git -C "$checkout" apply --whitespace=nowarn "$patch" ||
        die "$(basename "$patch") does not apply to skiko $REVISION" \
            "If upstream moved, read what refused to apply before changing the pin."
done

# 2. The Kotlin half, published where the Compose build looks for it. deploy.release keeps
# skiko from appending -SNAPSHOT to the version, which nothing would then ask for. Only the mingwX64
# publication: publishing the root module under the same version would replace JetBrains'
# metadata for every other target with one that knows only this one.
(
    cd "$checkout/skiko"
    ./gradlew --no-daemon --quiet publishMingwX64PublicationToMavenLocal \
        -Pskiko.native.mingw.enabled=true -Pskiko.awt.enabled=false \
        -Pdeploy.version="$PUBLISHED_AS" -Pdeploy.release=true
)

# 2b. The root module, which is what Compose actually depends on. Gradle resolves
# `org.jetbrains.skiko:skiko` through its metadata, which lists one variant per target and
# says which module carries it, and JetBrains' lists no mingw_x64, so a Compose module
# built for Windows is told skiko does not support it. JetBrains' root is taken as it is
# published and the two mingw_x64 variants are added, written after the linux_x64 ones
# with the target and module renamed. Every other variant is untouched, so a build for any
# other target that finds this copy first resolves exactly what it would have without it.
root="$HOME/.m2/repository/org/jetbrains/skiko/skiko/$PUBLISHED_AS"
central="https://repo1.maven.org/maven2/org/jetbrains/skiko/skiko/$PUBLISHED_AS"
mkdir -p "$root"
for suffix in .module .pom .jar -sources.jar -kotlin-tooling-metadata.json; do
    file="skiko-$PUBLISHED_AS$suffix"
    curl -fsSL -o "$root/$file.download" "$central/$file" || die "could not download $central/$file"
    mv "$root/$file.download" "$root/$file"
done
python3 "$SCRIPT_DIR/extend-skiko-root.py" "$root/skiko-$PUBLISHED_AS.module" "$PUBLISHED_AS"

# 3. Skia for Windows, as JetBrains builds it: MSVC, static runtime.
skia="$WORK/skia-$SKIA_RELEASE-windows-x64"
if [[ ! -f "$skia/out/Release-windows-x64/skia.lib" ]]; then
    rm -rf "$skia" && mkdir -p "$skia"
    curl -fsSL -o "$skia.zip" "$SKIA_URL" || die "could not download $SKIA_URL"
    unzip -q -o "$skia.zip" -d "$skia"
    rm -f "$skia.zip"
fi
skia_out="$skia/out/Release-windows-x64"

# 4. The C++ half. The defines are skiko's own for Windows (CommonTasksConfiguration.kt in
# the skiko checkout), and the native-target flags (no RTTI, no exceptions) in MSVC spelling.
out="$WORK/out/windows-x64"
obj="$WORK/obj"
rm -rf "$out" "$obj"
mkdir -p "$out" "$obj"
src="$checkout/skiko/src"
flags=(
    --driver-mode=cl --target=x86_64-pc-windows-msvc /std:c++17 /O2 /MT /GR- /c /nologo
    -Wno-everything
    -imsvc "$xwin/crt/include" -imsvc "$xwin/sdk/include/ucrt"
    -imsvc "$xwin/sdk/include/um" -imsvc "$xwin/sdk/include/shared"
    /DSK_ALLOW_STATIC_GLOBAL_INITIALIZERS=1 /DSK_FORCE_DISTANCE_FIELD_TEXT=0 /DSK_GAMMA_APPLY_TO_A8
    /DSK_GAMMA_SRGB /DSK_SCALAR_TO_FLOAT_EXCLUDED /DSK_SUPPORT_GPU=1 /DSK_GANESH /DSK_GL
    /DSK_SHAPER_HARFBUZZ_AVAILABLE /DSK_UNICODE_AVAILABLE /DSK_SHAPER_UNICODE_AVAILABLE
    /DSK_SUPPORT_OPENCL=0 /DSK_USING_THIRD_PARTY_ICU
    /DU_DISABLE_RENAMING=0 /DU_DISABLE_VERSION_SUFFIX=1 /DU_HAVE_LIB_SUFFIX=1 /DU_LIB_SUFFIX_C_NAME=_skiko
    /USK_HIDE_PATH_EDIT_METHODS
    /DSK_BUILD_FOR_WIN /D_CRT_SECURE_NO_WARNINGS /D_HAS_EXCEPTIONS=0 /DWIN32_LEAN_AND_MEAN /DNOMINMAX
    /DSK_DIRECT3D /DSK_ANGLE /DSK_RELEASE
)
for dir in "" include include/core include/gpu include/effects include/pathops include/utils \
           include/codec include/svg modules/jsonreader modules/skottie/include \
           modules/skparagraph/include modules/skshaper/include modules/skunicode/include \
           modules/sksg/include modules/svg/include third_party/externals/harfbuzz/src \
           third_party/icu third_party/externals/icu/source/common; do
    flags+=("/I$skia/$dir")
done
flags+=("/I$src/nativeJsMain/cpp" "/I$src/commonMain/cpp/common/include" "/I$src/commonMain/cpp/common")

compile() {
    local file="$1"
    local name
    name="$(echo "${file#"$src"/}" | tr '/' '_')"
    "$CLANG" "${flags[@]}" "$file" "/Fo$obj/${name%.*}.obj" ||
        die "skiko's $file did not compile in MSVC mode"
}
export -f compile die
export CLANG obj src
export flags_file="$obj/flags"
count=0
while IFS= read -r -d '' file; do
    compile "$file" &
    count=$((count + 1))
    # Four at a time: enough to be quick, few enough to leave the machine usable.
    (( count % 4 == 0 )) && wait
done < <(find "$src/commonMain/cpp/common" "$src/nativeJsMain/cpp" -type f \( -name '*.cc' -o -name '*.cpp' \) -print0)
wait
built=$(ls "$obj"/*.obj | wc -l | tr -d ' ')
[[ "$built" -eq "$count" ]] || die "compiled $built of skiko's $count C++ files"
"$LLVM_AR" rcs "$out/skiko-bridges.lib" "$obj"/*.obj

# 5. The two objects of our own that sit beside them.
c_flags=(--driver-mode=cl --target=x86_64-pc-windows-msvc /O2 /MT /c /nologo
    -imsvc "$xwin/crt/include" -imsvc "$xwin/sdk/include/ucrt"
    -imsvc "$xwin/sdk/include/um" -imsvc "$xwin/sdk/include/shared")
"$CLANG" "${c_flags[@]}" "$NATIVE_DIR/mingw_bridge.c" "/Fo$out/mingw_bridge.obj"
"$CLANG" "${c_flags[@]}" /std:c++17 /GR- -Wno-c23-extensions "/clang:--embed-dir=$skia_out" \
    "$NATIVE_DIR/embedded_icu.cpp" "/Fo$out/embedded_icu.obj"

rm -rf "$out/skia"
mkdir -p "$out/skia"
cp "$skia_out"/*.lib "$out/skia/"

echo "skiko $PUBLISHED_AS for mingwX64 published to the local Maven repository"
echo "C++ half and Skia in $out ($count bridge sources)"
