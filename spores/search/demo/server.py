# Quicklaunch demo server
#
# A browser harness for the `spores/search` launcher. It serves the same
# snapshot shape that `ui/launcher.lua` consumes and applies the same ranking
# order as `spores/search/src/lib.rs`:
#
#     exact alias > alias prefix > title > subtitle > frecency
#
# This is a harness, not Canvas. Canvas owns every pixel in the real system and
# will render `launcher.lua` itself; until it can be booted here, this page
# makes the *behaviour* observable and refreshable in a browser.
#
#   GET /                    the launcher page
#   GET /api/search?q=       ranked hits, each carrying `art`
#   GET /icon/<id>.png       the app bundle's own icon, resolved by the host
#   GET /icon/<id>.svg       generated artwork for hits that ship none
#
#   python3 server.py --port 8791 --applications apps.json
#
import argparse
import hashlib
import json
import math
import os
import plistlib
import shutil
import subprocess
import tempfile
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, quote, unquote, urlparse

DEFAULT_APPLICATIONS = [
    {"bundle_id": "com.apple.Safari", "name": "Safari", "subtitle": "com.apple.Safari", "icon": "S"},
    {"bundle_id": "com.google.Chrome", "name": "Google Chrome", "subtitle": "com.google.Chrome", "icon": "C"},
    {"bundle_id": "com.tinyspeck.slackmacgap", "name": "Slack", "subtitle": "Chat", "icon": "#"},
    {"bundle_id": "com.apple.Terminal", "name": "Terminal", "subtitle": "Utilities", "icon": ">"},
    {"bundle_id": "com.raycast.macos", "name": "Raycast", "subtitle": "Launcher", "icon": "R"},
    {"bundle_id": "org.fungos.soil", "name": "Soil", "subtitle": "Spore runtime", "icon": "o"},
    {"bundle_id": "org.fungos.canvas", "name": "Canvas", "subtitle": "Compositor", "icon": "C"},
]

DEFAULT_FILES = [
    {"id": "vfs:/Documents/field-notes.md", "title": "field-notes.md", "subtitle": "Documents", "kind": "file"},
    {"id": "vfs:/Documents/launcher-design.md", "title": "launcher-design.md", "subtitle": "Documents", "kind": "file"},
    {"id": "vfs:/src/soil/wit/soil-search.wit", "title": "soil-search.wit", "subtitle": "soil/wit", "kind": "file"},
]

# One entry per remaining `soil:search` kind, so every thumbnail shape is
# visible without inventing vocabulary the protocol does not have.
DEFAULT_OTHERS = [
    {"id": "action:dark-mode", "title": "Toggle Dark Mode", "subtitle": "Appearance", "kind": "action"},
    {"id": "spore:dev.fungos.dragshift", "title": "Dragshift", "subtitle": "Installed Spore", "kind": "spore"},
    {"id": "principal:avery", "title": "Avery Wagar", "subtitle": "Future Present Labs", "kind": "principal"},
    {"id": "repo:fungos/soil", "title": "soil", "subtitle": "~/src/soil \u00b7 main", "kind": "repository"},
    {"id": "service:soil-search-web", "title": "soil-search-web", "subtitle": "127.0.0.1:8790", "kind": "service"},
    {"id": "task:dock-96j", "title": "dock-96j \u00b7 Launcher v0", "subtitle": "in progress", "kind": "task"},
    {"id": "record:nightshade/coherence", "title": "Coherence", "subtitle": "Records \u00b7 nightshade", "kind": "recordentry"},
    {"id": "peer:home-pi", "title": "home-pi", "subtitle": "Mycelium peer", "kind": "peer"},
]

