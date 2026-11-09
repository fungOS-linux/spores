#!/usr/bin/env python3
"""Rasterise a scene document from tools/shot.lua to a PNG.

    python3 tools/shot.py <scene.json> <out.png> [width] [height]

Primitives carry resolved colour. Anything without one is drawn in the text ink
so a missing theme is visible, not invisible.
"""
import json
import sys
from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
FALLBACK = (230, 235, 245)
SUPERSAMPLE = 2


def main() -> int:
    scene, out = sys.argv[1], sys.argv[2]
    prims = json.load(open(scene))
    width, height = (int(v) for v in (sys.argv[3:5] + [1200, 44])[:2])
    scale = SUPERSAMPLE
    image = Image.new("RGBA", (width * scale, height * scale), (0, 0, 0, 255))
    draw = ImageDraw.Draw(image, "RGBA")
    fonts: dict[int, ImageFont.FreeTypeFont] = {}

    def font(px: float):
        key = max(1, round(px * scale))
        if key not in fonts:
            fonts[key] = ImageFont.truetype(FONT, key)
        return fonts[key]

    def x_of(unit: float) -> float:
        return (unit + 4.0) / 8.0 * width * scale

    def y_of(unit: float) -> float:
        return (4.0 - unit) / 8.0 * height * scale

    for prim in prims:
        colour = tuple(prim.get("rgb") or FALLBACK)
        alpha = int(round(255 * prim.get("opacity", 1.0)))
        if prim["type"] == "rect":
            left, top = x_of(prim["x"]), y_of(prim["y"])
            w = prim.get("w", 0) / 8.0 * width * scale
            h = prim.get("h", 0) / 8.0 * height * scale
            radius = min(prim.get("radius", 0) / 8.0 * min(width, height) * scale, min(w, h) / 2)
            draw.rounded_rectangle([left, top, left + w, top + h], radius=max(0.0, radius), fill=colour + (alpha,))
        elif prim["type"] == "text":
            draw.text((x_of(prim["x"]), y_of(prim["y"])), prim.get("text", ""),
                      font=font(prim.get("px", 12)), fill=colour + (alpha,), anchor="la")

    image.resize((width, height), Image.LANCZOS).save(out)
    print(f"wrote {out} ({len(prims)} primitives)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
