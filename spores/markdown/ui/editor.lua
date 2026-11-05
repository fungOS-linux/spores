-- markdown spore · view
--
-- A pure function of the snapshot. Frame, rail and document are one tree: no
-- coordinates, no colours, no callbacks. Anything that looks pressable is an
-- affordance; wiring presses to actions is the host's next step.

-- The surface hands its own small state back as inputs: which mode the pane is
-- in, and which section the rail selected. A view is a pure function of these.
local inputs = module.inputs or {}
local mode = inputs.mode or "read"
local section = inputs.section or "What this is"

local function slug(value)
  return (tostring(value):lower():gsub("[^%w]+", "-"):gsub("^-", ""):gsub("-$", ""))
end

local outline = {
  { "Field notes", "h1", 1 },
  { "What this is", "h2", 1 },
  { "Content it handles", "h2", 1 },
  { "Code", "h3", 0 },
  { "Diagrams", "h3", 0 },
  { "Images and attachments", "h3", 0 },
  { "Open questions", "h2", 0 },
}

-- A rail row is a control, not a picture of one: the host owns the hit region
-- and returns `section` as an input on the next render.
local function rail_entry(label, level, active)
  return ui.button(label, {
    action = "section-" .. slug(label),
    body = { value = label },
    small = level == "h3",
    quiet = not active,
  })
end

local rail = {}
for _, entry in ipairs(outline) do
  rail[#rail + 1] = rail_entry(entry[1], entry[2], entry[1] == section)
end

ui.render(
  ui.column {
    gap = "none",
    h = 8,
    children = {
      -- Title bar
      ui.bar {
        h = ui.gap.xl,
        pad = ui.gap.md,
        ink = "surface",
        opacity = 0.95,
        border = true,
        children = {
          ui.row {
            widths = { 0.5, 0.5 },
            children = {
              ui.column {
                gap = "none",
                children = {
                  ui.text("Field notes", { style = "subheading", ink = "primary" }),
                  ui.text("spore · markdown · draft", { style = "caption", ink = "muted" }),
                },
              },
              ui.row {
                gap = "xs",
                align = "end",
                children = {
                  ui.icon_button("search", { size = "small", quiet = true, action = "search" }),
                  ui.icon_button("more", { size = "small", quiet = true, action = "more" }),
                  ui.button("Share", { action = "share", body = { value = "share" } }),
                },
              },
            },
          },
        },
      },

      -- Toolbar
      ui.bar {
        h = ui.gap.lg,
        pad = ui.gap.md,
        ink = "surface",
        opacity = 0.85,
        border = true,
        children = {
          ui.row {
            widths = { 0.6, 0.4 },
            children = {
              ui.row {
                gap = "xs",
                children = {
                  ui.button("Read", { small = true, quiet = mode ~= "read",
                                      action = "mode-read", body = { value = "read" } }),
                  ui.button("Edit", { small = true, quiet = mode ~= "edit",
                                      action = "mode-edit", body = { value = "edit" } }),
                  ui.text("  ", { style = "caption" }),
                  ui.icon_button("bold", { size = "small", quiet = true, action = "bold" }),
                  ui.icon_button("italic", { size = "small", quiet = true, action = "italic" }),
                  ui.icon_button("code", { size = "small", quiet = true, action = "code" }),
                  ui.icon_button("link", { size = "small", quiet = true, action = "link" }),
                },
              },
              ui.row {
                gap = "xs",
                align = "end",
                children = {
                  ui.icon("dot", { size = "micro", ink = "success" }),
                  ui.text("saved", { style = "caption", ink = "muted" }),
                },
              },
            },
          },
        },
      },

      -- Contents and document
      ui.row {
        widths = { 0.28, 0.72 },
        gap = "lg",
        grow = true,
        children = {
          ui.column {
            gap = "xs",
            pad = ui.gap.md,
            children = {
              ui.text("CONTENTS", { style = "caption", ink = "muted" }),
              ui.spacer { size = "xs" },
              table.unpack(rail),
            },
          },

          ui.column {
            gap = "sm",
            pad = ui.gap.lg,
            children = {
              ui.text(section, { style = "heading", ink = "primary" }),
              ui.bar {
                pad = ui.gap.sm,
                ink = "surface",
                opacity = 0.7,
                radius = "sm",
                children = {
                  ui.row {
                    widths = { 0.6, 0.4 },
                    children = {
                      ui.text(mode == "edit" and "editing · unsaved" or "read only · draft",
                              { style = "caption", ink = "muted" }),
                      ui.row {
                        align = "end",
                        children = {
                          ui.text(mode == "edit" and "⌘S saves" or "press Edit to type",
                                  { style = "caption", ink = "muted" }),
                        },
                      },
                    },
                  },
                },
              },
              ui.bar {
                pad = ui.gap.sm,
                ink = "primary",
                opacity = 0.10,
                radius = "sm",
                children = {
                  ui.text(
                    "A Markdown reader and editor. The document is data; this file is "
                    .. "the interface. Nothing above it knows how a heading is drawn.",
                    { style = "body" }
                  ),
                },
              },
              ui.text("Content it handles", { style = "subheading", ink = "primary" }),
              ui.bullets {
                "headings, paragraphs, quotes, rules",
                "ordered and unordered lists",
                "inline strong, emphasis, code, links",
                "images and attachments by target",
              },
              ui.code("rust", [[fn main() {
    println!("one scene, three hosts");
}]]),
              ui.diagram("mermaid", "Spore -> Snapshot -> Lua UI -> Canvas"),
              ui.quote("Roles and sizes come from the theme, so a theme change moves the page."),
            },
          },
        },
      },

      -- Status bar
      ui.bar {
        h = ui.gap.lg,
        pad = ui.gap.md,
        ink = "surface",
        opacity = 0.95,
        border = true,
        border_top = true,
        children = {
          ui.row {
            widths = { 0.5, 0.5 },
            children = {
              ui.text("utf-8 · markdown · theme project", { style = "caption", ink = "muted" }),
              ui.row {
                gap = "sm",
                align = "end",
                children = {
                  ui.kbd("⌘S"),
                  ui.text("3:12", { style = "caption", ink = "muted" }),
                },
              },
            },
          },
        },
      },
    },
  },
  module.theme
)
