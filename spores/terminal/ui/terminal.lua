-- Terminal Spore fallback/status UI.
--
-- The interactive terminal is a host-rendered grid from soil:terminal. This
-- view lists Mycelium targets and shows the latest plain-text frame for
-- debugging and non-Canvas surfaces.

local M = {}

M.name = "terminal"
M.version = "0.1.0"

local ROLE = {
  title = "primary",
  body = "text",
  quiet = "muted",
  degraded = "warning",
}

local function target_row(target, index)
  return {
    id = "t" .. index,
    kind = "row",
    role = ROLE.body,
    gap = 1,
    children = {
      {
        id = "t" .. index .. ".name",
        kind = "text",
        role = target.shell and ROLE.title or ROLE.quiet,
        text = target.label or target.id,
      },
      {
        id = "t" .. index .. ".meta",
        kind = "text",
        role = target.health == "healthy" and ROLE.quiet or ROLE.degraded,
        text = string.format("%s · %s", target.kind or "target", target.health or "unknown"),
      },
    },
  }
end

function M.render(snapshot)
  local nodes = {
    {
      id = "title",
      kind = "text",
      role = ROLE.title,
      level = 1,
      text = "Terminal",
    },
  }

  local terminal = snapshot.terminal
  if terminal then
    nodes[#nodes + 1] = {
      id = "term.title",
      kind = "text",
      role = ROLE.quiet,
      text = terminal.title or "session",
    }
    for index, line in ipairs((terminal.text or ""):gmatch("([^\n]*)\n?")) do
      if line ~= "" then
        nodes[#nodes + 1] = {
          id = "term." .. index,
          kind = "text",
          role = ROLE.body,
          text = line,
        }
      end
    end
  else
    nodes[#nodes + 1] = {
      id = "empty",
      kind = "text",
      role = ROLE.quiet,
      text = "No session. Select a Mycelium target.",
    }
  end

  if #(snapshot.targets or {}) > 0 then
    nodes[#nodes + 1] = {
      id = "targets",
      kind = "text",
      role = ROLE.quiet,
      level = 2,
      text = "Targets",
    }
    for index, target in ipairs(snapshot.targets) do
      nodes[#nodes + 1] = target_row(target, index)
    end
  end

  return {
    revision = 0,
    ui = "terminal@0.1.0",
    nodes = nodes,
  }
end

return M
