#!/usr/bin/env bash
# Usage: ./scripts/tests/blitz-patch.test.sh
#
# The patches under scripts/patches/ apply to the blitz-dom release the crate pins, and the
# directory they live in names that release.
#
# What goes wrong when they do not: a blitz-dom bump that leaves the old patch directory
# behind makes scripts/apply-patches.sh patch a release nobody depends on any more, or fail
# halfway, and every cargo command in a fresh clone stops with it.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

failures=0
pass() { printf 'ok    %s\n' "$1"; }
fail() {
    failures=$((failures + 1))
    printf 'FAIL  %s\n' "$1" >&2
    shift
    local line
    for line in "$@"; do printf '        %s\n' "$line" >&2; done
}

# --- the directory's version is the pinned version ---------------------------
pinned="$(sed -nE 's/^blitz-dom[[:space:]]*=[[:space:]]*"=([0-9.]+)".*/\1/p' dioxus-compose/Cargo.toml)"
dirs=(scripts/patches/blitz-dom-*)
if [[ -z "$pinned" ]]; then
    fail "dioxus-compose/Cargo.toml has no exact blitz-dom pin" \
         "expected a line like: blitz-dom = \"=0.2.4\""
elif [[ ${#dirs[@]} -ne 1 || "${dirs[0]}" != "scripts/patches/blitz-dom-$pinned" ]]; then
    fail "the patch directory does not match the pin" \
         "pinned: blitz-dom =$pinned" \
         "found:  ${dirs[*]}"
else
    pass "scripts/patches/blitz-dom-$pinned matches the =$pinned pin"
fi

# The workspace patch and the script name the same release.
if [[ -n "$pinned" ]] \
    && grep -qF "path = \".scratch/patched/blitz-dom-$pinned\"" Cargo.toml \
    && grep -qE "^version=$pinned\$" scripts/apply-patches.sh; then
    pass "Cargo.toml and apply-patches.sh name blitz-dom $pinned"
else
    fail "Cargo.toml's [patch.crates-io] path or apply-patches.sh's version is not $pinned"
fi

# --- the patches apply cleanly to the pinned release ------------------------
# Runs the real script against this checkout, so it needs the network the first time.
if ! command -v curl >/dev/null 2>&1; then
    echo "skip  patches apply (no curl on PATH)"
elif output="$(./scripts/apply-patches.sh 2>&1)"; then
    pass "scripts/apply-patches.sh applies every patch"
    if grep -q '\.rej$' < <(find .scratch/patched -name '*.rej' 2>/dev/null); then
        fail "a patch left a .rej file"
    fi
else
    fail "scripts/apply-patches.sh failed" "$output"
fi

if [[ $failures -ne 0 ]]; then
    echo "$failures check(s) failed" >&2
    exit 1
fi
