-- Markdown reader/editor UI.
--
-- Canvas loads, JITs, and hot-reloads this file. It is the entire presentation
-- layer: layout structure, roles, chrome, and how images and diagrams sit in
-- the flow. Rust sends a snapshot; it never decides what is visible.
--
-- No ambient authority: no system calls, no file access, no module loading, no
-- extra globals. Canvas supplies input and a snapshot and nothing else.
-- Editing this file is the supported way to restyle the app, including from an
-- agent mid-session.

local M = {}

M.name = "markdown"
M.version = "0.1.0"

-- Coherence roles, never colours. Canvas resolves them per theme.
local ROLE = {
  chrome = "muted",
  heading = "primary",
  body = "text",
  quiet = "muted",
  mark = "primary",
  unsaved = "warning",
}

local function position(cursor)
  return string.format("%d:%d", (cursor.line or 0) + 1, (cursor.column or 0) + 1)
end

local function chrome(snapshot)
  return {
    id = "chrome",
    kind = "row",
    role = ROLE.chrome,
    align = "end",
    gap = 2,
    children = {
      { id = "mode", kind = "text", role = ROLE.chrome, text = snapshot.mode },
      {
        id = "dirty",
        kind = "text",
        role = snapshot.dirty and ROLE.unsaved or ROLE.chrome,
        text = snapshot.dirty and "unsaved" or "saved",
      },
      { id = "position", kind = "text", role = ROLE.chrome, text = position(snapshot.cursor) },
    },
  }
end

local function images(block, out)
  for _, span in ipairs(block.spans or {}) do
    if span.style == "image" then
      out[#out + 1] = {
        id = string.format("b%d-image", block.index),
        kind = "image",
        role = ROLE.quiet,
        target = span.target,
        alt = span.text,
        line = block.line,
      }
    end
  end
end

local function block_node(block)
  if block.kind == "spacer" then
    return { id = string.format("b%d", block.index), kind = "space", role = ROLE.body }
  end
  if block.kind == "rule" then
    return {
      id = string.format("b%d", block.index),
      kind = "rule",
      role = ROLE.quiet,
      line = block.line,
    }
  end
  if block.kind == "code" then
    return {
      id = string.format("b%d", block.index),
      kind = "code",
      role = ROLE.quiet,
      language = block.language,
      text = block.text,
      line = block.line,
    }
  end
  if block.kind == "diagram" then
    return {
      id = string.format("b%d", block.index),
      kind = "diagram",
      role = ROLE.quiet,
      format = block.language,
      source = block.text,
      line = block.line,
    }
  end

  local role = ROLE.body
  if block.kind == "heading" then
    role = ROLE.heading
  elseif block.kind == "quote" then
    role = ROLE.quiet
  end

  local text = block.text
  if block.kind == "list_item" then
    if block.ordered then
      text = string.format("%d. %s", block.item + 1, text)
    else
      text = "- " .. text
    end
  end

  return {
    id = string.format("b%d", block.index),
    kind = "text",
    role = role,
    level = block.level,
    text = text,
    spans = block.spans,
    line = block.line,
  }
end

-- Render one snapshot into Canvas nodes. Deterministic: same snapshot, same
-- scene. Canvas owns layout, typography, and pixels from here.
function M.render(snapshot)
  local nodes = { chrome(snapshot) }

  for _, block in ipairs(snapshot.blocks) do
    images(block, nodes)
    local node = block_node(block)
    if node.text ~= "" or node.kind ~= "text" then
      nodes[#nodes + 1] = node
    end
  end

  if snapshot.mode == "edit" then
    nodes[#nodes + 1] = {
      id = "caret",
      kind = "caret",
      role = ROLE.mark,
      line = snapshot.cursor.line,
      column = snapshot.cursor.column,
      selection = snapshot.selection,
    }
  end

  return {
    revision = snapshot.revision,
    line_count = snapshot.line_count,
    scroll = snapshot.scroll,
    ui = string.format("%s@%s", M.name, M.version),
    nodes = nodes,
  }
end

return M
