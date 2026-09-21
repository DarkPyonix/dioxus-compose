#!/usr/bin/env bash
# Every sample has to find the renderer the same way.
#
# A sample links the renderer shared library and the dynamic linker loads it at start up,
# so a sample whose build script did not emit an rpath builds clean and then dies with
# "Library not loaded" the first time anyone runs it. Two of the four samples had a copy
# of the build script that only looked at DIOXUS_COMPOSE_RENDERER_DIR, so the documented
# command worked for the other two and failed for them, which reads as the sample being
# broken rather than as the sample missing a line.
#
# The scripts solve one problem and there is no per-sample part of it, so they are checked
# for being the same file rather than for containing the right lines. Drift is the failure
# here: a fix made in one copy and not the others is exactly how this happened.
set -euo pipefail

cd "$(dirname "$0")/../.."

reference="samples/chat/build.rs"
[[ -f "$reference" ]] || {
    echo "error: $reference is missing, so there is nothing to compare the others to" >&2
    exit 1
}

status=0
for script in samples/*/build.rs; do
    [[ "$script" == "$reference" ]] && continue
    if ! diff -q "$reference" "$script" >/dev/null; then
        echo "error: $script differs from $reference" >&2
        echo "       Both exist to point the sample binary at the renderer, and there is" >&2
        echo "       nothing sample-specific about doing that. A copy that has drifted" >&2
        echo "       usually means a fix landed in one sample and not the rest, and the" >&2
        echo "       sample that missed it starts with 'Library not loaded'." >&2
        diff -u "$reference" "$script" | sed 's/^/       /' >&2
        status=1
    fi
done

# The fallback is what makes the documented command work in a checkout. Without it a
# sample links only when DIOXUS_COMPOSE_RENDERER_DIR is exported, which the README offers
# as one way of doing it rather than as a requirement.
grep -q 'dioxus-compose-renderer/build/native-image/dist/lib' "$reference" || {
    echo "error: $reference does not fall back to the renderer built in this checkout" >&2
    echo "       Without that fallback every sample needs DIOXUS_COMPOSE_RENDERER_DIR" >&2
    echo "       exported before it will run." >&2
    status=1
}

[[ $status -eq 0 ]] && echo "ok    sample renderer link"
exit $status
