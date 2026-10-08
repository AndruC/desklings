"""Validate src/sprites.rs and render a labelled sprite sheet.

Usage: python tools/sprite_sheet.py src/sprites.rs docs/sprites.png [NAME,NAME,...]
Requires Pillow. Species palettes are read from src/monster.rs; the FIXED colours below mirror
Surface::blit in src/main.rs, so keep those in sync by hand.
"""
import pathlib, re, sys
from PIL import Image, ImageDraw

if len(sys.argv) < 3:
    sys.exit(__doc__)
src = open(sys.argv[1], encoding='utf-8').read()
out = sys.argv[2]
only = set(sys.argv[3].split(',')) if len(sys.argv) > 3 else None

sprites = []
for m in re.finditer(r'pub const (\w+): Sprite = &\[(.*?)\];', src, re.S):
    rows = re.findall(r'"([^"]*)"', m.group(2))
    sprites.append((m.group(1), rows))

VALID = set('.kwbdahepronygcs')
ICONS = {'HEART', 'MEAT', 'ZZZ', 'BANG', 'STAR', 'UP', 'SWEAT', 'SHOE', 'DUMBBELL', 'SHIELD', 'BOOK', 'WAVE', 'SWORD'}
# Largest size each sprite may be (width, height); keep in sync with sprites_are_well_formed in monster.rs.
LIMITS = {**{n: (7, 7) for n in ICONS}, 'POOP': (8, 6)}
for n in ('EGG', 'BLIP_A', 'BLIP_B', 'BLOP', 'RAPTIN', 'FLUFFIN', 'SHELLBY'):
    LIMITS[n] = (16, 16)
for n in ('PYROREX', 'CRAGDON', 'GALEWING', 'MYSTIFUR', 'BULWARK', 'TIDECREST', 'GRUMBLOO'):
    LIMITS[n] = (20, 20)
for n in ('INFERNAX', 'SERAPHOX', 'TITANSHELL'):
    LIMITS[n] = (24, 24)

ok = True
for name, rows in sprites:
    if not rows:
        print(f'{name} is empty'); ok = False; continue
    if name not in LIMITS:
        print(f'{name} has no size limit in LIMITS; add one'); ok = False
    elif len(rows[0]) > LIMITS[name][0] or len(rows) > LIMITS[name][1]:
        print(f'{name} is {len(rows[0])}x{len(rows)}, over the {LIMITS[name][0]}x{LIMITS[name][1]} limit'); ok = False
    w = len(rows[0])
    for i, r in enumerate(rows):
        if len(r) != w:
            print(f'{name} row {i} len {len(r)} != {w}: "{r}"'); ok = False
        bad = set(r) - VALID
        if bad:
            print(f'{name} row {i} bad chars {bad}'); ok = False
if not ok:
    sys.exit(1)

# Species palettes come straight from the species table so the preview can't drift from the game.
monster_rs = open(pathlib.Path(sys.argv[1]).with_name('monster.rs'), encoding='utf-8').read()
PAL = {
    m.group(1).upper(): tuple(int(c, 16) for c in m.group(2, 3, 4))
    for m in re.finditer(r'name: "(\w+)",.*?pal: pal\((0x\w+), (0x\w+), (0x\w+)\)', monster_rs, re.S)
}
if len(PAL) != 16:
    sys.exit(f'expected 16 palettes in monster.rs, found {len(PAL)}')
FIXED = {'k': 0xFF1A1A2E, 'w': 0xFFFFFFFF, 'p': 0xFFFF8FA3, 'r': 0xFFE63946, 'o': 0xFFC8553D,
         'n': 0xFF8B5A2B, 'y': 0xFFFFD166, 'g': 0xFF57CC99, 'c': 0xFF4CC9F0, 's': 0xFFB8C0CC}

def rgb(c):
    return ((c >> 16) & 255, (c >> 8) & 255, c & 255)


def color(ch, pal, asleep=False):
    body, belly, acc = pal
    if ch == 'b': return body
    if ch == 'd': return belly
    if ch == 'a': return acc
    if ch == 'h': return body if asleep else 0xFFFFFFFF
    if ch == 'e': return body if asleep else FIXED['k']
    return FIXED.get(ch)


S = 6
items = [(n, r) for n, r in sprites if only is None or n in only]
cell_w, cell_h = 26 * S, 26 * S
cols = 6
monsters = [x for x in items if x[0] not in ICONS]
icons = [x for x in items if x[0] in ICONS]
mrows = (len(monsters) + cols - 1) // cols
irows = (len(icons) + 12 - 1) // 12
W = cols * cell_w * 2
H = mrows * (cell_h + 16) + irows * (12 * S + 16) + 20
img = Image.new('RGB', (W, H), (128, 128, 128))
d = ImageDraw.Draw(img)


def draw(name, rows, x0, y0, pal, asleep=False):
    for ry, row in enumerate(rows):
        for rx, ch in enumerate(row):
            c = color(ch, pal, asleep)
            if c is None: continue
            d.rectangle([x0 + rx * S, y0 + ry * S, x0 + rx * S + S - 1, y0 + ry * S + S - 1], fill=rgb(c))


for i, (name, rows) in enumerate(monsters):
    cx, cy = (i % cols) * cell_w * 2, (i // cols) * (cell_h + 16)
    key = name.split('_')[0]
    pal = PAL.get(key, (0xFF888888,) * 3)
    d.rectangle([cx, cy, cx + cell_w - 1, cy + cell_h + 15], fill=(12, 26, 20))
    d.rectangle([cx + cell_w, cy, cx + 2 * cell_w - 1, cy + cell_h + 15], fill=(235, 235, 228))
    d.text((cx + 4, cy + 2), f'{name} {len(rows[0])}x{len(rows)}', fill=(255, 255, 255))
    d.text((cx + cell_w + 4, cy + 2), 'asleep', fill=(0, 0, 0))
    h = len(rows)
    draw(name, rows, cx + S, cy + 16 + (24 - h) * S, pal)
    draw(name, rows, cx + cell_w + S, cy + 16 + (24 - h) * S, pal, asleep=True)

y = mrows * (cell_h + 16) + 10
for i, (name, rows) in enumerate(icons):
    x0, y0 = (i % 12) * 14 * S, y + (i // 12) * (12 * S + 16)
    d.text((x0, y0), name, fill=(0, 0, 0))
    bx, by = x0, y0 + 12
    for yy in range(11):
        for xx in range(11):
            corner = xx in (0, 10) and yy in (0, 10)
            edge = xx in (0, 10) or yy in (0, 10)
            if corner: continue
            c = FIXED['k'] if edge else 0xFFFFFFFF
            d.rectangle([bx + xx * S, by + yy * S, bx + xx * S + S - 1, by + yy * S + S - 1], fill=rgb(c))
    ih = len(rows)
    draw(name, rows, bx + 2 * S, by + (1 + (9 - ih) // 2) * S, (0, 0, 0))

img.save(out)
print('ok', len(sprites), 'sprites')
