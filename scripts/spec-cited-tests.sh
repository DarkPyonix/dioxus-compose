#!/usr/bin/env bash
# Usage: ./scripts/spec-cited-tests.sh [SPEC.md]
#
# Prints the test names a SPEC cites that must exist, one per line, sorted.
#
# A name looks like `fr14_something_or_other`: a requirement prefix, an underscore, and
# lowercase words. Anything else in backticks is a type, a path or a flag. A name written
# as `name` (예정) is a test that is planned and not written yet, so it is left out; the
# marker has to be removed when the test lands, and then the name is checked like any other.
set -uo pipefail
spec="${1:-docs/SPEC.md}"
grep -oE '`(fr|nfr|pr)[0-9]+(_[0-9]+)*_[a-z0-9_]+`( \(예정\))?' "$spec" |
    grep -v '(예정)$' | tr -d '`' | sort -u