# Frecency and aliases would live in soil:appdata in the real Spore. The
# frequencies are spread across kinds so an empty query opens on a
# representative list instead of seven applications in a row.
ALIASES = {"c": "com.google.Chrome", "t": "com.apple.Terminal", "s": "com.apple.Safari"}
FRECENCY = {
    "com.apple.Terminal": {"uses": 9, "last": time.time() - 300},
    "com.google.Chrome": {"uses": 7, "last": time.time() - 900},
    "action:dark-mode": {"uses": 6, "last": time.time() - 3_600},
    "com.apple.Safari": {"uses": 5, "last": time.time() - 7_200},
    "principal:avery": {"uses": 4, "last": time.time() - 86_400},
    "spore:dev.fungos.dragshift": {"uses": 3, "last": time.time() - 172_800},
    "repo:fungos/soil": {"uses": 2, "last": time.time() - 604_800},
    "service:soil-search-web": {"uses": 1, "last": time.time() - 1_209_600},
}


def load(path):
    if not path or not os.path.exists(path):
        return None
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
    return payload.get("applications", payload)


# ---- real logos -------------------------------------------------------------
#
# Turning a bundle id into artwork is the *host's* job. `soil:search` carries a
# monogram (`hit.icon`), never an image, so this harness plays host: it reads the
# installed bundle's `.icns`, bakes a PNG, and hands the browser the bytes. The
# day Canvas owns the asset pipeline, the same lookup moves into soil's
# application index and reaches the view through `canvas.image` instead.

ICON_PIXELS = "128"
ICON_CACHE = os.path.join(tempfile.gettempdir(), "quicklaunch-icons")
BUNDLE_ROOTS = (
    "/Applications",
    "/Applications/Utilities",
    "/System/Applications",
    "/System/Applications/Utilities",
    os.path.expanduser("~/Applications"),
)


def bundle_index():
    """bundle id -> (bundle path, icon file, display name). Executes nothing."""
    index = {}
    for root in BUNDLE_ROOTS:
        try:
            entries = sorted(os.scandir(root), key=lambda entry: entry.name)
        except OSError:
            continue
        for entry in entries:
            if not entry.name.endswith(".app") or not entry.is_dir():
                continue
            try:
                with open(os.path.join(entry.path, "Contents", "Info.plist"), "rb") as handle:
                    info = plistlib.load(handle)
            except (OSError, ValueError, plistlib.InvalidFileException):
                continue
            bundle_id = info.get("CFBundleIdentifier")
            if not isinstance(bundle_id, str) or bundle_id in index:
                continue
            name = (info.get("CFBundleDisplayName") or info.get("CFBundleName")
                    or entry.name[: -len(".app")])
            icon = info.get("CFBundleIconFile") or info.get("CFBundleIconName") or "AppIcon"
            if not icon.endswith(".icns"):
                icon += ".icns"
            index[bundle_id] = (entry.path, icon, name)
    return index


def installed_applications(bundles, limit):
    """The machine's own bundles, shaped the way `soil:search` reports them."""
    applications = [
        {
            "bundle_id": bundle_id,
            "name": name,
            # Where the bundle lives is the folder a user knows it by, and it is
            # derived from the path rather than invented.
            "subtitle": os.path.basename(os.path.dirname(path)),
            "icon": name[:1].upper(),
        }
        for bundle_id, (path, _icon, name) in bundles.items()
    ]
    applications.sort(key=lambda app: app["name"].lower())
    return applications[:limit]


def icon_png(bundles, bundle_id, cache):
    """The bundle's icon as a cached PNG, or None when it ships no artwork."""
    if bundle_id in cache:
        return cache[bundle_id]
    cache[bundle_id] = None  # one attempt per bundle, even when it misses
    known = bundles.get(bundle_id)
    if not known or shutil.which("sips") is None:
        return None
    source = os.path.join(known[0], "Contents", "Resources", known[1])
    if not os.path.exists(source):
        return None
    os.makedirs(ICON_CACHE, exist_ok=True)
    target = os.path.join(ICON_CACHE, bundle_id + ".png")
    result = subprocess.run(
        ["sips", "-s", "format", "png", "--resampleHeightWidth", ICON_PIXELS, ICON_PIXELS,
         source, "--out", target],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=30,
        check=False,
    )
    if result.returncode != 0 or not os.path.exists(target):
        return None
    cache[bundle_id] = target
    return target


# ---- generated artwork ------------------------------------------------------
#
# An app has a brand and its artwork should be its own; every other kind is a
# category and should look like the category. So applications hash their id to a
# hue (or use their real icon) while files read their extension and the rest key
# off the kind, which makes a row greppable by shape at a glance.

