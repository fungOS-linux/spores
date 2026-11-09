#!/usr/bin/env python3
"""A tiny personal SFX sheet for the browser.

It reads the same appdata mapping the Spore uses and shells out to the same
`sfx` command for playback. There is no second mapping format and no separate
sound engine.

    python3 tools/sfx-web.py --soil-command /tmp/sfx-target/debug/soil-command
    open https://sfx.myc

Bind it to loopback. The Mycelium gateway is the only thing that should expose
it beyond this machine.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

HTML = r"""<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>SFX</title>
<style>
:root{color-scheme:dark;--bg:#111315;--panel:#191d20;--line:#2b3136;--text:#e7ecef;--muted:#95a0a8;--accent:#f0b35e;--danger:#ff7b72}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:15px/1.45 ui-sans-serif,system-ui,-apple-system,Segoe UI,sans-serif}
main{max-width:1080px;margin:0 auto;padding:32px 20px 60px}
header{display:flex;align-items:end;justify-content:space-between;gap:20px;margin-bottom:22px}
h1{font-size:30px;letter-spacing:-.03em;margin:0} .sub{color:var(--muted);font-size:13px;margin-top:4px}
.room{color:var(--accent);font-size:13px;text-align:right}
.grid{display:grid;grid-template-columns:repeat(4,1fr);gap:10px}@media(max-width:720px){.grid{grid-template-columns:repeat(2,1fr)}}
button.pad{appearance:none;text-align:left;background:var(--panel);border:1px solid var(--line);border-radius:12px;padding:13px;min-height:88px;color:var(--text);cursor:pointer;transition:.12s transform,.12s border-color}
button.pad:hover{border-color:var(--accent);transform:translateY(-1px)}button.pad:active{transform:translateY(1px)}button.pad[disabled]{opacity:.4;cursor:not-allowed}
.pad .id{font-weight:650;font-size:15px}.pad .meta{color:var(--muted);font-size:12px;margin-top:5px}.pad .key{float:right;color:var(--muted);font:11px ui-monospace,SFMono-Regular,monospace}
#status{margin-top:18px;color:var(--muted);font-size:13px;min-height:20px}.err{color:var(--danger)}
footer{margin-top:28px;color:var(--muted);font-size:12px;border-top:1px solid var(--line);padding-top:14px}
</style>
<main>
<header><div><h1>SFX</h1><div class="sub">personal instance · pads from appdata</div></div><div class="room" id="room">loading…</div></header>
<div class="grid" id="pads"></div>
<div id="status">ready</div>
<footer id="foot"></footer>
</main>
<script>
const pads = document.getElementById('pads');
const status = document.getElementById('status');
const room = document.getElementById('room');
const foot = document.getElementById('foot');
let mapping = null;
let ctx = null;
const buffers = new Map();
function el(tag, cls, text){const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n}
function audioContext(){ctx = ctx || new (window.AudioContext || window.webkitAudioContext)(); if(ctx.state==='suspended') ctx.resume(); return ctx}
function render(data){
  mapping=data.mapping; pads.replaceChildren();
  room.textContent = 'room ' + (mapping.room || 'host default') + ' · gain ' + mapping.gain;
  foot.textContent = data.appdata + ' · ' + mapping.rules.length + ' rules · ' + mapping.samples.length + ' samples';
  for (const pad of data.pads) {
    const b=el('button','pad'); b.disabled = pad.available === false;
    const id=el('div','id',pad.label || pad.id); if(pad.key){const k=el('span','key',pad.key);id.append(k)} b.append(id);
    b.append(el('div','meta', pad.available === false ? (pad.reason || 'unavailable') : (pad.category || 'pad') + (pad.duration_ms ? ' · '+pad.duration_ms+' ms' : '')));
    b.onclick=()=>play(pad.id); pads.append(b);
  }
}
async function load(){const r=await fetch('/api/mapping'); if(!r.ok) throw new Error(await r.text()); render(await r.json())}
async function play(pad){
  status.className=''; status.textContent='playing '+pad+'…';
  try{
    const context=audioContext();
    let buffer=buffers.get(pad);
    if(!buffer){
      const r=await fetch('/api/audio/'+encodeURIComponent(pad)+'.wav');
      if(!r.ok) throw new Error((await r.json()).error || 'audio failed');
      buffer=await context.decodeAudioData(await r.arrayBuffer());
      buffers.set(pad,buffer);
    }
    const source=context.createBufferSource(); source.buffer=buffer; source.connect(context.destination); source.start();
    status.textContent='played '+pad+' in the browser';
  }
  catch(e){status.className='err';status.textContent=e.message}
}
load().catch(e=>{status.className='err';status.textContent=e.message});
</script>
"""


def command(args: list[str], soil_command: str, root: Path, appdata: Path, commands: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [soil_command, "--root", str(commands), "--vfs-root", str(root),
         "--appdata-root", str(appdata), "--audio", "run", "sfx", *args],
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=30,
    )


def command_bytes(args: list[str], soil_command: str, root: Path, appdata: Path, commands: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [soil_command, "--root", str(commands), "--vfs-root", str(root),
         "--appdata-root", str(appdata), "--audio", "run", "sfx", *args],
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=30,
    )


def prewarm(server: "ThreadingHTTPServer") -> None:
    """Render every available pad once, off the request path."""
    try:
        listing = command(["--json", "list"], server.soil_command, server.root, server.appdata, server.commands)
        if listing.returncode != 0:
            return
        for pad in json.loads(listing.stdout).get("pads", []):
            if pad.get("available") is False or pad["id"] in server.cache:
                continue
            rendered = command_bytes(["render", pad["id"]], server.soil_command, server.root, server.appdata, server.commands)
            if rendered.returncode == 0 and rendered.stdout:
                server.cache[pad["id"]] = rendered.stdout
                print(f"sfx-web: cached {pad['id']} ({len(rendered.stdout)} bytes)", flush=True)
    except Exception as error:  # prewarm is an optimization; requests still work
        print(f"sfx-web: prewarm skipped: {error}", file=sys.stderr, flush=True)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        sys.stderr.write("sfx-web: " + fmt % args + "\n")

    def send_json(self, status: int, value) -> None:
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = urlparse(self.path).path
        if path in ("/", "/index.html"):
            body = HTML.encode()
            self.send_response(200)
            self.send_header("content-type", "text/html; charset=utf-8")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path.startswith("/api/mapping"):
            result = command(["mapping"], self.server.soil_command, self.server.root, self.server.appdata, self.server.commands)
            if result.returncode != 0:
                self.send_json(500, {"error": result.stderr.strip() or "mapping failed"})
                return
            try:
                mapping = json.loads(result.stdout)
            except json.JSONDecodeError as error:
                self.send_json(500, {"error": f"mapping JSON: {error}"})
                return
            pads = [
                {"id": p["id"], "label": p.get("label"), "category": p.get("category"),
                 "duration_ms": p.get("duration_ms"), "available": p.get("available", True),
                 "reason": p.get("reason"), "key": p.get("key")}
                for p in self.mapping_pads(mapping)
            ]
            self.send_json(200, {"mapping": mapping, "pads": pads, "appdata": str(self.server.appdata)})
            return
        if self.path.startswith("/api/audio/"):
            pad = self.path.rsplit("/", 1)[-1].removesuffix(".wav")
            data = self.server.cache.get(pad)
            if data is None:
                rendered = command_bytes(["render", pad], self.server.soil_command, self.server.root, self.server.appdata, self.server.commands)
                if rendered.returncode != 0:
                    self.send_json(404, {"error": rendered.stderr.decode(errors="replace").strip() or "unknown pad"})
                    return
                data = rendered.stdout
                self.server.cache[pad] = data
            self.send_response(200)
            self.send_header("content-type", "audio/wav")
            self.send_header("content-length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        self.send_json(404, {"error": "not found"})

    def mapping_pads(self, mapping):
        # `sfx list --json` is the source of the pad view; mapping alone has
        # rules/samples but not the built-in metadata.
        result = command(["--json", "list"], self.server.soil_command, self.server.root, self.server.appdata, self.server.commands)
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or "list failed")
        return json.loads(result.stdout)["pads"]

    def do_POST(self):
        if urlparse(self.path).path != "/api/play":
            self.send_json(404, {"error": "not found"})
            return
        try:
            length = int(self.headers.get("content-length", "0"))
            payload = json.loads(self.rfile.read(length) or b"{}")
            pad = str(payload.get("pad", ""))
            if not pad:
                raise ValueError("pad is required")
        except (ValueError, json.JSONDecodeError) as error:
            self.send_json(400, {"error": str(error)})
            return
        result = command(["play", pad], self.server.soil_command, self.server.root, self.server.appdata, self.server.commands)
        if result.returncode != 0:
            self.send_json(500, {"error": result.stderr.strip() or "play failed"})
            return
        self.send_json(200, {"output": result.stdout.strip()})


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8795)
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--soil-command", default=os.environ.get("SOIL_COMMAND", "soil-command"))
    parser.add_argument("--root", default=os.path.expanduser("~/.local/share/fungos/vfs"))
    parser.add_argument("--appdata", default=os.path.expanduser("~/.local/share/fungos/appdata"))
    parser.add_argument("--commands", default=os.path.expanduser("~/.local/share/fungos/commands"))
    args = parser.parse_args()
    server = ThreadingHTTPServer((args.bind, args.port), Handler)
    server.soil_command = args.soil_command
    server.root = Path(args.root)
    server.appdata = Path(args.appdata)
    server.commands = Path(args.commands)
    server.cache = {}
    print(f"sfx-web: http://{args.bind}:{args.port} · appdata {args.appdata}", flush=True)
    threading.Thread(target=prewarm, args=(server,), daemon=True).start()
    server.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
