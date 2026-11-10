#!/usr/bin/env python3
"""Generates this port's own particle sprite sets.

The original Visuality draws its effects with its own Blender-made
art (BenjiSyS, MIT). This port ships its own pixel art instead —
simple 16x16 shapes drawn here — registered as named sprite sets the
engine's mod-particle loading picks up:

    assets/visuality_horizon/particles/<set>.json
        {"textures": ["visuality_horizon:<set>_<i>", ...]}
    assets/visuality_horizon/textures/particle/<set>_<i>.png

Named (mod) particles cannot be tinted by the engine, so colour
variants the original reaches by tinting one art set — the six slime
colours of the settings page, the three ore materials — are baked
into separate sets here. Run from the mod folder:

    python3 generate_sprites.py
"""

import json
import math
import os
import random
import struct
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets", "visuality_horizon")
SIZE = 16


def write_png(path, pixels):
    """pixels: SIZE rows of SIZE (r, g, b, a) tuples."""
    raw = b""
    for row in pixels:
        raw += b"\x00" + b"".join(
            struct.pack("BBBB", *(px if len(px) == 4 else (*px, 255))) for px in row
        )

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(png)


def canvas():
    return [[(0, 0, 0, 0)] * SIZE for _ in range(SIZE)]


def put(px, x, y, color):
    if 0 <= x < SIZE and 0 <= y < SIZE:
        px[y][x] = color


def blend(px, x, y, color):
    if 0 <= x < SIZE and 0 <= y < SIZE:
        r, g, b, a = px[y][x]
        sr, sg, sb, sa = color
        if a == 0:
            px[y][x] = color
        else:
            px[y][x] = (
                (r * a + sr * sa) // 255,
                (g * a + sg * sa) // 255,
                (b * a + sb * sa) // 255,
                min(255, a + sa),
            )


def shade(color, f, a=255):
    r, g, b = color
    return (max(0, min(255, int(r * f))), max(0, min(255, int(g * f))), max(0, min(255, int(b * f))), a)


# ---------------------------------------------------------------- shapes

def bone(px, base, angle, flip):
    """A bone shard: a short thick diagonal splinter with knobbly ends."""
    dark = shade(base, 0.72)
    light = shade(base, 1.12)
    cx, cy = 8.0, 8.0
    dx, dy = math.cos(angle), math.sin(angle)
    if flip:
        dy = -dy
    nx, ny = -dy, dx
    for t in range(-6, 7):
        w = 1.6 if abs(t) < 4 else 2.3  # knobbly ends
        for s in (-w, 0.0, w):
            x = int(round(cx + dx * t + nx * s))
            y = int(round(cy + dy * t + ny * s))
            put(px, x, y, dark if s == w else base)
    for t in range(-4, 3):
        x = int(round(cx + dx * t - nx * 0.8))
        y = int(round(cy + dy * t - ny * 0.8))
        put(px, x, y, light)


