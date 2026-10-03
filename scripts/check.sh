#!/usr/bin/env bash
# Usage: ./scripts/check.sh [--full]
#
# Runs the repository's quality gate: the Rust workspace. The default uses Criterion's
# quick mode for local and CI presubmit checks; --full runs the full benchmark sample.
#
# The renderer is compose-rust's, built and tested in that repository. Nothing here builds
# Kotlin.
#
# Everything below runs with compose-rust's `mock-renderer` feature on. The tests drive the
# Host and read the records it writes; none of them draws, so none of them needs the
# renderer the default `native-renderer` feature downloads, and a gate that fetched one
# would fail whenever the published renderer and the pinned compose-rust disagree, for a
# reason no change here can fix. Feature unification turns it on for every crate in the
# workspace that depends on dioxus-compose. The default build is still compiled once, by
# the docs.rs check at the end, which takes the renderer feature's code path without
# linking a renderer.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

full=0

for arg in "$@"; do
    case "$arg" in
        --full) full=1 ;;
        *) echo "usage: $0 [--full]" >&2; exit 2 ;;
    esac
done

if [[ $full -eq 1 ]]; then
    bench_args=(--noplot)
    unset DXC_BENCH_QUICK
else
    bench_args=(--quick --noplot)
    export DXC_BENCH_QUICK=1
fi

mock=(--features dioxus-compose/mock-renderer)

cargo fmt --all -- --check
cargo clippy --workspace --all-targets "${mock[@]}" -- -D warnings
cargo test --workspace "${mock[@]}"
# A headless or documentation build, with neither renderer feature. It has to compile, and
# the tests that assert it does not quietly pretend to have a renderer only exist in it.
cargo test -p dioxus-compose --no-default-features
# The Android entry point is behind cfg(target_os = "android"), so nothing above compiles
# it. Skipped where the target is missing, because adding it is a download and this gate is
# meant to run anywhere.
if rustup target list --installed | grep -qx aarch64-linux-android; then
    cargo clippy -p dioxus-compose --target aarch64-linux-android "${mock[@]}" -- -D warnings
else
    echo "skipping the Android target (rustup target add aarch64-linux-android)"
fi
# The same for the browser's entry point, which is behind cfg(target_family = "wasm"). The
# renderer feature is off because a browser links no renderer: the page installs the
# renderer API from the entry point it calls.
if rustup target list --installed | grep -qx wasm32-unknown-unknown; then
    cargo clippy -p dioxus-compose --no-default-features \
        --target wasm32-unknown-unknown -- -D warnings
else
    echo "skipping the wasm target (rustup target add wasm32-unknown-unknown)"
fi
# What docs.rs does: the renderer feature is on and there is no network to fetch a renderer
# with. The documentation still has to build.
DOCS_RS=1 cargo check -p dioxus-compose
# --benches restricts the run to Criterion bench targets. Without it cargo also runs the
# lib's default test harness in bench mode, and that harness rejects Criterion's flags.
cargo bench --workspace --benches "${mock[@]}" -- "${bench_args[@]}"
