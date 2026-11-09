#!/usr/bin/env python3
"""Generates Eating Animation's frame assets for the Kestrel port:
one item definition per covered food/drink under
`assets/eatinganimation/items/`, and four frame models per item
under `assets/eatinganimation/models/item/`.

The frames are this port's own work, authored for this script:
each frame model is one or two flat cuboids sampling a cropped
rectangle of the item's own vanilla texture — a bite taken out of
the top-right corner for foods, the top emptied away for drinks —
so no texture is copied and none is shipped. The definition does
the animating: `condition(using_item)` -> `range_dispatch` on
`use_duration` (elapsed ticks), the resolver's own machinery, so
the code mod only registers one model rule per item.

Run from anywhere; paths resolve against this file's folder.
Re-running rewrites the same bytes (the Rust unit tests pin the
table, the thresholds and the file set).
"""

import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ITEMS = HERE / "assets" / "eatinganimation" / "items"
MODELS = HERE / "assets" / "eatinganimation" / "models" / "item"

# (item id, use duration in ticks, kind, layer0 texture)
# Durations are vanilla's: foods eat in 32 ticks, dried kelp in
# 16, the honey bottle drinks in 40. Kinds: "eat" (corner bites),
# "drink" (empties from the top), "potion" (a drink with the
# vanilla potion's two layers: bottle, then tinted liquid).
FOODS = [
    ("apple", 32, "eat", "minecraft:item/apple"),
    ("golden_apple", 32, "eat", "minecraft:item/golden_apple"),
    ("enchanted_golden_apple", 32, "eat", "minecraft:item/golden_apple"),
    ("melon_slice", 32, "eat", "minecraft:item/melon_slice"),
    ("sweet_berries", 32, "eat", "minecraft:item/sweet_berries"),
    ("glow_berries", 32, "eat", "minecraft:item/glow_berries"),
    ("chorus_fruit", 32, "eat", "minecraft:item/chorus_fruit"),
    ("carrot", 32, "eat", "minecraft:item/carrot"),
    ("golden_carrot", 32, "eat", "minecraft:item/golden_carrot"),
    ("potato", 32, "eat", "minecraft:item/potato"),
    ("baked_potato", 32, "eat", "minecraft:item/baked_potato"),
    ("poisonous_potato", 32, "eat", "minecraft:item/poisonous_potato"),
    ("beetroot", 32, "eat", "minecraft:item/beetroot"),
    ("dried_kelp", 16, "eat", "minecraft:item/dried_kelp"),
    ("beef", 32, "eat", "minecraft:item/beef"),
    ("cooked_beef", 32, "eat", "minecraft:item/cooked_beef"),
    ("porkchop", 32, "eat", "minecraft:item/porkchop"),
    ("cooked_porkchop", 32, "eat", "minecraft:item/cooked_porkchop"),
    ("mutton", 32, "eat", "minecraft:item/mutton"),
    ("cooked_mutton", 32, "eat", "minecraft:item/cooked_mutton"),
    ("chicken", 32, "eat", "minecraft:item/chicken"),
    ("cooked_chicken", 32, "eat", "minecraft:item/cooked_chicken"),
    ("rabbit", 32, "eat", "minecraft:item/rabbit"),
    ("cooked_rabbit", 32, "eat", "minecraft:item/cooked_rabbit"),
    ("cod", 32, "eat", "minecraft:item/cod"),
    ("cooked_cod", 32, "eat", "minecraft:item/cooked_cod"),
    ("salmon", 32, "eat", "minecraft:item/salmon"),
    ("cooked_salmon", 32, "eat", "minecraft:item/cooked_salmon"),
    ("tropical_fish", 32, "eat", "minecraft:item/tropical_fish"),
    ("pufferfish", 32, "eat", "minecraft:item/pufferfish"),
    ("bread", 32, "eat", "minecraft:item/bread"),
    ("cookie", 32, "eat", "minecraft:item/cookie"),
    ("pumpkin_pie", 32, "eat", "minecraft:item/pumpkin_pie"),
    ("rotten_flesh", 32, "eat", "minecraft:item/rotten_flesh"),
    ("spider_eye", 32, "eat", "minecraft:item/spider_eye"),
    ("mushroom_stew", 32, "eat", "minecraft:item/mushroom_stew"),
    ("rabbit_stew", 32, "eat", "minecraft:item/rabbit_stew"),
    ("beetroot_soup", 32, "eat", "minecraft:item/beetroot_soup"),
    ("suspicious_stew", 32, "eat", "minecraft:item/suspicious_stew"),
    ("milk_bucket", 32, "drink", "minecraft:item/milk_bucket"),
    ("honey_bottle", 40, "drink", "minecraft:item/honey_bottle"),
    ("ominous_bottle", 32, "drink", "minecraft:item/ominous_bottle"),
    ("potion", 32, "potion", "minecraft:item/potion"),
]

