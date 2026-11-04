-- Fixture for the Lua UI contract. Run with `lua tests/ui_fixture.lua` from the
-- spore directory. No JSON bridge: the host materializes a table from the
-- typed snapshot, so the fixture is the same table a host would build.

local source = assert(io.open("ui/editor.lua", "rb"), "ui/editor.lua missing")
local chunk = assert(load(source:read("*a"), "ui/editor.lua"))
source:close()
local ui = chunk()

assert(ui.name == "markdown", "ui name")
assert(type(ui.render) == "function", "render is exported")

local snapshot = {
  revision = 7,
  mode = "edit",
  dirty = true,
  line_count = 14,
  scroll = 0,
  cursor = { line = 3, column = 4 },
  selection = { { line = 3, column = 0 }, { line = 3, column = 4 } },
  blocks = {
    { index = 0, line = 0, kind = "heading", level = 1, ordered = false, item = 0,
      text = "Field notes", spans = { { style = "text", text = "Field notes" } } },
    { index = 1, line = 2, kind = "paragraph", level = 0, ordered = false, item = 0,
      text = "A small document",
      spans = { { style = "text", text = "A " }, { style = "emphasis", text = "small" },
                { style = "text", text = " document" } } },
    { index = 2, line = 4, kind = "list_item", level = 0, ordered = true, item = 0,
      text = "first", spans = { { style = "text", text = "first" } } },
    { index = 3, line = 6, kind = "diagram", level = 0, ordered = false, item = 0,
      language = "mermaid", text = "graph TD\n  A --> B",
      spans = { { style = "code", text = "graph TD\n  A --> B" } } },
    { index = 4, line = 10, kind = "paragraph", level = 0, ordered = false, item = 0,
      text = "map",
      spans = { { style = "image", text = "map", target = "asset:sha256:00ff" } } },
  },
  assets = {
    { kind = "image", target = "asset:sha256:00ff", line = 10, label = "map" },
    { kind = "diagram", target = "inline:1234", line = 6, label = "mermaid" },
  },
}

local scene = ui.render(snapshot)
assert(scene.revision == snapshot.revision, "revision carries through")
assert(scene.ui == "markdown@0.1.0", "ui identity is reported")
assert(type(scene.nodes) == "table" and #scene.nodes > 0, "nodes produced")

local by_id = {}
for _, node in ipairs(scene.nodes) do
  by_id[node.id] = node
end

assert(by_id.chrome and by_id.chrome.kind == "row", "chrome row")
assert(by_id.b0 and by_id.b0.role == "primary" and by_id.b0.text == "Field notes", "heading")
assert(by_id["b2"] and by_id["b2"].text == "1. first", "ordered list marker is UI")
assert(by_id.b3 and by_id.b3.kind == "diagram" and by_id.b3.format == "mermaid", "diagram")
assert(by_id["b4-image"] and by_id["b4-image"].target == "asset:sha256:00ff", "image")
assert(by_id.caret and by_id.caret.line == 3 and by_id.caret.column == 4, "caret")

-- Read mode removes the caret: presentation follows mode, Rust sends neither.
snapshot.mode = "read"
snapshot.dirty = false
local reading = ui.render(snapshot)
for _, node in ipairs(reading.nodes) do
  assert(node.kind ~= "caret", "no caret in read mode")
end

print("ui: ok")