FONT = "-apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif"

KIND_HUE = {
    "action": 18, "spore": 288, "principal": 198, "repository": 138,
    "service": 222, "task": 152, "recordentry": 264, "peer": 6,
}

EXT_HUE = {
    ".md": 210, ".rs": 22, ".wit": 282, ".lua": 224, ".toml": 38, ".json": 46,
    ".py": 204, ".ts": 190, ".png": 300, ".pdf": 4, ".txt": 205, ".sh": 130,
}


def _hue(key):
    return hashlib.sha256(key.encode("utf-8")).digest()[0] * 360 // 256


def _initials(title):
    words = [word for word in title.replace("-", " ").replace("_", " ").split() if word]
    if not words:
        return "?"
    if len(words) == 1:
        return words[0][:1].upper()
    return (words[0][:1] + words[1][:1]).upper()


def _svg(inner):
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"'
        ' width="128" height="128">' + inner + "</svg>"
    )


def _defs(hue):
    return (
        "<defs>"
        f'<linearGradient id="g" x1="0" y1="0" x2=".35" y2="1">'
        f'<stop offset="0" stop-color="hsl({hue}, 92%, 68%)"/>'
        f'<stop offset="1" stop-color="hsl({(hue + 28) % 360}, 78%, 42%)"/>'
        "</linearGradient>"
        '<linearGradient id="gloss" x1="0" y1="0" x2="0" y2="1">'
        '<stop offset="0" stop-color="#fff" stop-opacity=".42"/>'
        '<stop offset="1" stop-color="#fff" stop-opacity="0"/>'
        "</linearGradient>"
        "</defs>"
    )


def _squircle(hue):
    return (
        '<rect x="4" y="4" width="120" height="120" rx="30" fill="url(#g)"/>'
        '<path d="M4 34a30 30 0 0 1 30-30h60a30 30 0 0 1 30 30v26H4z" fill="url(#gloss)"/>'
        '<rect x="4.5" y="4.5" width="119" height="119" rx="29.5" fill="none"'
        ' stroke="#fff" stroke-opacity=".22"/>'
    )


def _glyph(kind, initials):
    """White-on-badge foreground: one silhouette per kind."""
    white = 'fill="#fff" fill-opacity=".94"'
    if kind == "principal":
        return (f'<circle cx="64" cy="52" r="18" {white}/>'
                f'<path d="M30 104a34 30 0 0 1 68 0z" {white}/>')
    if kind == "action":
        return f'<path d="M72 16 38 72h22l-6 40 38-58H70z" {white}/>'
    if kind == "spore":
        return (f'<circle cx="64" cy="64" r="15" {white}/>'
                '<circle cx="64" cy="26" r="7" fill="#fff" fill-opacity=".62"/>'
                '<circle cx="95" cy="82" r="7" fill="#fff" fill-opacity=".62"/>'
                '<circle cx="33" cy="82" r="7" fill="#fff" fill-opacity=".62"/>')
    if kind == "repository":
        return ('<path d="M46 38v54" stroke="#fff" stroke-opacity=".94" stroke-width="8"'
                ' stroke-linecap="round"/>'
                '<path d="M46 92c0-26 14-38 38-38" stroke="#fff" stroke-opacity=".94"'
                ' stroke-width="8" stroke-linecap="round" fill="none"/>'
                f'<circle cx="46" cy="34" r="10" {white}/>'
                f'<circle cx="46" cy="96" r="10" {white}/>'
                f'<circle cx="88" cy="54" r="10" {white}/>')
    if kind == "service":
        return ('<path d="M38 96a34 34 0 0 1 34-34" stroke="#fff" stroke-opacity=".5"'
                ' stroke-width="8" stroke-linecap="round" fill="none"/>'
                '<path d="M38 96a58 58 0 0 1 58-58" stroke="#fff" stroke-opacity=".8"'
                ' stroke-width="8" stroke-linecap="round" fill="none"/>'
                f'<circle cx="38" cy="96" r="10" {white}/>')
    if kind == "task":
        return ('<path d="M34 66l20 20 40-46" stroke="#fff" stroke-opacity=".95"'
                ' stroke-width="13" stroke-linecap="round" stroke-linejoin="round"'
                ' fill="none"/>')
    if kind == "recordentry":
        return ('<rect x="28" y="46" width="72" height="60" rx="10" '
                'fill="#fff" fill-opacity=".38"/>'
                f'<rect x="36" y="30" width="72" height="60" rx="10" {white}/>'
                '<rect x="48" y="46" width="40" height="7" rx="3.5"'
                ' fill="#0b0e14" fill-opacity=".35"/>'
                '<rect x="48" y="60" width="30" height="7" rx="3.5"'
                ' fill="#0b0e14" fill-opacity=".22"/>')
    if kind == "peer":
        return (f'<circle cx="42" cy="64" r="16" {white}/>'
                f'<circle cx="88" cy="64" r="16" {white}/>'
                '<path d="M58 64h14" stroke="#fff" stroke-opacity=".9" stroke-width="8"'
                ' stroke-linecap="round"/>')
    return f'<text x="64" y="64" text-anchor="middle" dominant-baseline="central"' \
           f' font-family="{FONT}" font-size="54" font-weight="700"' \
           f' fill="#fff" fill-opacity=".96">{initials}</text>'