POTION_OVERLAY = "minecraft:item/potion_overlay"

# The stage boundaries, as fractions of the item's use duration:
# stage 1 from the first tick, then stages 2-4 at these.
STAGE_FRACTIONS = [0.35, 0.60, 0.82]
# Foods: the bite notch (width, height in texture pixels) taken
# out of the sprite's top-right corner, per stage.
EAT_NOTCHES = [(5, 5), (8, 8), (11, 11), (13, 13)]
# Drinks: how much of the sprite's top is emptied away, per stage.
DRINK_CUTS = [3, 6, 9, 12]


def rects(kind: str, stage: int):
    """The texture rectangles (x0, y0, x1, y1; y down) a stage
    still shows."""
    if kind == "eat":
        w, h = EAT_NOTCHES[stage]
        return [(0, 0, 16 - w, 16), (16 - w, h, 16, 16)]
    cut = DRINK_CUTS[stage]
    return [(0, cut, 16, 16)]


def element(rect, z0, z1, slot, tintindex=None):
    x0, y0, x1, y1 = rect

    def face(uv):
        f = {"uv": uv, "texture": slot}
        if tintindex is not None:
            f["tintindex"] = tintindex
        return f

    return {
        "from": [x0, 16 - y1, z0],
        "to": [x1, 16 - y0, z1],
        "faces": {
            "north": face([x1, y0, x0, y1]),
            "south": face([x0, y0, x1, y1]),
            "west": face([x0, y0, x0 + 1, y1]),
            "east": face([x1 - 1, y0, x1, y1]),
            "up": face([x0, y0, x1, y0 + 1]),
            "down": face([x0, y1 - 1, x1, y1]),
        },
    }


def frame_model(kind, texture, stage):
    elements = []
    for rect in rects(kind, stage):
        if kind == "potion":
            # The bottle behind, the tinted liquid in front of it.
            elements.append(element(rect, 7.5, 8.3, "#layer0"))
            elements.append(element(rect, 8.3, 8.5, "#layer1", tintindex=0))
        else:
            elements.append(element(rect, 7.5, 8.5, "#layer0"))
    textures = {"layer0": texture}
    if kind == "potion":
        textures["layer1"] = POTION_OVERLAY
    return {
        "parent": "minecraft:item/generated",
        "textures": textures,
        "elements": elements,
    }


def model_leaf(model_id, potion):
    leaf = {"type": "minecraft:model", "model": model_id}
    if potion:
        # The vanilla potion definition's own tint: the liquid
        # takes the stack's potion contents colour.
        leaf["tints"] = [{"type": "minecraft:potion", "default": 16777215}]
    return leaf


def definition(food, duration, kind):
    potion = kind == "potion"
    thresholds = [round(f * duration) for f in STAGE_FRACTIONS]
    entries = [
        {
            "threshold": t,
            "model": model_leaf(f"eatinganimation:item/{food}_{stage}", potion),
        }
        for t, stage in zip(thresholds, (2, 3, 4))
    ]
    return {
        "model": {
            "type": "minecraft:condition",
            "property": "minecraft:using_item",
            "on_true": {
                "type": "minecraft:range_dispatch",
                "property": "minecraft:use_duration",
                "entries": entries,
                "fallback": model_leaf(f"eatinganimation:item/{food}_1", potion),
            },
            "on_false": model_leaf(f"minecraft:item/{food}", potion),
        }
    }


def dump(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    for food, duration, kind, texture in FOODS:
        dump(ITEMS / f"{food}.json", definition(food, duration, kind))
        for stage in range(4):
            dump(
                MODELS / f"{food}_{stage + 1}.json",
                frame_model(kind, texture, stage),
            )
    print(f"{len(FOODS)} definitions, {len(FOODS) * 4} frame models")


if __name__ == "__main__":
    main()
