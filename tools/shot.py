#!/usr/bin/env python3
"""Rasterise a scene document from tools/shot.lua to a PNG.

    python3 tools/shot.py <scene.json> <out.png> [width] [height]

One shape: the `soil_scene` document shot.lua writes — `{kind, schema, nodes}`,
with geometry in `transform.translation` / `transform.scale`. A preview also
resolves `rgb` and text `px`, which is what makes a palette previewable at all.
Anything without a colour is drawn in the fallback ink so a missing theme is
visible, not invisible.
"""
import json
import math
import os
import sys
from PIL import Image, ImageDraw, ImageFont

# The same document is rendered on Linux and on macOS, so the face is found rather
# than assumed. Anything the renderer can reach is fine: this is a preview, and a
# wrong-but-present face answers the geometry question the scene was drawn for.
FONT_CANDIDATES = (
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    # Arial Unicode carries the symbols the store draws (stars, arrows); plain
    # Arial turns them into tofu in the preview even when the scene is right.
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
)
FALLBACK = (230, 235, 245)
SUPERSAMPLE = 2


def font_path() -> str:
    for path in FONT_CANDIDATES:
        if os.path.exists(path):
            return path
    raise SystemExit("shot.py: no TrueType face found; add one to FONT_CANDIDATES")


def main() -> int:
    scene, out = sys.argv[1], sys.argv[2]
    prims = json.load(open(scene))["nodes"]
    width, height = (int(v) for v in (sys.argv[3:5] + [1200, 44])[:2])
    scale = SUPERSAMPLE
    image = Image.new("RGB", (width * scale, height * scale), (0, 0, 0))
    # Drawing onto RGB with an RGBA ink is what makes `opacity` blend; on an RGBA
    # image PIL stores the alpha and keeps the colour, so every translucent panel
    # came out at full strength.
    draw = ImageDraw.Draw(image, "RGBA")
    fonts: dict[int, ImageFont.FreeTypeFont] = {}

    def font(px: float):
        key = max(1, round(px * scale))
        if key not in fonts:
            fonts[key] = ImageFont.truetype(font_path(), key)
        return fonts[key]

    def x_of(unit: float) -> float:
        return (unit + 4.0) / 8.0 * width * scale

    def y_of(unit: float) -> float:
        return (4.0 - unit) / 8.0 * height * scale

    for prim in prims:
        colour = tuple(prim.get("rgb") or FALLBACK)
        alpha = int(round(255 * prim.get("opacity", 1.0)))
        transform = prim.get("transform") or {}
        translation = transform.get("translation") or [0.0, 0.0, 0.0]
        extent = transform.get("scale") or [1.0, 1.0, 1.0]
        if prim["type"] == "rect":
            left, top = x_of(translation[0]), y_of(translation[1])
            w = extent[0] / 8.0 * width * scale
            h = extent[1] / 8.0 * height * scale
            # A rotated rect is anchored at its top-left and turns about it, the
            # same rule the scene and Canvas use. The page is y-down and the
            # scene is y-up, so the sign flips.
            angle = (transform.get("rotation") or [0, 0, 0])[2]
            if abs(angle) > 1e-6:
                cos, sin = math.cos(-angle), math.sin(-angle)

                def corner(x, y):
                    return (left + x * cos - y * sin, top + x * sin + y * cos)

                draw.polygon([corner(0, 0), corner(w, 0), corner(w, h), corner(0, h)],
                             fill=colour + (alpha,))
            else:
                # `radius` is a length in logical pixels in the scene ABI — Canvas
                # feeds it straight to iced's border radius — so it scales like every
                # other length here. Treating it as a fraction of the rect turned
                # every panel into a pill.
                radius = min(prim.get("radius", 0) * scale, min(w, h) / 2)
                draw.rounded_rectangle([left, top, left + w, top + h], radius=max(0.0, radius), fill=colour + (alpha,))
        elif prim["type"] == "text":
            draw.text((x_of(translation[0]), y_of(translation[1])), prim.get("text", ""),
                      font=font(prim.get("px", 12)), fill=colour + (alpha,), anchor="la")

    image.resize((width, height), Image.LANCZOS).save(out)
    print(f"wrote {out} ({len(prims)} primitives)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