def _file_art(hue, label):
    return (
        '<path d="M28 6h46l26 26v90a6 6 0 0 1-6 6H28a6 6 0 0 1-6-6V12a6 6 0 0 1 6-6z"'
        ' fill="url(#g)"/>'
        '<path d="M74 6l26 26H76a2 2 0 0 1-2-2z" fill="#fff" fill-opacity=".55"/>'
        '<rect x="30" y="74" width="60" height="30" rx="7" fill="#0b0e14" fill-opacity=".42"/>'
        f'<text x="60" y="89" text-anchor="middle" dominant-baseline="central"'
        f' font-family="{FONT}" font-size="17" font-weight="700" letter-spacing="1"'
        f' fill="#fff" fill-opacity=".95">{label}</text>'
    )


def art_svg(kind, key, title):
    """Deterministic, colourful artwork for a hit that ships no icon of its own."""
    kind = (kind or "file").lower()
    if kind == "file":
        extension = os.path.splitext(title)[1].lower()
        hue = EXT_HUE.get(extension, 212)
        label = (extension.lstrip(".") or "file")[:3].upper()
        return _svg(_defs(hue) + _file_art(hue, label))
    hue = KIND_HUE.get(kind, _hue(key))
    return _svg(_defs(hue) + _squircle(hue) + _glyph(kind, _initials(title)))


def score(entry, query, alias_table, frecency):
    """Mirror spores/search/src/lib.rs: alias, then title, then frecency."""
    query = query.lower().strip()
    title = entry["title"].lower()
    subtitle = (entry.get("subtitle") or "").lower()

    alias = None
    for shorthand, target in alias_table.items():
        if target == entry["id"]:
            alias = shorthand
            break

    rank = 100_000
    if alias is not None:
        if alias == query:
            rank += 1_000_000
        elif alias.startswith(query):
            rank += 500_000

    if query:
        if title == query:
            rank += 200_000
        elif title.startswith(query):
            rank += 120_000
        elif query in title:
            rank += 60_000
        elif query in subtitle:
            rank += 10_000
        else:
            # Subsequence match keeps "gch" finding "Google Chrome".
            cursor = 0
            for char in query:
                found = title.find(char, cursor)
                if found < 0:
                    return None
                cursor = found + 1
            rank += 2_000

    freq = frecency.get(entry["id"])
    if freq:
        age_days = (time.time() - freq["last"]) / 86_400
        recency = 1.0 / (1.0 + min(max(age_days, 1.0), 1000.0))
        rank += int(recency * math.log1p(freq["uses"]) * 10_000)
    return rank


def search(entries, query, alias_table=ALIASES, frecency=FRECENCY):
    if not query.strip():
        ranked = sorted(entries, key=lambda e: -frecency.get(e["id"], {}).get("uses", 0))
        return [(entry, 100_000) for entry in ranked[:8]]
    scored = []
    for entry in entries:
        value = score(entry, query, alias_table, frecency)
        if value is not None:
            scored.append((entry, value))
    scored.sort(key=lambda pair: -pair[1])
    return scored[:12]


