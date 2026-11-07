-- DragShift · the ring.
--
-- A drop opens one ring. Convert and Tools are the same ring with a different
-- set of slices, and a tap toggles between them: the arrangement is what the
-- hand learns, so the two rings must not reflow into each other.
--
-- Everything here is a function of `module.inputs`. The host owns which slice is
-- under the pointer and which one is lit; this file only decides what a ring
-- looks like.

local inputs = module.inputs or {}
local theme = module.theme

-- The fallback ring exists so the view renders before a module supplies one, and
-- so the fixture is a real render rather than a mock. It mirrors `Kind::Image`
-- and `Ring::Convert` in src/lib.rs; a slice that build cannot honour is shown
-- unavailable with the same reason it gives.
local CONVERT = {
  { id = "png", label = "PNG" },
  { id = "jpeg", label = "JPEG" },
  { id = "webp", label = "WebP" },
  { id = "tiff", label = "TIFF" },
  { id = "bmp", label = "BMP" },
  { id = "ico", label = "ICO" },
  { id = "pdf", label = "PDF" },
  { id = "avif", label = "AVIF", available = false },
  { id = "heic", label = "HEIC", available = false },
}

local slices = inputs.slices or CONVERT
local ring = inputs.ring or "convert"
local selected = tonumber(inputs.selected) or 1
local source = tostring(inputs.source or "a file")

ui.render(
  ui.column {
    gap = "none",
    h = 8,
    children = {
      ui.bar {
        h = ui.gap.xl,
        pad = ui.gap.md,
        ink = "surface",
        opacity = 0.95,
        border = true,
        children = {
          ui.row {
            widths = { 0.6, 0.4 },
            children = {
              ui.column {
                gap = "none",
                children = {
                  ui.text(source, { style = "subheading", ink = "primary" }),
                  ui.text(ring == "tools" and "Tools" or "Convert to",
                          { style = "caption", ink = "muted" }),
                },
              },
              ui.row {
                gap = "xs",
                align = "end",
                children = {
                  ui.text("Drop to apply", { style = "caption", ink = "muted" }),
                },
              },
            },
          },
        },
      },

      ui.spacer { size = "md" },
      ui.radial(slices, {
        action = "pick",
        selected = selected,
        diameter = 420,
        inner = 0.42,
        segments = 6,
      }),
      ui.spacer { size = "md" },

      ui.bar {
        h = ui.gap.lg,
        pad = ui.gap.md,
        ink = "surface",
        opacity = 0.9,
        border = true,
        border_top = true,
        children = {
          ui.row {
            widths = { 0.5, 0.5 },
            children = {
              ui.text("the original is never changed", { style = "caption", ink = "muted" }),
              ui.row {
                gap = "sm",
                align = "end",
                children = {
                  ui.kbd("TAB"),
                  ui.text("switch ring", { style = "caption", ink = "muted" }),
                  ui.kbd("ESC"),
                  ui.text("cancel", { style = "caption", ink = "muted" }),
                },
              },
            },
          },
        },
      },
    },
  },
  theme
)
