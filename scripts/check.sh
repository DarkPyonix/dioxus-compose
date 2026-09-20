#!/usr/bin/env bash
# Usage: ./scripts/check.sh [--full]
#
# Runs the repository's Rust quality gates. The default uses Criterion's quick
# mode for local and CI presubmit checks; --full runs the full benchmark sample.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

case "${1:-}" in
    "")
        bench_args=(--quick --noplot)
        export DXC_BENCH_QUICK=1
        ;;
    --full)
        bench_args=(--noplot)
        unset DXC_BENCH_QUICK
        ;;
    *)
        echo "usage: $0 [--full]" >&2
        exit 2
        ;;
esac

cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo bench --workspace -- "${bench_args[@]}"
