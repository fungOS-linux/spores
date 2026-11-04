-- Preview harness for a Lua UI. NOT a host and not part of the spore.
--
-- Canvas renders web, native, and in-window. This script exists so the UI can be
-- seen without a compositor: it feeds a real snapshot to ui/editor.lua and
-- prints static HTML that mirrors the scene. Anything it cannot draw (a
-- mermaid diagram, a retrieved asset) is shown as a named placeholder, exactly
-- as Canvas would report it.
--
--   cargo run --example snapshot > /tmp/md_snapshot.lua
--   lua tools/preview.lua /tmp/md_snapshot.lua > /tmp/preview.html
--
-- If the preview and Canvas disagree, Canvas is right.

local snapshot_path = assert(arg[1], "usage: preview.lua <snapshot.lua>")

local ui_source = assert(io.open("ui/editor.lua", "rb"), "ui/editor.lua missing")
local ui = assert(load(ui_source:read("*a"), "ui/editor.lua"))()
ui_source:close()

local snapshot = assert(loadfile(snapshot_path))()
local scene = ui.render(snapshot)

local function escape(text)
  return (tostring(text):gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"):gsub('"', "&quot;"))
end

local ROLE = {
  text = "#e8e6e3",
  muted = "#8b949e",
  primary = "#7aa2f7",
  success = "#9ece6a",
  warning = "#e0af68",
  danger = "#f7768e",
}

local function spans(node)
  if not node.spans or #node.spans == 0 then
    return escape(node.text)
  end
  local out = {}
  for _, span in ipairs(node.spans) do
    local text = escape(span.text)
    if span.style == "code" then
      out[#out + 1] = "<code>" .. text .. "</code>"
    elseif span.style == "strong" then
      out[#out + 1] = "<strong>" .. text .. "</strong>"
    elseif span.style == "emphasis" then
      out[#out + 1] = "<em>" .. text .. "</em>"
    elseif span.style == "strong_emphasis" then
      out[#out + 1] = "<strong><em>" .. text .. "</em></strong>"
    elseif span.style == "link" then
      out[#out + 1] = '<a title="' .. escape(span.target) .. '">' .. text .. "</a>"
    elseif span.style == "image" then
      out[#out + 1] = '<span class="asset">image · ' .. text .. "</span>"
    else
      out[#out + 1] = text
    end
  end
  return table.concat(out)
end

local function node_html(node, edition)
  local ink = ROLE[node.role] or ROLE.text
  local style = string.format("color:%s", ink)
  if node.kind == "text" and node.level and node.level > 0 then
    local sizes = { [1] = "2.1rem", [2] = "1.6rem", [3] = "1.3rem" }
    style = style .. ";font-size:" .. (sizes[node.level] or "1.1rem") .. ";font-weight:650"
  end
  if node.kind == "rule" then
    return '<hr style="border-color:' .. ROLE.muted .. '">'
  end
  if node.kind == "space" then
    return '<div style="height:.8rem"></div>'
  end
  if node.kind == "code" then
    local body = escape(node.text):gsub("\n", "<br>")
    local language = node.language and ('<span class="tag">' .. escape(node.language) .. "</span>") or ""
    return '<pre class="code">' .. language .. "<div>" .. body .. "</div></pre>"
  end
  if node.kind == "diagram" then
    local body = escape(node.source):gsub("\n", "<br>")
    return '<figure class="diagram"><figcaption>Canvas renders · ' .. escape(node.format)
      .. "</figcaption><pre>" .. body .. "</pre></figure>"
  end
  if node.kind == "image" then
    return '<figure class="image"><div class="thumb">asset</div><figcaption>'
      .. escape(node.alt or "") .. " · " .. escape(node.target or "") .. "</figcaption></figure>"
  end
  if node.kind == "caret" and edition then
    return '<div class="caret">caret · line ' .. (node.line + 1) .. ", column " .. (node.column + 1) .. "</div>"
  end
  if node.kind == "row" then
    local children = {}
    for _, child in ipairs(node.children or {}) do
      children[#children + 1] = node_html(child, false)
    end
    return '<header class="chrome">' .. table.concat(children, "<span>·</span>") .. "</header>"
  end
  return '<p style="' .. style .. '">' .. spans(node) .. "</p>"
end

local body = {}
for _, node in ipairs(scene.nodes) do
  body[#body + 1] = node_html(node, true)
end

local html = {
  "<!doctype html><meta charset=utf-8>",
  "<title>markdown spore · Lua UI preview</title>",
  "<style>",
  ":root{color-scheme:dark}",
  "body{background:#14161a;margin:0;padding:3rem 1.5rem;font:16px/1.65 ui-sans-serif,system-ui,sans-serif}",
  "main{max-width:44rem;margin:0 auto}",
  ".chrome{display:flex;justify-content:flex-end;gap:.5rem;font-size:.8rem;letter-spacing:.04em;text-transform:uppercase;border-bottom:1px solid #262a31;padding-bottom:.7rem;margin-bottom:2rem}",
  ".chrome span{color:#3b4048}",
  "p{margin:.55rem 0}",
  "code{font:.9em ui-monospace,SFMono-Regular,monospace;background:#1e2127;padding:.1em .35em;border-radius:3px}",
  ".code{background:#171a1f;border:1px solid #262a31;border-radius:6px;padding:1rem;overflow:auto;font:13px/1.6 ui-monospace,monospace}",
  ".code .tag{float:right;color:#8b949e;font-size:.75rem;text-transform:uppercase}",
  ".diagram{border:1px dashed #3b4048;border-radius:6px;padding:.9rem 1rem;color:#8b949e}",
  ".diagram figcaption{font-size:.75rem;text-transform:uppercase;letter-spacing:.05em;margin-bottom:.4rem}",
  ".diagram pre{margin:0;color:#e8e6e3;font:13px/1.5 ui-monospace,monospace}",
  ".image{display:flex;gap:.85rem;align-items:center;border:1px solid #262a31;border-radius:6px;padding:.6rem}",
  ".thumb{width:4.5rem;height:3rem;display:grid;place-items:center;background:#1e2127;color:#8b949e;font-size:.7rem;border-radius:4px}",
  ".image figcaption{font-size:.8rem;color:#8b949e}",
  ".asset{color:#8b949e;font-size:.9em}",
  "a{color:#7aa2f7;text-decoration:none;border-bottom:1px dotted #7aa2f7}",
  ".caret{margin-top:2rem;color:#7aa2f7;font-size:.8rem;font-family:ui-monospace,monospace}",
  "</style>",
  "<main>" .. table.concat(body) .. "</main>",
}
io.write(table.concat(html, "\n"))