class Handler(BaseHTTPRequestHandler):
    applications = DEFAULT_APPLICATIONS
    files = DEFAULT_FILES
    others = DEFAULT_OTHERS
    bundles = {}
    icons = {}

    def log_message(self, *_args):
        pass

    def catalog(self):
        """What this machine can find, each hit carrying the art that fits it."""
        entries = []
        for app in self.applications:
            entries.append({
                "id": app["bundle_id"],
                "title": app["name"],
                "subtitle": app.get("subtitle", ""),
                "kind": "application",
                "source": "host:applications",
                "icon": app.get("icon") or app["name"][:1].upper(),
            })
        for item in self.files:
            entries.append({
                "id": item["id"],
                "title": item["title"],
                "subtitle": item.get("subtitle", ""),
                "kind": item.get("kind", "file"),
                "source": "vfs",
                "icon": item["title"][:1].upper(),
            })
        for item in self.others:
            entries.append({
                "id": item["id"],
                "title": item["title"],
                "subtitle": item.get("subtitle", ""),
                "kind": item.get("kind", "file"),
                "source": item.get("source", "host"),
                "icon": item["title"][:1].upper(),
            })
        for entry in entries:
            entry["art"] = self.art_for(entry)
        return entries

    def art_for(self, entry):
        key = quote(entry["id"], safe="")
        if entry["kind"] == "application" and icon_png(self.bundles, entry["id"], self.icons):
            return {"png": f"/icon/{key}.png"}
        return {"svg": f"/icon/{key}.svg"}

    def _send(self, status, body, content_type):
        payload = body.encode("utf-8") if isinstance(body, str) else body
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(payload)

    def icon_route(self, path):
        name = unquote(path[len("/icon/"):])
        stem, dot, extension = name.rpartition(".")
        if not dot:
            return self._send(404, json.dumps({"error": "not found"}), "application/json")
        if extension == "png":
            png = icon_png(self.bundles, stem, self.icons)
            if not png:
                return self._send(404, json.dumps({"error": "no artwork"}), "application/json")
            with open(png, "rb") as handle:
                return self._send(200, handle.read(), "image/png")
        if extension == "svg":
            for entry in self.catalog():
                if entry["id"] == stem:
                    art = art_svg(entry["kind"], entry["id"], entry["title"])
                    return self._send(200, art, "image/svg+xml; charset=utf-8")
        return self._send(404, json.dumps({"error": "not found"}), "application/json")

    def do_GET(self):
        route = urlparse(self.path)
        if route.path in ("/", "/index.html"):
            return self._send(200, PAGE, "text/html; charset=utf-8")
        if route.path == "/api/health":
            return self._send(200, json.dumps({"ok": True}), "application/json")
        if route.path.startswith("/icon/"):
            return self.icon_route(route.path)
        if route.path == "/api/search":
            query = parse_qs(route.query).get("q", [""])[0]
            hits = search(self.catalog(), query)
            body = json.dumps({
                "query": query,
                "hits": [dict(entry, score=value) for entry, value in hits],
                "next": None,
                "generation": 1,
            })
            return self._send(200, body, "application/json")
        return self._send(404, json.dumps({"error": "not found"}), "application/json")


