"""Preview renderer for the Spore's Lua view.

Soil's Lua host is not on this branch, so this stands in for it: it lays out the
tree `ui/sheet.lua` returns and draws it. The real host hands the same tree to
`soil-layout` and draws it as a Canvas scene; nothing here decides what the sheet
*is*, only where its nodes land and which theme token becomes which colour.

Usage: python3 spores/sheets/tools/preview.py <tree.json> <out.png> [light|dark]
"""
import json
import sys

from PIL import Image, ImageDraw, ImageFont

THEMES = {
    "light": {
        "bg": "#FFFFFF", "surface": "#F3F6F8", "text": "#1B2A33", "primary": "#102B3A",
        "muted": "#6B7A86", "danger": "#A61919", "info": "#1739B0", "line": "#D5DDE3",
    },
    "dark": {
        "bg": "#0E1519", "surface": "#172128", "text": "#D7E0E6", "primary": "#F3F7F9",
        "muted": "#8394A0", "danger": "#FF7B7B", "info": "#8AB4FF", "line": "#27343C",
    },
}

FONT = "/System/Library/Fonts/Menlo.ttc"
_fonts = {}


def font(size):
    key = int(round(size))
    if key not in _fonts:
        _fonts[key] = ImageFont.truetype(FONT, key)
    return _fonts[key]


def measure(text, size):
    if not text:
        return 0
    box = font(size).getbbox(text)
    return box[2] - box[0]


def props_of(node):
    return node.get("props") or {}


def width_of(node):
    props = props_of(node)
    if node["kind"] in ("text", "button"):
        return props.get("width") or measure(node.get("text", ""), props.get("size", 12))
    if node["kind"] == "rule":
        return props.get("width") or 1
    if props.get("width"):
        return props["width"]
    children = node.get("children") or []
    if node["kind"] == "row":
        return sum(width_of(child) for child in children) + props.get("gap", 0) * max(0, len(children) - 1)
    return max((width_of(child) for child in children), default=0)


def height_of(node):
    props = props_of(node)
    if props.get("height"):
        return props["height"]
    if node["kind"] in ("text", "button"):
        return props.get("size", 12) * 1.4
    if node["kind"] == "rule":
        return 1
    children = node.get("children") or []
    gap = props.get("gap", 0)
    if node["kind"] == "column":
        return sum(height_of(child) for child in children) + gap * max(0, len(children) - 1)
    return max((height_of(child) for child in children), default=0)


def layout(node, x, y, w, h, boxes):
    props = props_of(node)
    children = node.get("children") or []
    gap = props.get("gap", 0)
    boxes.append((node, x, y, w, h))
    if not children:
        return
    if node["kind"] == "column":
        cursor = y
        for child in children:
            child_height = height_of(child)
            layout(child, x, cursor, w, child_height, boxes)
            cursor += child_height + gap
        return
    fixed = [width_of(child) for child in children]
    flex = [child for child in children if props_of(child).get("flex_grow")]
    remaining = w - sum(fixed) - gap * max(0, len(children) - 1)
    cursor = x
    for index, child in enumerate(children):
        child_width = max(width_of(child), remaining / len(flex)) if child in flex and remaining > 0 else fixed[index]
        child_height = props.get("height") or height_of(child)
        layout(child, cursor, y, child_width, child_height, boxes)
        cursor += child_width + gap


def draw(tree, boxes, theme, out):
    boxes.sort(key=lambda box: box[3] * box[4], reverse=True)
    width = int(props_of(tree).get("width", 1020)) + 56
    height = int(props_of(tree).get("height", 400)) + 56
    image = Image.new("RGB", (width, height), theme["bg"])
    paint = ImageDraw.Draw(image)

    for node, x, y, w, h in boxes:
        props = props_of(node)
        fill = props.get("fill")
        if fill:
            paint.rectangle([x, y, x + w - 1, y + h - 1], fill=theme.get(fill, theme["surface"]))

    for node, x, y, w, h in boxes:
        props = props_of(node)
        if node["kind"] == "rule":
            colour = theme["line"] if props.get("ink", "line") == "line" else theme.get(props.get("ink"), theme["muted"])
            if h <= 2:
                paint.line([(x, y + 0.5), (x + w, y + 0.5)], fill=colour, width=1)
            else:
                paint.line([(x + 0.5, y), (x + 0.5, y + h)], fill=colour, width=1)
            continue
        if node["kind"] not in ("text", "button"):
            continue
        text = node.get("text") or ""
        size = props.get("size", 12)
        pad = props.get("pad", 0)
        ink = props.get("ink", "text")
        colour = theme.get(ink, theme["text"])
        box = font(size).getbbox(text)
        text_width = box[2] - box[0]
        text_height = box[3] - box[1]
        if props.get("align") == "right":
            tx = x + w - pad - text_width - box[0]
        elif props.get("align") == "center":
            tx = x + (w - text_width) / 2 - box[0]
        else:
            tx = x + pad - box[0]
        ty = y + (h - text_height) / 2 - box[1]
        paint.text((tx, ty), text, font=font(size), fill=colour)

    image.save(out)
    return image.size


def main():
    tree = json.load(open(sys.argv[1]))
    out = sys.argv[2]
    theme = THEMES[sys.argv[3] if len(sys.argv) > 3 else "light"]
    boxes = []
    layout(tree, 28, 28, props_of(tree).get("width", 1020), props_of(tree).get("height", 400), boxes)
    size = draw(tree, boxes, theme, out)
    print(f"wrote {out} {size[0]}x{size[1]} nodes={len(boxes)}")


if __name__ == "__main__":
    main()
