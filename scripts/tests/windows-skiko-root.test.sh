#!/usr/bin/env bash
# Compose depends on `org.jetbrains.skiko:skiko`, and Gradle picks the variant for a target
# from that module's metadata. JetBrains' names no mingw_x64, so the Windows build extends
# a copy of it to name the skiko-mingwx64 published locally. This runs that step on a
# trimmed copy of JetBrains' file (`fixtures/skiko-root.module`) and checks that the two
# mingw_x64 variants are added in the linux_x64 shape, that nothing else moves, and that a
# second run changes nothing.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
script="$repo_root/dioxus-compose-renderer/scripts/extend-skiko-root.py"
fixture="$repo_root/scripts/tests/fixtures/skiko-root.module"
for file in "$script" "$fixture"; do
    [[ -f "$file" ]] || { echo "missing $file"; exit 1; }
done
command -v python3 >/dev/null || { echo "skip: no python3"; exit 0; }

mkdir -p "$repo_root/.scratch"
work="$(mktemp -d "$repo_root/.scratch/windows-skiko-root.XXXXXX")"
trap 'rm -rf "$work"' EXIT
cp "$fixture" "$work/skiko.module"

python3 "$script" "$work/skiko.module" 0.144.6 || { echo "FAIL the script failed on the fixture"; exit 1; }
cp "$work/skiko.module" "$work/once.module"
python3 "$script" "$work/skiko.module" 0.144.6 || { echo "FAIL the script failed on its own output"; exit 1; }

python3 - "$fixture" "$work/once.module" "$work/skiko.module" <<'PY'
import json, sys
original, once, twice = (json.load(open(p)) for p in sys.argv[1:4])
red = 0
def fail(message):
    global red
    print("FAIL " + message)
    red = 1

names = [v["name"] for v in once["variants"]]
for expected in ("mingwX64ApiElements-published", "mingwX64SourcesElements-published"):
    if expected not in names:
        fail(f"{expected} was not added")

for variant in once["variants"]:
    if not variant["name"].startswith("mingwX64"):
        continue
    if variant["attributes"].get("org.jetbrains.kotlin.native.target") != "mingw_x64":
        fail(f"{variant['name']} does not name the mingw_x64 target")
    at = variant.get("available-at", {})
    if at.get("module") != "skiko-mingwx64" or at.get("version") != "0.144.6":
        fail(f"{variant['name']} does not point at skiko-mingwx64 0.144.6")
    if at.get("url") != "../../skiko-mingwx64/0.144.6/skiko-mingwx64-0.144.6.module":
        fail(f"{variant['name']} points at {at.get('url')}")

untouched = [v for v in once["variants"] if not v["name"].startswith("mingwX64")]
if untouched != original["variants"] or once["component"] != original["component"]:
    fail("variants other than mingw_x64 changed, so other targets would resolve differently")

if twice != once:
    fail("a second run changed the file again")

if not red:
    print("ok   skiko root names mingw_x64 and nothing else moved")
sys.exit(red)
PY
