# What the single-line strings would measure in the system UI font's default instance,
# with no shaping at all: the sum of hmtx advances at the font's default variation
# coordinates (opsz 28, wght 400, wdth 100), with no kerning and no tracking.
#
# This is not Parley. It needs no build, and it answers a narrower question: how far the
# untouched default instance is from what Chromium drew. Chromium sets the optical size from
# the font size and applies the font's tracking table; Parley 0.6 has no code that sets
# opsz (there is no "opsz" or "optical" anywhere in its source), and blitz-dom passes only
# font-variation-settings through. If the gap here is large, a shaper that leaves opsz at
# its default cannot match Chromium, whatever else it gets right.
#
# usage: python3 sf-default-instance-widths.py <experiment dir>
# writes: captured/sf-default-instance.json
import struct, json, sys
d = open('/System/Library/Fonts/SFNS.ttf', 'rb').read()
n = struct.unpack('>H', d[4:6])[0]
tabs = {}
for i in range(n):
    tag, cs, off, ln = struct.unpack('>4sIII', d[12 + 16 * i:28 + 16 * i])
    tabs[tag.decode()] = (off, ln)
upm = struct.unpack('>H', d[tabs['head'][0] + 18:tabs['head'][0] + 20])[0]
nhm = struct.unpack('>H', d[tabs['hhea'][0] + 34:tabs['hhea'][0] + 36])[0]
ho = tabs['hmtx'][0]


def adv(g):
    g = min(g, nhm - 1)
    return struct.unpack('>H', d[ho + 4 * g:ho + 4 * g + 2])[0]


co = tabs['cmap'][0]
nt = struct.unpack('>H', d[co + 2:co + 4])[0]
cmap = {}
for i in range(nt):
    pid, eid, off = struct.unpack('>HHI', d[co + 4 + 8 * i:co + 12 + 8 * i])
    so = co + off
    fmt = struct.unpack('>H', d[so:so + 2])[0]
    if fmt == 12:
        ng = struct.unpack('>I', d[so + 12:so + 16])[0]
        for k in range(ng):
            s, e, g = struct.unpack('>III', d[so + 16 + 12 * k:so + 28 + 12 * k])
            for c in range(s, e + 1):
                cmap.setdefault(c, g + c - s)
exp = sys.argv[1]
t = json.load(open(exp + '/captured/text.json'))
cases = {c['id']: c for c in json.load(open(exp + '/fixtures/text-cases.json'))}
rows = []
for r in t['results']:
    c = cases[r['id']]
    if c.get('width') or c.get('fontWeight') or r['id'].startswith('korean'):
        continue
    w = sum(adv(cmap.get(ord(ch), 0)) for ch in c['text']) * c['fontSize'] / upm
    lw = r['lines'][0]['w']
    rows.append({'id': r['id'], 'fontSize': c['fontSize'], 'chromium': round(lw, 3), 'defaultInstanceAdvanceSum': round(w, 3), 'diffPct': round(100 * (w - lw) / lw, 2)})
    print(f"{r['id']:16} size {c['fontSize']:>2} chromium {lw:8.3f}  default-instance hmtx sum {w:8.3f}  diff {w - lw:+7.3f} ({100 * (w - lw) / lw:+.2f}%)")
json.dump({'font': '/System/Library/Fonts/SFNS.ttf', 'unitsPerEm': upm, 'rows': rows}, open(exp + '/captured/sf-default-instance.json', 'w'), indent=1)
