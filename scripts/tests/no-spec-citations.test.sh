#!/usr/bin/env bash
# Code must not cite the planning documents (CLAUDE.md, Writing rule 3).
#
# A citation like "(SPEC PR-4)" in an error message explains nothing to the person who
# hits it, points at a file that is stripped from the published branch, and goes stale
# when a requirement is renumbered. The reason has to be written out instead.
#
# Comments are checked too, not only messages: the same staleness applies, and a comment
# that says "NFR-8 needs this" leaves the next reader no better off than silence.
#
# Commit messages and test function names are exempt and not searched here.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

pattern='SPEC|FR-[0-9]|NFR-[0-9]|PR-[0-9]|INTENT'

# Paths that legitimately name the documents: the publishing script decides which files to
# strip, and its test asserts on those names.
exempt='^(scripts/publish-main\.sh|scripts/tests/publish-main\.test\.sh|scripts/tests/no-spec-citations\.test\.sh|scripts/tests/planning-docs\.test\.sh)$'

files=()
while IFS= read -r file; do
    [[ "$file" =~ \.(kt|rs|c|m|h|sh|ps1|swift)$ ]] || continue
    [[ "$file" =~ $exempt ]] && continue
    files+=("$file")
done < <(git ls-files)

# One pass over every file, then drop test function names, which keep their requirement
# ids on purpose (CLAUDE.md, TDD rule 2).
hits="$(grep -nE "$pattern" "${files[@]}" 2>/dev/null |
    grep -viE '(fn|fun) +(fr|nfr|pr)[0-9]' || true)"

if [[ -n "$hits" ]]; then
    count="$(wc -l <<<"$hits" | tr -d ' ')"
    echo "fail  code cites the planning documents in $count place(s); write the reason out instead" >&2
    echo "$hits" >&2
    exit 1
fi

echo "ok    no SPEC or INTENT citations in code"
