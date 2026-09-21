#!/usr/bin/env bash
# Builds the Rust Host as an Android cdylib and puts it where the APK expects it.
# Usage: ./build-host.sh [--debug] [abi ...]
#
# The default ABI is arm64-v8a, which is what a modern device and an Apple silicon
# emulator both run. Pass `x86_64` as well for an Intel emulator.
#
# The NDK comes from ANDROID_NDK_HOME, or from the newest one under the SDK.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
module_dir="$(cd "$script_dir/.." && pwd)"
repo_root="$(cd "$module_dir/../.." && pwd)"

profile=release
profile_dir=release
abis=()
for argument in "$@"; do
    case "$argument" in
        --debug) profile=dev; profile_dir=debug ;;
        *) abis+=("$argument") ;;
    esac
done
if [[ ${#abis[@]} -eq 0 ]]; then
    abis=(arm64-v8a)
fi

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
ndk="${ANDROID_NDK_HOME:-}"
if [[ -z "$ndk" ]]; then
    newest="$(ls "$sdk/ndk" 2>/dev/null | sort -V | tail -1)"
    if [[ -n "$newest" ]]; then
        ndk="$sdk/ndk/$newest"
    fi
fi
if [[ ! -d "$ndk" ]]; then
    echo "no NDK found; install one or set ANDROID_NDK_HOME" >&2
    exit 1
fi

case "$(uname -s)" in
    Darwin) host_tag=darwin-x86_64 ;;
    Linux) host_tag=linux-x86_64 ;;
    *) echo "unsupported build host: $(uname -s)" >&2; exit 1 ;;
esac
toolchain="$ndk/toolchains/llvm/prebuilt/$host_tag/bin"

# Matches `minSdk` in module.yaml. The NDK ships one compiler wrapper per API level, so
# the two numbers have to be the same or the library will not load on the oldest device
# the application says it supports.
api=26

abi_target() {
    case "$1" in
        arm64-v8a) echo aarch64-linux-android ;;
        x86_64) echo x86_64-linux-android ;;
        *) echo "unsupported ABI: $1" >&2; exit 1 ;;
    esac
}

target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"

for abi in "${abis[@]}"; do
    target="$(abi_target "$abi")"
    linker="$toolchain/${target}${api}-clang"
    if [[ ! -x "$linker" ]]; then
        echo "no NDK linker at $linker" >&2
        exit 1
    fi
    upper_target="$(echo "$target" | tr 'a-z-' 'A-Z_')"
    env "CARGO_TARGET_${upper_target}_LINKER=$linker" \
        "CC_${target}=$linker" \
        "AR_${target}=$toolchain/llvm-ar" \
        cargo build --manifest-path "$repo_root/Cargo.toml" \
        --profile "$profile" --example android_demo --target "$target"

    destination="$module_dir/jniLibs/$abi"
    mkdir -p "$destination"
    cp "$target_dir/$target/$profile_dir/examples/libandroid_demo.so" "$destination/"
    echo "built $destination/libandroid_demo.so"
done
