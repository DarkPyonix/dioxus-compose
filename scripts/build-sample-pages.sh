#!/usr/bin/env bash
# Usage: ./scripts/build-sample-pages.sh [--debug] [output-directory]
#
# One browser page per sample, plus an index that links them.
#
# A page is the renderer's wasm module, the sample's wasm module, and the generated
# forwarder between them. The renderer half is the same bytes for every sample, so it is
# built once and copied; only the Host half is rebuilt per sample.
#
# A sample that does not build is skipped and named at the end.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

profile_flag=
output=
for argument in "$@"; do
    case "$argument" in
        --debug) profile_flag=--debug ;;
        -*) echo "unknown option $argument" >&2; exit 2 ;;
        *) output="$argument" ;;
    esac
done
output="${output:-$repo_root/target/sample-pages}"
case "$output" in /*) ;; *) output="$PWD/$output" ;; esac

web_dir="$repo_root/dioxus-compose-renderer/web"
renderer_out="$repo_root/dioxus-compose-renderer/build/tasks/_web_buildWasmJs"

echo "==> building the renderer's module once"
(cd "$repo_root/dioxus-compose-renderer" && ./kotlin build -p wasmJs --include-module web) ||
    { echo "the renderer's wasm module did not build" >&2; exit 1; }

# Whatever the toolchain called the output directory this time. Naming it exactly would
# be a guess that breaks on the next Amper release, and the search is cheap.
bundle="$(find "$repo_root/dioxus-compose-renderer/build" -name '*.wasm' -path '*web*' \
    -not -path '*resources*' -print -quit 2>/dev/null)"
bundle_dir="$(dirname "${bundle:-$renderer_out}")"

mkdir -p "$output"
built=()
skipped=()

for manifest in samples/*/Cargo.toml; do
    sample="$(basename "$(dirname "$manifest")")"
    [[ -f "samples/$sample/src/lib.rs" ]] || continue
    echo "== $sample"
    if ! ./dioxus-compose-renderer/web/scripts/build-host.sh ${profile_flag:+$profile_flag} \
        --sample "$sample"; then
        skipped+=("$sample")
        continue
    fi
    page="$output/$sample"
    rm -rf "$page"
    mkdir -p "$page"
    cp -R "$bundle_dir/." "$page/" 2>/dev/null || true
    cp "$web_dir/resources/dioxus_compose_host.wasm" "$page/"
    cp "$web_dir/resources/dioxus-compose-host.gen.mjs" "$page/"
    cp "$web_dir/resources/index.html" "$page/"
    cp "$web_dir/resources/styles.css" "$page/" 2>/dev/null || true
    built+=("$sample")
done

# An index, so the collection is something a person can walk into rather than a set of
# directories they have to know the names of.
{
    echo '<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">'
    echo '<meta name="viewport" content="width=device-width, initial-scale=1">'
    echo '<title>dioxus-compose samples</title>'
    echo '<style>body{font:16px/1.6 system-ui,sans-serif;margin:3rem auto;max-width:34rem;padding:0 1rem}'
    echo 'h1{font-size:1.5rem}li{margin:.4rem 0}</style></head><body>'
    echo '<h1>dioxus-compose samples</h1>'
    echo '<p>The same Rust source these ship as desktop programs, drawn in a browser.</p><ul>'
    for sample in "${built[@]}"; do
        echo "<li><a href=\"$sample/\">$sample</a></li>"
    done
    echo '</ul></body></html>'
} > "$output/index.html"

echo
echo "built ${#built[@]} pages into $output: ${built[*]:-none}"
[[ ${#skipped[@]} -eq 0 ]] || echo "skipped ${#skipped[@]}: ${skipped[*]}"
[[ ${#built[@]} -gt 0 ]] || { echo "no page was built at all" >&2; exit 1; }
