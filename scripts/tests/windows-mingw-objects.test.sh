#!/usr/bin/env bash
# The Windows renderer is a MinGW object linked by an MSVC-mode linker, and two MinGW
# conventions mean something else there. Left as they are, the link succeeds and the
# program is wrong: static constructors never run, so the Kotlin runtime starts without its
# globals and the first call hangs; and every function's unwind data is dropped, so a Kotlin
# exception that crosses two functions sends the unwinder into a loop. Both were seen.
#
# `dioxus-compose-renderer/scripts/fix-mingw-objects.py` rewrites the objects before the
# link. This runs it on a small MinGW object built the way Kotlin/Native builds its own
# (`fixtures/mingw-object.cpp` says how), then reads the result back and checks:
#
#   - no .ctors section is left, and the constructor is in .CRT$XCU, where the MSVC
#     runtime looks;
#   - every .pdata$NAME and .xdata$NAME is an associative COMDAT of the .text$NAME it
#     describes, which is what keeps an MSVC-mode linker from discarding it.
#
# It needs python3 and nothing else; no compiler, no Windows.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixer="$repo_root/dioxus-compose-renderer/scripts/fix-mingw-objects.py"
fixture="$repo_root/scripts/tests/fixtures/mingw-object.a"

for file in "$fixer" "$fixture"; do
    [[ -f "$file" ]] || { echo "missing $file"; exit 1; }
done
command -v python3 >/dev/null || { echo "skip: no python3"; exit 0; }

# Inside the repository, as everything this project makes is.
mkdir -p "$repo_root/.scratch"
work="$(mktemp -d "$repo_root/.scratch/windows-mingw-objects.XXXXXX")"
trap 'rm -rf "$work"' EXIT

cp "$fixture" "$work/object.a"
python3 "$fixer" "$work/object.a" >/dev/null || { echo "the fixer failed on the fixture"; exit 1; }

python3 - "$work/object.a" "$fixture" <<'PY'
import struct, sys

def members(path):
    buf = open(path, "rb").read()
    pos = 8
    while pos + 60 <= len(buf):
        size = int(buf[pos + 48:pos + 58].decode().strip())
        body = pos + 60
        if buf[body:body + 2] == b"\x64\x86":
            yield buf, body
        pos = body + size + (size & 1)

def read(path):
    """Sections of every COFF member: name, and COMDAT (selection, associated index)."""
    out = []
    for buf, base in members(path):
        _, nsec, _, symptr, nsym, optsize, _ = struct.unpack_from("<HHIIIHH", buf, base)
        strtab = base + symptr + nsym * 18
        table = base + 20 + optsize
        names = []
        for i in range(nsec):
            at = table + i * 40
            name = buf[at:at + 8].rstrip(b"\0").decode()
            if name.startswith("/"):
                o = int(name[1:]); end = buf.index(b"\0", strtab + o)
                name = buf[strtab + o:end].decode()
            names.append(name)
        comdat = {}
        i = 0
        while i < nsym:
            at = base + symptr + i * 18
            section = struct.unpack_from("<h", buf, at + 12)[0]
            storage, aux = buf[at + 16], buf[at + 17]
            if storage == 3 and aux and section > 0:
                characteristics = struct.unpack_from("<I", buf, table + (section - 1) * 40 + 36)[0]
                if characteristics & 0x1000:
                    number = struct.unpack_from("<H", buf, at + 18 + 12)[0]
                    comdat[section] = (buf[at + 18 + 14], number)
            i += 1 + aux
        out.append((names, comdat))
    return out

fixed, original = sys.argv[1], sys.argv[2]
red = 0

def fail(message):
    global red
    print("FAIL " + message)
    red = 1

# The fixture has to contain what is being fixed, or the checks below prove nothing.
before = [n for names, _ in read(original) for n in names]
if ".ctors" not in before:
    fail("the fixture has no .ctors section to move; rebuild it as its source says")
if not any(n.startswith(".pdata$") for n in before):
    fail("the fixture has no per-function unwind data; rebuild it with -ffunction-sections")

for names, comdat in read(fixed):
    if ".ctors" in names:
        fail("a .ctors section is still there, and the MSVC runtime will never run what is in it")
    if ".CRT$XCU" not in names:
        fail("no .CRT$XCU section: the constructor is not where the MSVC runtime looks")
    for index, name in enumerate(names, 1):
        for prefix in (".pdata$", ".xdata$"):
            if not name.startswith(prefix):
                continue
            selection, number = comdat.get(index, (None, None))
            code = ".text$" + name[len(prefix):]
            if selection != 5:
                fail(f"{name} is COMDAT selection {selection}, not associative: an MSVC-mode "
                     "linker drops it and the function has no unwind entry")
            elif names[number - 1] != code:
                fail(f"{name} is associated with {names[number - 1]}, not {code}")

if not red:
    print("ok   constructors moved to .CRT$XCU and unwind data made associative")
sys.exit(red)
PY
