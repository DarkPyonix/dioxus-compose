#!/usr/bin/env bash
# Tests for scripts/split-branches.sh.
#
# Every case runs against a throwaway repository built in a temporary
# directory -- never against this repository, and never against any remote.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
split="$script_dir/../split-branches.sh"

failures=0
pass() { echo "ok   - $1"; }
fail() { echo "FAIL - $1"; failures=$((failures + 1)); }
check() { if [[ "$2" == "$3" ]]; then pass "$1"; else fail "$1: expected [$3], got [$2]"; fi; }
check_contains() {
    if [[ "$2" == *"$3"* ]]; then pass "$1"; else fail "$1: [$2] does not contain [$3]"; fi
}
check_absent() {
    if [[ "$2" != *"$3"* ]]; then pass "$1"; else fail "$1: [$2] unexpectedly contains [$3]"; fi
}

# A repository shaped like dioxus-compose: public source and guide, private
# planning documents.
make_repo() {
    local dir="$1"
    mkdir -p "$dir"
    git -C "$dir" init -q -b develop
    git -C "$dir" config user.email test@example.invalid
    git -C "$dir" config user.name "Split Test"
    mkdir -p "$dir/docs/guide" "$dir/dioxus-compose/src"
    echo readme > "$dir/README.md"
    echo license > "$dir/LICENSE"
    echo project > "$dir/PROJECT.md"
    echo claude > "$dir/CLAUDE.md"
    echo intent > "$dir/docs/INTENT.md"
    echo spec > "$dir/docs/SPEC.md"
    echo guide > "$dir/docs/guide/index.md"
    echo code > "$dir/dioxus-compose/src/lib.rs"
    git -C "$dir" add -A
    git -C "$dir" commit -q -m "Initial commit"
}

files_on() { git -C "$1" ls-tree -r --name-only "$2" | sort; }

# --- refuses to run with uncommitted changes -------------------------------
tmp="$(mktemp -d)"
repo="$tmp/dirty"
make_repo "$repo"
echo "scratch" >> "$repo/README.md"
out="$(cd "$repo" && "$split" 2>&1)"
status=$?
check "refuses with uncommitted changes (exit)" "$status" "1"
check_contains "refuses with uncommitted changes (message)" "$out" "uncommitted"
check "refuses with uncommitted changes (main not created)" \
    "$(git -C "$repo" rev-parse --verify -q main >/dev/null 2>&1; echo $?)" "1"

# --- dry run is the default and writes nothing -----------------------------
repo="$tmp/dry"
make_repo "$repo"
out="$(cd "$repo" && "$split" 2>&1)"
check "dry run succeeds" "$?" "0"
check_contains "dry run names itself" "$out" "dry run"
check_contains "dry run lists PROJECT.md" "$out" "PROJECT.md"
check_contains "dry run lists docs/SPEC.md" "$out" "docs/SPEC.md"
check_absent "dry run keeps README.md off the removal list" "$out" "- README.md"
check "dry run does not create main" \
    "$(git -C "$repo" rev-parse --verify -q main >/dev/null 2>&1; echo $?)" "1"

# --- --write produces main with the private documents removed --------------
repo="$tmp/write"
make_repo "$repo"
out="$(cd "$repo" && "$split" --write 2>&1)"
check "--write succeeds" "$?" "0"
main_files="$(files_on "$repo" main)"
check "main keeps the public tree" "$main_files" "$(printf '%s\n' \
    LICENSE README.md dioxus-compose/src/lib.rs docs/guide/index.md | sort)"
check_absent "main drops PROJECT.md" "$main_files" "PROJECT.md"
check_absent "main drops CLAUDE.md" "$main_files" "CLAUDE.md"
check_absent "main drops docs/INTENT.md" "$main_files" "docs/INTENT.md"
check_absent "main drops docs/SPEC.md" "$main_files" "docs/SPEC.md"

# --- the working tree of the current branch is never touched ---------------
check "develop's working tree still has PROJECT.md" "$(cat "$repo/PROJECT.md")" "project"
check "develop's working tree still has docs/SPEC.md" "$(cat "$repo/docs/SPEC.md")" "spec"
check "the checkout is still on develop" "$(git -C "$repo" rev-parse --abbrev-ref HEAD)" "develop"
check "the checkout is still clean" "$(git -C "$repo" status --porcelain)" ""

