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
#   python3 server.py --port 8791 --applications apps.json
#
import argparse
import json
import math
import os
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

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

# Frecency and aliases would live in soil:appdata in the real Spore.
ALIASES = {"c": "com.google.Chrome", "t": "com.apple.Terminal", "s": "com.apple.Safari"}
FRECENCY = {"com.apple.Terminal": {"uses": 9, "last": time.time() - 300}}


def load(path):
    if not path or not os.path.exists(path):
        return None
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
    return payload.get("applications", payload)


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

    def log_message(self, *_args):
        pass

    def _send(self, status, body, content_type):
        payload = body.encode("utf-8") if isinstance(body, str) else body
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        route = urlparse(self.path)
        if route.path in ("/", "/index.html"):
            return self._send(200, PAGE, "text/html; charset=utf-8")
        if route.path == "/api/health":
            return self._send(200, json.dumps({"ok": True}), "application/json")
        if route.path == "/api/search":
            query = parse_qs(route.query).get("q", [""])[0]
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
            hits = search(entries, query)
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
    display: grid; grid-template-columns: 30px 1fr auto; align-items: center;
    gap: 12px; padding: 9px 12px; border-radius: 9px; cursor: default;
  }
  li.active { background: var(--raised); box-shadow: 0 0 0 1px var(--line) inset; }
  .glyph {
    width: 30px; height: 30px; border-radius: 8px;
    display: grid; place-items: center;
    background: linear-gradient(180deg, #2b3444, #212836);
    border: 1px solid var(--line);
    font-size: 14px; color: var(--text); font-weight: 600;
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
      <span class="glyph">${(hit.icon || hit.title[0] || '?').replace(/[<>&]/g, '')}</span>
      <span><div class="title"></div><div class="sub"></div></span>
      <span class="kind"></span>`;
    li.querySelector('.title').textContent = hit.title;
    li.querySelector('.sub').textContent = hit.subtitle || '';
    li.querySelector('.kind').textContent = hit.kind;
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
    args = parser.parse_args()

    applications = load(args.applications)
    if applications:
        Handler.applications = applications
    files = load(args.files)
    if files:
        Handler.files = files

    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(f"quicklaunch demo: http://{args.host}:{args.port}/  apps={len(Handler.applications)} files={len(Handler.files)}")
    server.serve_forever()


if __name__ == "__main__":
    main()
