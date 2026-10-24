#!/usr/bin/env bash
# Usage: ./scripts/tests/spec-cites-real-tests.test.sh
#
# Every test name the SPEC names in backticks is a test that exists.
#
# Acceptance criteria increasingly say which test stands behind them, which is what lets
# a reader check a claim instead of trusting it. A name that no longer matches anything
# turns that into the opposite: a requirement that reads as verified and is not. It is
# easy to do, because renaming a test does not touch the document, and because a name can
# be written from memory. One was, in the commit this test arrived with.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

failures=0
missing=()

# The protocol, the boundary and the renderer are compose-rust's, and so are their tests.
# compose-rust comes from crates.io, and its package leaves tests/ out, so the registry
# source cannot answer for them. The repository at the tag of the locked version can: it
# is cloned once, shallow, into .scratch/ (inside this repository, ignored by git), and
# searched with this repository. Without the network that clone cannot happen, and the
# names found nowhere here are reported as unchecked rather than missing, because nothing
# said they were wrong.
roots=(dioxus-compose samples)
compose_rust_checked=0
compose_rust_version="$(awk '/^name = "compose-rust"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
if [[ -n "$compose_rust_version" ]]; then
    compose_rust_dir=".scratch/spec-cites/compose-rust-v$compose_rust_version"
    if [[ ! -d "$compose_rust_dir/.git" ]]; then
        rm -rf "$compose_rust_dir"
        mkdir -p "$(dirname "$compose_rust_dir")"
        git clone --quiet --depth 1 --branch "v$compose_rust_version" \
            https://github.com/DarkPyonix/compose-rust "$compose_rust_dir" 2>/dev/null ||
            rm -rf "$compose_rust_dir"
    fi
    if [[ -d "$compose_rust_dir/.git" ]]; then
        roots+=("$compose_rust_dir")
        compose_rust_checked=1
    fi
fi

# A test name in the SPEC looks like `fr14_something_or_other`: a requirement prefix, an
# underscore, and lowercase words. Anything else in backticks is a type, a path or a flag.
while read -r name; do
    if ! grep -rq "fn $name\b\|fun $name\b" \
        --include='*.rs' --include='*.kt' \
        "${roots[@]}" 2>/dev/null; then
        missing+=("$name")
    fi
done < <(grep -oE '`(fr|nfr|pr)[0-9]+(_[0-9]+)*_[a-z0-9_]+`' docs/SPEC.md |
    tr -d '`' | sort -u)

if [[ ${#missing[@]} -gt 0 && $compose_rust_checked -eq 0 ]]; then
    printf 'skip  compose-rust v%s could not be fetched, so %d names found nowhere here\n' \
        "${compose_rust_version:-?}" "${#missing[@]}"
    printf '        were not checked against it:\n'
    for name in "${missing[@]}"; do printf '        %s\n' "$name"; done
elif [[ ${#missing[@]} -gt 0 ]]; then
    failures=1
    printf 'FAIL  the SPEC names %d tests that do not exist\n' "${#missing[@]}" >&2
    for name in "${missing[@]}"; do printf '        %s\n' "$name" >&2; done
    printf '        A criterion naming a test that is not there reads as verified and\n' >&2
    printf '        is not. Either the test was renamed, or the name was written from\n' >&2
    printf '        memory and never checked.\n' >&2
else
    echo "ok    every test the SPEC names exists"
fi
exit "$failures"