# --- it prints the push command instead of pushing -------------------------
check_contains "prints the push command" "$out" "git push origin main"
check "no remote was contacted (none exists)" "$(git -C "$repo" remote)" ""

# --- idempotent: a second run is a no-op -----------------------------------
before="$(git -C "$repo" rev-parse main)"
out="$(cd "$repo" && "$split" --write 2>&1)"
check "second run succeeds" "$?" "0"
check_contains "second run reports no work" "$out" "up to date"
check "second run leaves main unmoved" "$(git -C "$repo" rev-parse main)" "$before"

# --- re-runnable as develop advances ---------------------------------------
echo more > "$repo/dioxus-compose/src/extra.rs"
echo "more spec" >> "$repo/docs/SPEC.md"
git -C "$repo" add -A
git -C "$repo" commit -q -m "Feat: More"
(cd "$repo" && "$split" --write >/dev/null 2>&1)
main_files="$(files_on "$repo" main)"
check_contains "an advanced develop brings new source to main" "$main_files" "extra.rs"
check_absent "an advanced develop still drops docs/SPEC.md" "$main_files" "docs/SPEC.md"
if [[ "$(git -C "$repo" rev-parse main)" != "$before" ]]; then
    pass "main advanced"
else
    fail "main advanced: still at $before"
fi
check "main descends from the previous main" \
    "$(git -C "$repo" merge-base --is-ancestor "$before" main; echo $?)" "0"
check "main records develop in its history" \
    "$(git -C "$repo" merge-base --is-ancestor develop main; echo $?)" "0"

# --- new files directly under docs/ are dropped without editing the script -
echo notes > "$repo/docs/ROADMAP.md"
git -C "$repo" add -A
git -C "$repo" commit -q -m "Docs: Roadmap"
(cd "$repo" && "$split" --write >/dev/null 2>&1)
check_absent "a new docs/ document is dropped too" "$(files_on "$repo" main)" "ROADMAP.md"
check_contains "docs/guide/ survives" "$(files_on "$repo" main)" "docs/guide/index.md"

# --- safe to run while main itself is checked out --------------------------
repo="$tmp/on-main"
make_repo "$repo"
(cd "$repo" && "$split" --write >/dev/null 2>&1)
git -C "$repo" checkout -q main
out="$(cd "$repo" && "$split" --write 2>&1)"
check "running while on main succeeds" "$?" "0"
check "running while on main leaves the tree clean" "$(git -C "$repo" status --porcelain)" ""
check "running while on main keeps main's checkout intact" \
    "$([[ -f "$repo/README.md" ]] && echo yes)" "yes"

# --- works from a branch whose HEAD differs from the source branch ---------
#
# Regression: `git rm --cached` compares index entries against HEAD, so
# without --force it refuses when the checked-out branch has diverged from
# --source, and the private files silently survive into the target.
repo="$tmp/other-head"
make_repo "$repo"
git -C "$repo" checkout -q -b feature
echo work > "$repo/dioxus-compose/src/feature.rs"
# The private files must differ between HEAD and --source too: that is what
# makes git compare them and refuse.
echo "edited on the feature branch" >> "$repo/PROJECT.md"
echo "edited on the feature branch" >> "$repo/docs/SPEC.md"
git -C "$repo" add -A
git -C "$repo" commit -q -m "Feat: Work in progress"
out="$(cd "$repo" && "$split" --write 2>&1)"
check "runs from a diverged branch" "$?" "0"
main_files="$(files_on "$repo" main)"
check_absent "a diverged HEAD still drops PROJECT.md" "$main_files" "PROJECT.md"
check_absent "a diverged HEAD still drops docs/SPEC.md" "$main_files" "docs/SPEC.md"
check_absent "the target takes content from --source, not HEAD" "$main_files" "feature.rs"
check "the diverged branch is still clean" "$(git -C "$repo" status --porcelain)" ""

# --- a missing develop branch is an error, not a silent success ------------
repo="$tmp/no-develop"
make_repo "$repo"
git -C "$repo" branch -m develop trunk
out="$(cd "$repo" && "$split" 2>&1)"
check "missing source branch fails" "$?" "1"
check_contains "missing source branch explains itself" "$out" "develop"
out="$(cd "$repo" && "$split" --source trunk 2>&1)"
check "--source selects another branch" "$?" "0"

rm -rf "$tmp"

if (( failures )); then
    echo "$failures test(s) failed" >&2
    exit 1
fi
echo "all split-branches.sh tests passed"
