#!/usr/bin/env bash
# Builds the Rust Host as a wasm module and puts it where the page fetches it.
# Usage: ./build-host.sh [--debug] [example]
#
# The default example is web_demo, the vertical slice. Any target that calls
# `dioxus_compose::web_main!` works the same way.
#
# Three link arguments make this module the Host half of a shared memory rather than a
# program of its own:
#
#   --import-memory   The Renderer's module defines the one linear memory and exports it.
#                     A Kotlin/Wasm module cannot import a memory, so the sharing has to
#                     go this way round or there is no sharing at all.
#   --global-base     Puts this module's data, and the stack and heap after it, above the
#                     addresses the Renderer's own allocator hands out. Both allocators
#                     work in one memory and an overlap draws a wrong screen rather than
#                     crashing, so the two ranges are separated deliberately.
#   --initial-memory  Declares how large the imported memory has to be. The Renderer's
#                     memory starts at zero pages, so the page grows it to this before the
#                     two module types match.
#
# `--stack-first` must not be added. It would put the shadow stack below --global-base,
# which is the half of the memory the Renderer is using.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
module_dir="$(cd "$script_dir/.." && pwd)"
repo_root="$(cd "$module_dir/../.." && pwd)"

profile=release
profile_dir=release
example=web_demo
for argument in "$@"; do
    case "$argument" in
        --debug) profile=dev; profile_dir=debug ;;
        *) example="$argument" ;;
    esac
done

target=wasm32-unknown-unknown
if ! rustup target list --installed | grep -qx "$target"; then
    echo "no $target toolchain; run: rustup target add $target" >&2
    exit 1
fi

# Both numbers are in the generated loader and in the generated Kotlin, from the same
# schema constants. Changing one here without regenerating would be caught by the page
# refusing to start, which is later than it should be, so they are read from the schema.
region_base="$(
    grep -o 'const val RUST_REGION_BASE: Int = [0-9]*' \
        "$module_dir/src/bridge/HostBridge.gen.kt" | grep -o '[0-9]*$'
)"
memory_pages="$(
    grep -o 'const MEMORY_MIN_PAGES = [0-9]*' \
        "$module_dir/resources/dioxus-compose-host.gen.mjs" | grep -o '[0-9]*$'
)"
initial_memory=$((memory_pages * 65536))

RUSTFLAGS="-Clink-arg=--import-memory \
-Clink-arg=--global-base=$region_base \
-Clink-arg=--initial-memory=$initial_memory" \
    cargo build --manifest-path "$repo_root/Cargo.toml" \
    --profile "$profile" --example "$example" --target "$target"

target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
built="$target_dir/$target/$profile_dir/examples/${example//-/_}.wasm"
destination="$module_dir/resources/dioxus_compose_host.wasm"
cp "$built" "$destination"
echo "built $destination ($(wc -c < "$destination" | tr -d ' ') bytes)"
echo "  region base:    $region_base"
echo "  initial memory: $initial_memory bytes ($memory_pages pages)"