PAGE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Quicklaunch</title>
<style>
  :root {
    color-scheme: dark;
    --bg: #0d0f14;
    --panel: #161a22;
    --raised: #1e2430;
    --line: #2a3241;
    --text: #e6ebf5;
    --muted: #8b96ab;
    --accent: #7aa2ff;
    --accent-ink: #0d0f14;
  }
  * { box-sizing: border-box; }
  html, body { height: 100%; margin: 0; }
  body {
    background: radial-gradient(1200px 700px at 50% -10%, #1a2130 0%, var(--bg) 60%);
    color: var(--text);
    font: 15px/1.45 -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
    display: grid;
    place-items: start center;
    padding-top: 14vh;
    -webkit-font-smoothing: antialiased;
  }
  .launcher {
    width: min(720px, 92vw);
    background: color-mix(in srgb, var(--panel) 92%, transparent);
    border: 1px solid var(--line);
    border-radius: 14px;
    box-shadow: 0 30px 80px rgba(0,0,0,.55), 0 2px 0 rgba(255,255,255,.03) inset;
    overflow: hidden;
    backdrop-filter: blur(18px);
  }
  .search {
    display: flex; align-items: center; gap: 12px;
    padding: 16px 18px;
    border-bottom: 1px solid var(--line);
  }
  .search svg { flex: none; opacity: .55; }
  input {
    flex: 1; min-width: 0;
    background: transparent; border: 0; outline: none;
    color: var(--text); font-size: 20px; letter-spacing: -.01em;
  }
  input::placeholder { color: var(--muted); }
  .scope {
    flex: none;
    font-size: 11px; text-transform: uppercase; letter-spacing: .08em;
    color: var(--muted); border: 1px solid var(--line);
    padding: 3px 7px; border-radius: 6px;
  }
  ul { list-style: none; margin: 0; padding: 6px; max-height: 52vh; overflow-y: auto; }
  li {
    display: grid; grid-template-columns: 34px 1fr auto; align-items: center;
    gap: 12px; padding: 8px 12px; border-radius: 9px; cursor: default;
  }
  li.active { background: var(--raised); box-shadow: 0 0 0 1px var(--line) inset; }
  .art { width: 34px; height: 34px; display: block; }
  .art img { width: 34px; height: 34px; display: block; border-radius: 8px; }
  .art .mono {
    width: 34px; height: 34px; border-radius: 9px; display: grid; place-items: center;
    background: linear-gradient(180deg, #2b3444, #212836);
    border: 1px solid var(--line);
    font-size: 14px; color: var(--text); font-weight: 650;
  }
  .title { font-weight: 550; letter-spacing: -.01em; }
  .sub { color: var(--muted); font-size: 12.5px; }
  .kind {
    font-size: 10.5px; text-transform: uppercase; letter-spacing: .07em;
    color: var(--muted); border: 1px solid var(--line);
    padding: 3px 6px; border-radius: 5px; white-space: nowrap;
  }
  li.active .kind { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 45%, var(--line)); }
  .empty { padding: 28px 18px 34px; text-align: center; color: var(--muted); }
  footer {
    display: flex; align-items: center; gap: 14px;
    padding: 10px 16px; border-top: 1px solid var(--line);
    color: var(--muted); font-size: 12px;
    background: color-mix(in srgb, var(--panel) 60%, transparent);
  }
  kbd {
    font: inherit; font-size: 11px; padding: 2px 6px; border-radius: 5px;
    background: var(--raised); border: 1px solid var(--line); color: var(--text);
  }
  .spacer { margin-left: auto; }
  .toast {
    position: fixed; left: 50%; bottom: 8vh; transform: translateX(-50%);
    background: var(--raised); border: 1px solid var(--line); color: var(--text);
    padding: 9px 14px; border-radius: 9px; font-size: 13px;
    opacity: 0; transition: opacity .18s ease; pointer-events: none;
  }
  .toast.show { opacity: 1; }
</style>
</head>
<body>
  <main class="launcher" role="dialog" aria-label="Quicklaunch">
    <div class="search">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><circle cx="11" cy="11" r="7"></circle><path d="m20 20-3.5-3.5"></path></svg>
      <input id="q" autocomplete="off" spellcheck="false" placeholder="Search apps, files and commands" aria-label="Search">
      <span class="scope" id="scope">all</span>
    </div>
    <ul id="rows" role="listbox" aria-label="Results"></ul>
    <footer>
      <span><kbd>↑</kbd><kbd>↓</kbd> move</span>
      <span><kbd>↵</kbd> open</span>
      <span><kbd>⌘K</kbd> actions</span>
      <span class="spacer" id="status">ready</span>
    </footer>
  </main>
  <div class="toast" id="toast"></div>
<script>
const input = document.getElementById('q');
const rows = document.getElementById('rows');
const status = document.getElementById('status');
const toast = document.getElementById('toast');
let hits = [];
let active = 0;

// Short kind labels: the snapshot carries the protocol's word, the view decides
// what fits next to a title. Same map as `ui/launcher.lua`.
const KIND_LABEL = {
  application: 'App', file: 'File', recordentry: 'Record', repository: 'Repo',
  spore: 'Spore', peer: 'Peer', service: 'Service', action: 'Action',
  task: 'Task', principal: 'Person',
};

// The host resolved art for this hit; a monogram is the fallback, never the ask.
function artOf(hit) {
  const art = hit.art || {};
  if (art.png || art.svg) {
    return `<img src="${art.png || art.svg}" alt="" loading="lazy">`;
  }
  const mono = (hit.icon || hit.title[0] || '?').replace(/[<>&]/g, '');
  return `<span class="mono">${mono}</span>`;
}

function showToast(text) {
  toast.textContent = text;
  toast.classList.add('show');
  clearTimeout(showToast.timer);
  showToast.timer = setTimeout(() => toast.classList.remove('show'), 1600);
}

function draw() {
  rows.innerHTML = '';
  if (!hits.length) {
    rows.innerHTML = '<li class="empty" style="display:block">No results</li>';
    return;
  }
  hits.forEach((hit, index) => {
    const li = document.createElement('li');
    li.className = index === active ? 'active' : '';
    li.innerHTML = `
      <span class="art"></span>
      <span><div class="title"></div><div class="sub"></div></span>
      <span class="kind"></span>`;
    li.querySelector('.art').innerHTML = artOf(hit);
    li.querySelector('.title').textContent = hit.title;
    li.querySelector('.sub').textContent = hit.subtitle || '';
    li.querySelector('.kind').textContent = KIND_LABEL[hit.kind] || hit.kind;
    li.addEventListener('mousemove', () => { if (active !== index) { active = index; draw(); } });
    li.addEventListener('click', () => { active = index; run(); });
    rows.appendChild(li);
  });
}

async function refresh() {
  const query = input.value;
  const response = await fetch(`/api/search?q=${encodeURIComponent(query)}`);
  const data = await response.json();
  hits = data.hits || [];
  active = 0;
  status.textContent = `${hits.length} result${hits.length === 1 ? '' : 's'}`;
  draw();
}

function run() {
  const hit = hits[active];
  if (!hit) return;
  showToast(`Open ${hit.title}  ·  ${hit.id}`);
}

input.addEventListener('input', refresh);
input.addEventListener('keydown', (event) => {
  if (event.key === 'ArrowDown') { event.preventDefault(); active = Math.min(active + 1, hits.length - 1); draw(); }
  if (event.key === 'ArrowUp') { event.preventDefault(); active = Math.max(active - 1, 0); draw(); }
  if (event.key === 'Enter') { event.preventDefault(); run(); }
  if (event.key === 'Escape') { input.value = ''; refresh(); }
  if (event.key === 'k' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); showToast('Action panel: open, copy, reveal, pin'); }
});

refresh();
input.focus();
</script>
</body>
</html>
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8791)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--applications")
    parser.add_argument("--files")
    parser.add_argument("--app-limit", type=int, default=400)
    args = parser.parse_args()

    applications = load(args.applications)
    if applications:
        Handler.applications = applications
    files = load(args.files)
    if files:
        Handler.files = files
    Handler.bundles = bundle_index()
    if not applications:
        # This machine's own bundles are the honest input: a launcher that lists
        # invented apps proves nothing about artwork. The sample list stays for
        # machines with no bundles to read.
        discovered = installed_applications(Handler.bundles, args.app_limit)
        if discovered:
            Handler.applications = discovered

    server = ThreadingHTTPServer((args.host, args.port), Handler)
    logos = sum(1 for app in Handler.applications
                if icon_png(Handler.bundles, app["bundle_id"], Handler.icons))
    print(f"quicklaunch demo: http://{args.host}:{args.port}/  "
          f"apps={len(Handler.applications)} ({logos} with logos) files={len(Handler.files)}")
    server.serve_forever()


if __name__ == "__main__":
    main()
