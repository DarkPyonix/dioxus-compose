#!/usr/bin/env bash
# Usage: ./scripts/check.sh [--full] [--no-kotlin]
#
# Runs the repository's quality gates: the Rust workspace, then the Kotlin renderer
# project. The default uses Criterion's quick mode for local and CI presubmit checks;
# --full runs the full benchmark sample.
#
# The Kotlin gate runs by default. Skip it with --no-kotlin or DXC_SKIP_KOTLIN=1 when
# the Kotlin Toolchain has not been downloaded yet (the wrapper fetches it on first use).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

full=0
skip_kotlin="${DXC_SKIP_KOTLIN:-0}"

for arg in "$@"; do
    case "$arg" in
        --full) full=1 ;;
        --no-kotlin) skip_kotlin=1 ;;
        *) echo "usage: $0 [--full] [--no-kotlin]" >&2; exit 2 ;;
    esac
done

if [[ $full -eq 1 ]]; then
    bench_args=(--noplot)
    unset DXC_BENCH_QUICK
else
    bench_args=(--quick --noplot)
    export DXC_BENCH_QUICK=1
fi

cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
# The renderer is a default feature, so the run above never builds the crate the way a
# headless or documentation build gets it. That build has to compile, and the tests that
# assert it does not quietly pretend to have a renderer only exist in it.
cargo test -p dioxus-compose --no-default-features
# What docs.rs does: the feature is on and there is no network to fetch a renderer with.
# The documentation still has to build.
DOCS_RS=1 cargo check -p dioxus-compose --all-features
# --benches restricts the run to Criterion bench targets. Without it cargo also runs the
# lib's default test harness in bench mode, and that harness rejects Criterion's flags.
cargo bench --workspace --benches -- "${bench_args[@]}"

if [[ "$skip_kotlin" != "0" ]]; then
    echo "skipping the Kotlin gate (--no-kotlin or DXC_SKIP_KOTLIN)"
    exit 0
fi

cd "$repo_root/dioxus-compose-renderer"
./kotlin build
./kotlin test
