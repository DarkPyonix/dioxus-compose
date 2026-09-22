#!/usr/bin/env bash
# The Renderer asks the Windows executable for the Host's functions by name, and the
# executable has to have been told to export exactly those names.
#
# GetProcAddress reads the PE export table. A Rust executable has no export table unless
# the link is told to make one, and `#[unsafe(no_mangle)] pub extern "C"` does not do it:
# that puts the symbol in the object file, which is what the Unix loader needs and is not
# what this needs. When they are missing the renderer loads, the window opens, and every
# call into the Host returns -1, so no mutations arrive and the window stays blank. A
# white window is exactly what the first person to run this on Windows saw.
#
# Nothing here needs Windows. Three lists have to agree, and all three are in the tree:
# what the build script exports, what the C shim looks up, and what the Host defines.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

build_script=dioxus-compose/build.rs
shim=dioxus-compose-renderer/desktop/c/renderer_entry.c
host=dioxus-compose/src/boundary.rs

for file in "$build_script" "$shim" "$host"; do
    [[ -f "$file" ]] || { echo "fail  $file is missing" >&2; exit 1; }
done

# What the build script asks the linker to export.
exported="$(grep -oE '"dioxus_compose_host_[a-z_]+"' "$build_script" |
    tr -d '"' | sort -u)"
# What the shim looks up. `LOAD_HOST_EXPORT(init, ...)` means dioxus_compose_host_init.
# The macro's own definition names its parameter `field`, so only the call sites count.
looked_up="$(grep -oE '^ *LOAD_HOST_EXPORT\([a-z_]+' "$shim" |
    sed 's/^ *LOAD_HOST_EXPORT(/dioxus_compose_host_/' | grep -v '_field$' | sort -u)"
# What the Host actually defines.
defined="$(grep -oE 'fn dioxus_compose_host_[a-z_]+' "$host" |
    sed 's/fn //' | sort -u)"

failures=0
compare() {
    local what="$1" left="$2" right="$3" left_name="$4" right_name="$5"
    local only_left only_right
    only_left="$(comm -23 <(echo "$left") <(echo "$right"))"
    only_right="$(comm -13 <(echo "$left") <(echo "$right"))"
    if [[ -n "$only_left" || -n "$only_right" ]]; then
        echo "fail  $what" >&2
        [[ -z "$only_left" ]] || echo "      only in $left_name: $(echo $only_left)" >&2
        [[ -z "$only_right" ]] || echo "      only in $right_name: $(echo $only_right)" >&2
        failures=$((failures + 1))
    fi
}

[[ -n "$exported" ]] || {
    echo "fail  the build script exports nothing, so a Windows build has no export table" >&2
    exit 1
}

compare "the linker is told to export a different set than the renderer looks up" \
    "$exported" "$looked_up" "the build script" "the renderer shim"
compare "the Host defines a different set than the linker is told to export" \
    "$defined" "$exported" "the Host" "the build script"

if [[ "$failures" -gt 0 ]]; then
    echo "a name in one list and not another is a call that returns -1 at run time" >&2
    exit 1
fi
echo "ok    $(echo "$exported" | wc -l | tr -d ' ') host functions are exported, looked up and defined alike"