def feather(px, base):
    """A feather: a diagonal shaft with soft barbs."""
    dark = shade(base, 0.78)
    for t in range(11):
        x, y = 3 + t, 12 - t
        put(px, x, y, dark)
        for s in range(1, 4):
            if t in (0, 10):
                continue
            put(px, x - s, y - (s // 2), shade(base, 1.0 - 0.06 * s))
            put(px, x + (s // 2), y + s, shade(base, 0.94 - 0.05 * s))
    put(px, 3, 13, dark)
    put(px, 2, 14, dark)


def emerald(px, base):
    """A cut gem: a bright diamond with a darker base facet."""
    dark = shade(base, 0.62)
    light = shade(base, 1.25)
    rows = {4: (6, 9), 5: (5, 10), 6: (4, 11), 7: (4, 11), 8: (5, 10), 9: (6, 9), 10: (7, 8)}
    for y, (x0, x1) in rows.items():
        for x in range(x0, x1 + 1):
            put(px, x, y, dark if y >= 8 else base)
    for x, y in ((6, 5), (7, 5), (6, 6)):
        put(px, x, y, light)


def blob(px, base, radius, squash, seed):
    """A slime blob: a round jelly with a shaded foot and a highlight,
    squashed by `squash` (1.0 tall, lower = flatter)."""
    rng = random.Random(seed)
    dark = shade(base, 0.66)
    mid = shade(base, 0.85)
    cx, cy = 8.0, 9.5
    rx, ry = radius, radius * squash
    for y in range(SIZE):
        for x in range(SIZE):
            d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
            if d <= 1.0:
                c = base if d < 0.55 else mid
                if y > cy + ry * 0.45:
                    c = dark
                put(px, x, y, (*c[:3], 235))
    for _ in range(3):
        put(px, int(cx - rx * 0.35), int(cy - ry * 0.4), (255, 255, 255, 230))
    _ = rng


def star(px, base, reach, core=True):
    """A four-point sparkle: long vertical/horizontal rays, short
    diagonals, a white heart."""
    light = shade(base, 1.3)
    c = 7.5
    for d in range(0, int(reach) + 1):
        a = 255 - int(150 * d / max(reach, 1))
        col = (*base[:3], a)
        for x, y in ((c, c - d), (c, c + d), (c - d, c), (c + d, c)):
            put(px, int(round(x)), int(round(y)), col)
    for d in (1, 2):
        col = (*light[:3], 200)
        for sx in (-1, 1):
            for sy in (-1, 1):
                put(px, int(c + sx * d * 0.7), int(c + sy * d * 0.7), col)
    if core:
        put(px, 7, 7, (255, 255, 255, 255))
        put(px, 8, 7, (255, 255, 255, 255))
        put(px, 7, 8, (255, 255, 255, 255))
        put(px, 8, 8, (255, 255, 255, 255))


def wisp(px, base, phase, tall):
    """A soul wisp: a soft teardrop with a bright heart, leaning as
    `phase` runs 0..1, fading toward the end."""
    light = shade(base, 1.35)
    cx = 8.0 + math.sin(phase * math.pi * 2.0) * 1.2
    h = (11 if tall else 8) * (1.0 - 0.25 * phase)
    fade = 1.0 - 0.45 * phase
    for i in range(int(h)):
        t = i / max(h - 1, 1)
        y = 14 - i
        wdt = 2.6 * math.sin(math.pi * min(1.0, t * 1.15)) + 0.4
        x0 = cx - wdt / 2 + t * math.sin(phase * math.pi) * 1.5
        for x in range(int(x0), int(x0 + wdt) + 1):
            a = int(235 * fade * (1.0 - 0.35 * t))
            put(px, x, y, (*base[:3], a))
        put(px, int(cx + t * 1.2), y, (*light[:3], int(250 * fade)))


# ---------------------------------------------------------------- the sets

SETS = {}


def add(name, frames):
    SETS[name] = frames


BONE = (233, 226, 208)
WITHER = (84, 74, 66)
FEATHER = (238, 240, 244)
EMERALD = (46, 204, 96)

add("bone_a", [lambda px: bone(px, BONE, 0.6, False)])
add("bone_b", [lambda px: bone(px, BONE, -0.9, True)])
add("wither_bone_a", [lambda px: bone(px, WITHER, 0.6, False)])
add("wither_bone_b", [lambda px: bone(px, WITHER, -0.9, True)])
add("feather", [lambda px: feather(px, FEATHER)])
add("emerald", [lambda px: emerald(px, EMERALD)])

SLIME_COLORS = {
    "classic": (0x88, 0xFF, 0x79),
    "lime": (0xB8, 0xFF, 0x5A),
    "mint": (0x7A, 0xFF, 0xC4),
    "sky": (0x7A, 0xC8, 0xFF),
    "pink": (0xFF, 0x8F, 0xD0),
    "amber": (0xFF, 0xC8, 0x6A),
}
SLIME_RADII = {"small": 3.6, "medium": 5.4, "large": 6.8}
SQUASH = [1.0, 0.82, 0.68, 0.82, 1.0, 1.12]
for size, radius in SLIME_RADII.items():
    for page, rgb in SLIME_COLORS.items():
        frames = []
        for i, sq in enumerate(SQUASH):
            frames.append(lambda px, rgb=rgb, radius=radius, sq=sq, i=i: blob(px, rgb, radius, sq, i))
        add(f"slime_{size}_{page}", frames)

SPARKS = {
    "sparkle_amethyst": (178, 132, 255),
    "sparkle_gold": (255, 228, 138),
    "sparkle_diamond": (184, 246, 255),
    "sparkle_emerald": (168, 255, 184),
}
for name, rgb in SPARKS.items():
    frames = []
    for i in range(6):
        reach = [2, 4, 6, 5, 3, 2][i]
        frames.append(lambda px, rgb=rgb, reach=reach: star(px, rgb, reach))
    add(name, frames)

SOUL = (148, 226, 255)
add("soul_ambient", [lambda px, i=i: wisp(px, SOUL, i / 8.0, True) for i in range(8)])
add("soul_sprint", [lambda px, i=i: wisp(px, SOUL, i / 5.0, False) for i in range(5)])


def main():
    tex_dir = os.path.join(ROOT, "textures", "particle")
    set_dir = os.path.join(ROOT, "particles")
    os.makedirs(tex_dir, exist_ok=True)
    os.makedirs(set_dir, exist_ok=True)
    total = 0
    for name, frames in sorted(SETS.items()):
        textures = []
        for i, draw in enumerate(frames):
            px = canvas()
            draw(px)
            write_png(os.path.join(tex_dir, f"{name}_{i}.png"), px)
            textures.append(f"visuality_horizon:{name}_{i}")
            total += 1
        with open(os.path.join(set_dir, f"{name}.json"), "w") as f:
            json.dump({"textures": textures}, f, indent=2)
            f.write("\n")
    print(f"{len(SETS)} sets, {total} frames")


if __name__ == "__main__":
    main()
