-- markdown spore · view
--
-- A pure function of the snapshot. The frame, the rail and the document are the
-- same tree: no coordinates, no colours, no callbacks.

local outline = {
  { "Field notes", "h1", 0 },
  { "What this is", "h2", 0 },
  { "Content it handles", "h2", 0 },
  { "Code", "h3", 1 },
  { "Diagrams", "h3", 1 },
  { "Images and attachments", "h3", 1 },
  { "Open questions", "h2", 0 },
}

local function outline_item(entry)
  local label, level, active = entry[1], entry[2], entry[3] == 1
  local props = {
    pad = ui.gap.sm,
    ink = "surface",
    opacity = active and 0.95 or 0.0,
    radius = 4,
    children = {
      ui.text(label, {
        style = "body",
        ink = active and "primary" or (level == "h3" and "muted" or "text"),
      }),
    },
  }
  return ui.bar(props)
end

local rail_items = {}
for _, entry in ipairs(outline) do
  rail_items[#rail_items + 1] = entry[3] == 1 and outline_item({ entry[1], entry[2], 1 })
    or outline_item(entry)
end

ui.render(
  ui.column {
    gap = "none",
    h = 8,
    children = {
      -- Title bar
      ui.bar {
        h = ui.gap.xl,
        bleed = 0.16,
        ink = "surface",
        opacity = 0.95,
        border = true,
        children = {
          ui.row {
            widths = { 0.55, 0.45 },
            children = {
              ui.column {
                gap = "none",
                children = {
                  ui.text("Field notes", { style = "subheading", ink = "primary" }),
                  ui.text("markdown · edit", { style = "caption", ink = "muted" }),
                },
              },
              ui.row {
                children = {
                  ui.text("Read", { style = "caption", ink = "muted" }),
                  ui.text("Edit · 3:12 · unsaved", { style = "caption", ink = "primary" }),
                },
              },
            },
          },
        },
      },

      -- Rail and document
      ui.row {
        widths = { 0.30, 0.70 },
        gap = "md",
        grow = true,
        children = {
          ui.column {
            gap = "xs",
            children = {
              ui.text("CONTENTS", { style = "caption", ink = "muted" }),
              ui.spacer { size = "xs" },
              table.unpack(rail_items),
            },
          },

          ui.column {
            gap = "sm",
            pad = ui.gap.md,
            children = {
              ui.text("What this is", { style = "heading", ink = "primary" }),
              ui.text(
                "A Markdown reader and editor. The document is data; this file is the "
                .. "interface. Nothing above it knows how a heading is drawn.",
                { style = "body" }
              ),
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
              ui.quote(
                "Roles and sizes come from the theme, so a theme change moves the page."
              ),
            },
          },
        },
      },

      -- Status bar
      ui.bar {
        h = ui.gap.lg,
        bleed = 0.16,
        ink = "surface",
        opacity = 0.95,
        border = true,
        border_top = true,
        children = {
          ui.row {
            children = {
              ui.text("utf-8 · markdown · 412 words", { style = "caption", ink = "muted" }),
              ui.text("ln 3, col 12", { style = "caption", ink = "muted" }),
            },
          },
        },
      },
    },
  },
  module.theme
)
