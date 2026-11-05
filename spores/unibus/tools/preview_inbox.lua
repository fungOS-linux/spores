-- Approximate HTML preview for the new declarative unibus UI.
--
-- This walks the same tree ui/inbox.lua gives Canvas and renders it with HTML
-- flexbox. It is a review aid, not a host: spacing and type are approximate.
--
--   lua tools/preview_inbox.lua examples/inbox_mock.lua > /tmp/inbox.html

local snapshot_path = assert(arg[1], "usage: preview_inbox.lua <snapshot.lua>")

local ui = assert(loadfile("ui/lib/ui.lua"))()
_G.ui = ui
_G.module = {
  theme = {
    title = 0.34,
    large = 0.24,
    body = 0.145,
    small = 0.105,
    micro = 0.085,
    radius = 0.02,
  },
}

local inbox = assert(loadfile("ui/inbox.lua"))()
local snapshot = assert(loadfile(snapshot_path))()
if arg[2] == "narrow" then
  snapshot.layout = snapshot.layout or {}
  snapshot.layout.width_class = "narrow"
  snapshot.layout.focus = arg[3] or "list"
end
local tree = inbox.tree(snapshot)

local function escape(text)
  return (tostring(text):gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"):gsub('"', "&quot;"))
end

local GAP = { none = 0, xs = 4, sm = 8, md = 16, lg = 28, xl = 44 }
local function gap(value) return string.format("%dpx", GAP[value or "none"] or 0) end
local function ink(value) return "ink-" .. (value or "text") end
local function style(value) return "style-" .. (value or "body") end

local function render(node)
  if not node then return "" end
  local kind = node.kind

  if kind == "text" then
    return string.format('<div class="txt %s %s">%s</div>', style(node.style), ink(node.ink), escape(node.value))
  elseif kind == "icon" then
    return string.format('<span class="glyph %s %s">%s</span>', ink(node.ink), node.size or "micro", escape(node.glyph or ""))
  elseif kind == "icon_button" then
    local cls = node.quiet and "quiet" or "primary"
    return string.format('<span class="pill icon %s %s">%s</span>', cls, node.size or "small", escape(node.glyph or ""))
  elseif kind == "button" then
    local cls = node.quiet and "quiet" or "primary"
    if node.selected then cls = cls .. " selected" end
    return string.format('<span class="pill %s %s">%s</span>', cls, node.small and "small" or "", escape(node.label or ""))
  elseif kind == "rule" then
    return '<hr>'
  elseif kind == "spacer" then
    return string.format('<div style="height:%s"></div>', gap(node.size))
  elseif kind == "fill" then
    return '<div class="fill"></div>'
  elseif kind == "bullets" then
    local out = { '<ul class="bullets">' }
    for _, item in ipairs(node.items or {}) do out[#out + 1] = "<li>" .. escape(item) .. "</li>" end
    out[#out + 1] = "</ul>"
    return table.concat(out)
  elseif kind == "quote" then
    return '<blockquote>' .. escape(node.value) .. "</blockquote>"
  elseif kind == "code" then
    return '<pre class="code">' .. escape(node.source) .. "</pre>"
  elseif kind == "diagram" then
    return '<div class="diagram">[' .. escape(node.format) .. "] " .. escape(node.source) .. "</div>"
  elseif kind == "row" then
    local out = { string.format('<div class="row" style="gap:%s;%s">', gap(node.gap), node.align == "end" and "justify-content:flex-end" or "") }
    for index, child in ipairs(node.children or {}) do
      local basis = node.widths and string.format("flex:0 0 %.2f%%", node.widths[index] * 100) or "flex:1 1 0"
      out[#out + 1] = string.format('<div style="%s;min-width:0">%s</div>', basis, render(child))
    end
    out[#out + 1] = "</div>"
    return table.concat(out)
  elseif kind == "column" then
    local pad = node.pad and string.format("padding:%s", gap("sm")) or ""
    return string.format('<div class="col" style="gap:%s;%s">%s</div>', gap(node.gap), pad, render_children(node))
  elseif kind == "card" then
    return string.format('<div class="card" style="gap:%s">%s</div>', gap(node.gap), render_children(node))
  elseif kind == "bar" then
    local surface = node.ink == "primary"
      and string.format("background:rgba(122,162,247,%.2f)", node.opacity or 0.12)
      or string.format("background:rgba(20,22,28,%.2f)", node.opacity or 0.9)
    local border = node.border and "border:1px solid #262a31;" or ""
    if node.border_top then border = border .. "border-top:1px solid #262a31;" end
    return string.format(
      '<div class="bar" style="gap:%s;%s;%s;border-radius:%s">%s</div>',
      gap(node.pad and "sm" or "none"),
      border,
      surface,
      node.radius and "4px" or "0",
      render_children(node)
    )
  end
  return render_children(node)
end

function render_children(node)
  local out = {}
  for _, child in ipairs(node.children or {}) do out[#out + 1] = render(child) end
  return table.concat(out)
end

local html = {
  "<!doctype html><meta charset=utf-8><title>unibus UI preview</title><style>",
  ":root{color-scheme:dark}",
  "*{box-sizing:border-box}",
  "body{margin:0;background:#0d0f13;color:#e8e6e3;font:15px/1.45 ui-sans-serif,system-ui,sans-serif}",
  ".app{max-width:1500px;margin:0 auto;padding:18px}",
  ".col{display:flex;flex-direction:column;min-width:0}",
  ".row{display:flex;align-items:flex-start;min-width:0}",
  ".bar{display:flex;flex-direction:column;padding:10px 14px;min-width:0}",
  ".bar.surface{background:#14161c}",
  ".bar.accent{background:rgba(122,162,247,.12);border:1px solid rgba(122,162,247,.2)}",
  ".card{display:flex;flex-direction:column;background:#171a21;border:1px solid #262a31;border-radius:6px;padding:12px}",
  ".txt{min-width:0;overflow-wrap:anywhere}",
  ".style-title{font-size:2.0rem;font-weight:700;letter-spacing:-.02em}",
  ".style-heading{font-size:1.45rem;font-weight:650;letter-spacing:-.01em}",
  ".style-subheading{font-size:1.1rem;font-weight:600}",
  ".style-body{font-size:.95rem}",
  ".style-caption{font-size:.76rem;letter-spacing:.02em}",
  ".style-mono{font:13px/1.5 ui-monospace,SFMono-Regular,monospace}",
  ".ink-text{color:#e8e6e3}.ink-muted{color:#8b949e}.ink-primary{color:#7aa2f7}",
  ".ink-success{color:#9ece6a}.ink-warning{color:#e0af68}.ink-danger{color:#f7768e}",
  ".ink-background{color:#0d0f13}.ink-surface{color:#14161c}",
  ".glyph{display:inline-block;line-height:1.1;width:1.2em;text-align:center}",
  ".glyph.primary{color:#7aa2f7}.glyph.muted{color:#5b626e}",
  ".pill{display:inline-flex;align-items:center;justify-content:center;border-radius:999px;padding:.28rem .7rem;font-size:.8rem;white-space:nowrap;background:#7aa2f7;color:#0d0f13;font-weight:600}",
  ".pill.small{font-size:.72rem;padding:.2rem .55rem}",
  ".pill.quiet{background:#252a33;color:#c4c9d2;font-weight:500}",
  ".pill.selected{outline:1px solid #7aa2f7}",
  ".pill.icon{width:2rem;height:2rem;padding:0;background:#252a33;color:#c4c9d2}",
  "hr{border:0;border-top:1px solid #262a31;margin:6px 0;width:100%}",
  ".bullets{margin:.2rem 0;padding-left:1.1rem}.bullets li{margin:.2rem 0}",
  "blockquote{margin:.3rem 0;border-left:2px solid #7aa2f7;padding-left:.8rem;color:#aab1bd}",
  ".code,.diagram{background:#171a21;border:1px solid #262a31;border-radius:6px;padding:10px;font:12px/1.5 ui-monospace,monospace;white-space:pre-wrap}",
  ".diagram{border-style:dashed;color:#8b949e}",
  ".fill{height:1px;background:#262a31}",
  "</style><body><div class='app'>",
  "<div class='col' style='gap:0'>",
  render(tree),
  "</div></div></body>",
}
io.write(table.concat(html, "\n"))
