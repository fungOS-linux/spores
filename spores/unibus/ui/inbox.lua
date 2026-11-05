-- Initial Unibus inbox UI.
--
-- Canvas owns every pixel; this file owns roles, structure and grouping. The
-- module sends a flat snapshot of record-backed metadata. No I/O, no clock, no
-- randomness, no credentials.
local M = {}

M.name = "unibus"
M.version = "0.1.0"

local ROLE = {
  chrome = "muted",
  title = "primary",
  body = "text",
  quiet = "muted",
  urgent = "warning",
  unread = "primary",
}

local function row(record, index)
  local nodes = {
    {
      id = "r" .. index .. ".state",
      kind = "text",
      role = record.unread and ROLE.unread or ROLE.quiet,
      text = record.unread and "●" or "○",
    },
    {
      id = "r" .. index .. ".title",
      kind = "text",
      role = record.unread and ROLE.title or ROLE.body,
      text = record.title or "(no subject)",
    },
  }
  if record.summary and record.summary ~= "" then
    nodes[#nodes + 1] = {
      id = "r" .. index .. ".summary",
      kind = "text",
      role = ROLE.quiet,
      text = record.summary,
    }
  end
  if record.urgent then
    nodes[#nodes + 1] = {
      id = "r" .. index .. ".urgent",
      kind = "text",
      role = ROLE.urgent,
      text = "needs attention",
    }
  end
  return {
    id = "r" .. index,
    kind = "row",
    role = ROLE.body,
    gap = 2,
    selected = record.selected and true or false,
    children = nodes,
  }
end

function M.render(snapshot)
  local nodes = {
    {
      id = "title",
      kind = "text",
      role = ROLE.title,
      level = 1,
      text = snapshot.title or "Inbox",
    },
  }
  if snapshot.brief and snapshot.brief ~= "" then
    nodes[#nodes + 1] = {
      id = "brief",
      kind = "text",
      role = ROLE.chrome,
      text = snapshot.brief,
    }
  end
  local index = 0
  for _, section in ipairs(snapshot.sections or {}) do
    nodes[#nodes + 1] = {
      id = "s." .. (section.collection or "unknown"),
      kind = "text",
      role = ROLE.chrome,
      level = 2,
      text = section.label or section.collection or "Records",
    }
    for _, record in ipairs(section.records or {}) do
      index = index + 1
      nodes[#nodes + 1] = row(record, index)
    end
  end
  return {
    revision = snapshot.revision or 0,
    ui = "unibus@0.1.0",
    nodes = nodes,
  }
end

return M
