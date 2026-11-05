-- Static preview for ui/inbox.lua. NOT a host.
--   lua tools/preview.lua examples/inbox_snapshot.lua > /tmp/inbox.html
local snapshot_path = assert(arg[1], "usage: preview.lua <snapshot.lua>")
local ui_file = assert(io.open("ui/inbox.lua", "rb"), "ui/inbox.lua missing")
local ui = assert(load(ui_file:read("*a"), "ui/inbox.lua"))()
ui_file:close()
local scene = ui.render(assert(loadfile(snapshot_path))())

local ROLE = {
  text = "#e8e6e3", muted = "#8b949e", primary = "#7aa2f7",
  success = "#9ece6a", warning = "#e0af68", danger = "#f7768e",
}
local function esc(text)
  return (tostring(text):gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"):gsub('"', "&quot;"))
end
local out = { "<!doctype html><meta charset=utf-8><body style='background:#16161e;font-family:system-ui;padding:2rem'>" }
for _, node in ipairs(scene.nodes) do
  local color = ROLE[node.role] or ROLE.text
  local size = node.level == 1 and "1.8rem" or node.level == 2 and "1.1rem" or "0.95rem"
  local weight = node.level and "650" or "400"
  if node.kind == "row" then
    out[#out + 1] = "<div style='display:flex;gap:1.2rem;color:" .. ROLE.text .. "'>"
    for _, child in ipairs(node.children or {}) do
      out[#out + 1] = "<span style='color:" .. (ROLE[child.role] or ROLE.text) .. "'>" .. esc(child.text) .. "</span>"
    end
    out[#out + 1] = "</div>"
  else
    out[#out + 1] = string.format("<div style='color:%s;font-size:%s;font-weight:%s;margin:.35rem 0'>%s</div>", color, size, weight, esc(node.text))
  end
end
out[#out + 1] = "</body>"
print(table.concat(out))
