#!/usr/bin/env bash
# Usage: ./scripts/tests/spec-cited-tests.test.sh
#
# A SPEC name marked (예정) is exempt from the existence check; an unmarked one is not.
set -uo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
printf '%s\n' \
    '테스트 `fr1_5_built_one`, `fr1_5_planned_one` (예정), `fr1_5_other_built`.' \
    '그리고 `SomeType` 와 `fr1_5_planned_two` (예정)' > "$tmp/SPEC.md"
got="$("$repo_root/scripts/spec-cited-tests.sh" "$tmp/SPEC.md" | tr '\n' ' ')"
if [[ "$got" == "fr1_5_built_one fr1_5_other_built " ]]; then
    echo "ok    planned names are exempt, unmarked names are checked"
else
    echo "FAIL  expected the two unmarked names, got: $got" >&2
    exit 1
fi
