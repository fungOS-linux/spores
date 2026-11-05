-- markdown spore · view
--
-- A pure function of the snapshot. Frame, rail and document are one tree: no
-- coordinates, no colours, no callbacks. Anything that looks pressable is an
-- affordance; wiring presses to actions is the host's next step.

local outline = {
  { "Field notes", "h1", 1 },
  { "What this is", "h2", 1 },
  { "Content it handles", "h2", 1 },
  { "Code", "h3", 0 },
  { "Diagrams", "h3", 0 },
  { "Images and attachments", "h3", 0 },
  { "Open questions", "h2", 0 },
}

local function rail_entry(label, level, active)
  return ui.bar {
    pad = ui.gap.sm,
    ink = "primary",
    opacity = active and 0.16 or 0.0,
    radius = 4,
    children = {
      ui.text(label, {
        style = "body",
        ink = active and "primary" or (level == "h3" and "muted" or "text"),
      }),
    },
  }
end

local rail = {}
for _, entry in ipairs(outline) do
  rail[#rail + 1] = rail_entry(entry[1], entry[2], entry[3] == 1 and entry[1] == "What this is")
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
                  ui.icon_button("search", { size = "small", quiet = true }),
                  ui.icon_button("more", { size = "small", quiet = true }),
                  ui.button("Share"),
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
                  ui.button("Read", { small = true, quiet = true }),
                  ui.button("Edit", { small = true, selected = true }),
                  ui.text("  ", { style = "caption" }),
                  ui.icon_button("bold", { size = "small", quiet = true }),
                  ui.icon_button("italic", { size = "small", quiet = true }),
                  ui.icon_button("code", { size = "small", quiet = true }),
                  ui.icon_button("link", { size = "small", quiet = true }),
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
              ui.text("What this is", { style = "heading", ink = "primary" }),
              ui.bar {
                pad = ui.gap.sm,
                ink = "primary",
                opacity = 0.10,
                radius = 4,
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
