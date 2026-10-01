"""Adds the mingw_x64 variants to skiko's published root module metadata, in place.

Gradle resolves `org.jetbrains.skiko:skiko` through this file, which lists one variant per
target and the module that carries it. JetBrains' lists no mingw_x64, so a Compose module
built for Windows is told skiko does not support it. The variants added are the linux_x64
ones with the target and module renamed to the locally published skiko-mingwx64, placed
after them. Nothing else in the file changes, and a file that already names mingw_x64 is
left as it is.

Usage: extend-skiko-root.py <skiko-VERSION.module> <VERSION>
"""
import copy, json, sys
path, version = sys.argv[1], sys.argv[2]
module = json.load(open(path))
variants = module["variants"]
if any(v.get("attributes", {}).get("org.jetbrains.kotlin.native.target") == "mingw_x64" for v in variants):
    sys.exit(0)
added = []
for variant in variants:
    if not variant["name"].startswith("linuxX64"):
        continue
    mingw = copy.deepcopy(variant)
    mingw["name"] = variant["name"].replace("linuxX64", "mingwX64")
    mingw["attributes"]["org.jetbrains.kotlin.native.target"] = "mingw_x64"
    mingw["available-at"] = {
        "url": f"../../skiko-mingwx64/{version}/skiko-mingwx64-{version}.module",
        "group": "org.jetbrains.skiko",
        "module": "skiko-mingwx64",
        "version": version,
    }
    added.append(mingw)
if not added:
    sys.exit("the published skiko root has no linux_x64 variants to model mingw_x64 on")
at = max(i for i, v in enumerate(variants) if v["name"].startswith("linuxX64")) + 1
module["variants"] = variants[:at] + added + variants[at:]
json.dump(module, open(path, "w"), indent=2)
