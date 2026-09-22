#!/usr/bin/env bash
# Usage: ./scripts/build-sample-apks.sh [--debug] [output-directory] [abi ...]
#
# One APK per sample.
#
# The Android module hosts one application at a time: its Activity loads a library under
# a single name, and which sample that is comes from the build rather than from Kotlin.
# So this is a loop that builds the sample's cdylib, builds the APK around it, and moves
# the result aside under the sample's own name before the next one overwrites it.
#
# A sample that does not build is skipped and named at the end. Eleven samples do not all
# have to work for the ten that do to be worth shipping.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

profile_flag=
profile_name=release
output=
abis=()
for argument in "$@"; do
    case "$argument" in
        --debug) profile_flag=--debug; profile_name=debug ;;
        -*) echo "unknown option $argument" >&2; exit 2 ;;
        *) if [[ -z "$output" ]]; then output="$argument"; else abis+=("$argument"); fi ;;
    esac
done
output="${output:-$repo_root/target/sample-apks}"
case "$output" in /*) ;; *) output="$PWD/$output" ;; esac
[[ ${#abis[@]} -gt 0 ]] || abis=(arm64-v8a)

mkdir -p "$output"
built=()
skipped=()

for manifest in samples/*/Cargo.toml; do
    sample="$(basename "$(dirname "$manifest")")"
    [[ -f "samples/$sample/src/lib.rs" ]] || continue
    echo "== $sample"

    if ! ./dioxus-compose-renderer/android/scripts/build-host.sh \
        ${profile_flag:+$profile_flag} --sample "$sample" "${abis[@]}"; then
        skipped+=("$sample (host)")
        continue
    fi

    # The APK is rebuilt from nothing each time. Amper keeps the packaged libraries in an
    # intermediate directory its own up-to-date check does not tie to this file, so a
    # second sample would otherwise ship the first one's library: that is how an Android
    # build here once ran three times with the same stale code in it.
    rm -rf dioxus-compose-renderer/build/tasks/_android_buildAndroid*
    if ! (cd dioxus-compose-renderer && ./kotlin build -m android); then
        skipped+=("$sample (apk)")
        continue
    fi

    apk="$(find dioxus-compose-renderer/build/tasks -name '*.apk' -print -quit)"
    if [[ -z "$apk" ]]; then
        skipped+=("$sample (no apk)")
        continue
    fi
    cp "$apk" "$output/$sample-android-${abis[0]}.apk"
    built+=("$sample")
done

echo
echo "built ${#built[@]}: ${built[*]:-none}"
[[ ${#skipped[@]} -eq 0 ]] || echo "skipped ${#skipped[@]}: ${skipped[*]}"
[[ ${#built[@]} -gt 0 ]] || { echo "no APK was built at all, which is the toolchain rather than a sample" >&2; exit 1; }
