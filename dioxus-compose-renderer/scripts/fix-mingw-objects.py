"""Makes MinGW-built COFF objects mean the same thing to an MSVC-mode linker, in place.

Two conventions differ, and both are fixed here:

1. Static constructors. MinGW puts them in .ctors and its own startup code runs them. The
   MSVC runtime runs the function pointers it finds in .CRT$XCU. Both are arrays of
   pointers to void(void) functions, so the section is renamed. Both names fit the 8-byte
   short name field.

2. Unwind data. For a function in COMDAT section .text$NAME, the MinGW toolchain emits its
   unwind data as .pdata$NAME and .xdata$NAME, each a COMDAT of its own ("select any"), and
   the MinGW linker pairs them with the code by name. An MSVC-mode linker follows the COFF
   rules instead, sees two sections nothing refers to, and drops them. The function then has
   no entry in the exception table, and an exception passing through it is unwound as if it
   were a leaf function: the return address is read from the wrong slot and the unwinder
   loops. Marking them associative to .text$NAME (selection 5) says, in the form the
   format defines, that they live and die with that code.
"""
import struct, sys

CTORS, CRT = b".ctors\0\0", b".CRT$XCU"
SELECT_ASSOCIATIVE = 5

def section_name(buf, at, strtab):
    raw = bytes(buf[at:at + 8])
    name = raw.rstrip(b"\0").decode(errors="replace")
    if name.startswith("/"):
        o = int(name[1:]); e = buf.index(b"\0", strtab + o)
        name = bytes(buf[strtab + o:e]).decode()
    return name

def patch(buf, base):
    machine, nsec, _, symptr, nsym, optsize, _ = struct.unpack_from("<HHIIIHH", buf, base)
    if machine != 0x8664:
        return 0, 0
    strtab = base + symptr + nsym * 18
    table = base + 20 + optsize
    names = []
    ctors = 0
    for i in range(nsec):
        at = table + i * 40
        if bytes(buf[at:at + 8]) == CTORS:
            buf[at:at + 8] = CRT
            ctors += 1
        names.append(section_name(buf, at, strtab))
    text_index = {n[len(".text$"):]: i + 1 for i, n in enumerate(names) if n.startswith(".text$")}
    # Unwind data for the object's plain .text carries an empty suffix.
    if "" not in text_index and ".text" in names:
        text_index[""] = names.index(".text") + 1
    associated = 0
    i = 0
    while i < nsym:
        at = base + symptr + i * 18
        if bytes(buf[at:at + 8]) == CTORS:
            buf[at:at + 8] = CRT
        secnum = struct.unpack_from("<h", buf, at + 12)[0]
        storage = buf[at + 16]
        aux = buf[at + 17]
        if storage == 3 and aux and secnum > 0:
            name = names[secnum - 1]
            for prefix in (".pdata$", ".xdata$"):
                if name.startswith(prefix):
                    target = text_index.get(name[len(prefix):])
                    characteristics = struct.unpack_from("<I", buf, table + (secnum - 1) * 40 + 36)[0]
                    if target and characteristics & 0x1000:
                        a = at + 18
                        struct.pack_into("<H", buf, a + 12, target)
                        buf[a + 14] = SELECT_ASSOCIATIVE
                        associated += 1
        i += 1 + aux
    return ctors, associated

path = sys.argv[1]
buf = bytearray(open(path, "rb").read())
assert buf[:8] == b"!<arch>\n"
pos, ctors, associated = 8, 0, 0
while pos + 60 <= len(buf):
    size = int(buf[pos + 48:pos + 58].decode().strip())
    body = pos + 60
    if buf[body:body + 2] == b"\x64\x86":
        c, a = patch(buf, body)
        ctors += c; associated += a
    pos = body + size + (size & 1)
open(path, "wb").write(buf)
print(f"{path}: moved {ctors} .ctors sections, made {associated} unwind sections associative")
