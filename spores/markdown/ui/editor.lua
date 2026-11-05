-- markdown spore · view
--
-- A pure function of the snapshot: return a tree, get a page. No state, no
-- callbacks, no coordinates. Change a token or a line of copy and the running
-- app follows.

ui.render(
  ui.column {
    gap = "md",
    children = {
      ui.row {
        children = {
          ui.text("Field notes", { style = "title" }),
          ui.text("edit · 3:12 · unsaved", { style = "caption", ink = "muted" }),
        },
      },
      ui.rule {},

      ui.text("What this is", { style = "heading", ink = "primary" }),
      ui.text(
        "A Markdown reader and editor. The document is data; this file is the interface. "
        .. "Nothing above it knows how a heading is drawn.",
        { style = "body" }
      ),

      ui.text("Content it handles", { style = "subheading", ink = "primary" }),
      ui.bullets {
        "headings, paragraphs, quotes, rules",
        "ordered and unordered lists",
        "inline strong, emphasis, code, links",
        "images and attachments by target",
        "fenced code and host-rendered diagrams",
      },

      ui.code("rust", [[fn main() {
    println!("one scene, three hosts");
}]]),

      ui.diagram("mermaid", "Spore -> Snapshot -> Lua UI -> Canvas"),

      ui.quote(
        "The snapshot carries no colours, fonts, or coordinates. Roles and sizes come "
        .. "from the theme, so a theme change moves the whole page."
      ),

      ui.rule {},
      ui.text("ui/editor.lua · rendered from disk · declarative", { style = "caption", ink = "muted" }),
    },
  },
  module.theme
)
