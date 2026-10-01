#!/usr/bin/env bash
# skiko is built for Windows from a pinned revision with patches applied. The revision is
# written in two places, the script that builds it and the README that says why, and the
# version it publishes as has to be the one Compose asks for. Any of the three drifting is
# a build that quietly uses a different skiko from the one the patches were written for,
# or publishes one nothing resolves.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
script="$repo_root/dioxus-compose-renderer/scripts/build-skiko-windows.sh"
readme="$repo_root/dioxus-compose-renderer/patches/README.md"
patches="$repo_root/dioxus-compose-renderer/patches/skiko"
red=0

for file in "$script" "$readme"; do
    [[ -f "$file" ]] || { echo "missing $file"; exit 1; }
done

revision="$(sed -n 's/^REVISION="\([0-9a-f]*\)"$/\1/p' "$script")"
published="$(sed -n 's/^PUBLISHED_AS="\(.*\)"$/\1/p' "$script")"

if [[ ${#revision} -ne 40 ]]; then
    echo "FAIL the script does not pin skiko to a full revision"; red=1
elif ! awk '/^## skiko/{s=1} s' "$readme" | grep -q "$revision"; then
    echo "FAIL patches/README.md names a different skiko revision from the script ($revision)"; red=1
fi

if ! awk '/^## skiko/{s=1} s' "$readme" | grep -q "v$published"; then
    echo "FAIL patches/README.md does not name the version the script publishes as ($published)"; red=1
fi

if ! ls "$patches"/*.patch >/dev/null 2>&1; then
    echo "FAIL no skiko patches to apply"; red=1
fi

for patch in "$patches"/*.patch; do
    name="skiko/$(basename "$patch")"
    grep -q "$name" "$readme" || { echo "FAIL $name is not explained in patches/README.md"; red=1; }
done

[[ $red -eq 0 ]] && echo "ok   skiko pin $revision, published as $published"
exit $red
