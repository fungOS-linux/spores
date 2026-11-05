// Minimal C ABI over libghostty-vt for the terminal Spore.
// The Spore never sees Ghostty structs; it gets a flat cell array.
#include <ghostty/vt.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct {
  uint32_t codepoint;
  uint8_t width;
  uint8_t has_text;
  uint16_t reserved;
  uint32_t fg;
  uint32_t bg;
  uint32_t flags;
} SoilGhosttyCell;

static uint32_t rgb(GhosttyColorRgb c) {
  return ((uint32_t)c.r << 16) | ((uint32_t)c.g << 8) | (uint32_t)c.b;
}

static uint32_t color(GhosttyStyleColor c, const GhosttyColorRgb* palette,
                      uint32_t fallback) {
  switch (c.tag) {
    case GHOSTTY_STYLE_COLOR_RGB:
      return rgb(c.value.rgb);
    case GHOSTTY_STYLE_COLOR_PALETTE:
      return rgb(palette[c.value.palette]);
    default:
      return fallback;
  }
}

void* soil_ghostty_new(uint16_t cols, uint16_t rows) {
  GhosttyTerminal terminal = NULL;
  if (ghostty_terminal_new(NULL, &terminal, cols, rows) != GHOSTTY_SUCCESS) {
    return NULL;
  }
  return terminal;
}

void soil_ghostty_free(void* handle) {
  if (handle) ghostty_terminal_free((GhosttyTerminal)handle);
}

void soil_ghostty_write(void* handle, const uint8_t* bytes, size_t len) {
  ghostty_terminal_vt_write((GhosttyTerminal)handle, bytes, len);
}

void soil_ghostty_resize(void* handle, uint16_t cols, uint16_t rows) {
  ghostty_terminal_resize((GhosttyTerminal)handle, cols, rows, 0, 0);
}

uint32_t soil_ghostty_cursor_x(void* handle) {
  uint16_t value = 0;
  ghostty_terminal_get((GhosttyTerminal)handle, GHOSTTY_TERMINAL_DATA_CURSOR_X, &value);
  return value;
}

uint32_t soil_ghostty_cursor_y(void* handle) {
  uint16_t value = 0;
  ghostty_terminal_get((GhosttyTerminal)handle, GHOSTTY_TERMINAL_DATA_CURSOR_Y, &value);
  return value;
}

uint32_t soil_ghostty_cursor_visible(void* handle) {
  bool value = true;
  ghostty_terminal_get((GhosttyTerminal)handle, GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE, &value);
  return value ? 1 : 0;
}

uint32_t soil_ghostty_title(void* handle, uint8_t* out, size_t cap) {
  GhosttyString value = {0};
  if (ghostty_terminal_get((GhosttyTerminal)handle, GHOSTTY_TERMINAL_DATA_TITLE, &value)
      != GHOSTTY_SUCCESS) {
    return 0;
  }
  size_t len = value.len < cap ? value.len : cap;
  for (size_t i = 0; i < len; i++) out[i] = value.ptr[i];
  return (uint32_t)len;
}

void soil_ghostty_grid(void* handle, uint16_t cols, uint16_t rows,
                       SoilGhosttyCell* out) {
  GhosttyColorRgb palette[256];
  GhosttyColorRgb fg_default = {0};
  GhosttyColorRgb bg_default = {0};
  ghostty_terminal_get((GhosttyTerminal)handle, GHOSTTY_TERMINAL_DATA_COLOR_PALETTE, palette);
  ghostty_terminal_get((GhosttyTerminal)handle, GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND, &fg_default);
  ghostty_terminal_get((GhosttyTerminal)handle, GHOSTTY_TERMINAL_DATA_COLOR_BACKGROUND, &bg_default);

  for (uint16_t y = 0; y < rows; y++) {
    for (uint16_t x = 0; x < cols; x++) {
      SoilGhosttyCell* dst = &out[(size_t)y * cols + x];
      GhosttyPoint point = {
          .tag = GHOSTTY_POINT_TAG_ACTIVE,
          .value = {.coordinate = {.x = x, .y = y}},
      };
      GhosttyGridRef ref = {0};
      if (ghostty_terminal_grid_ref((GhosttyTerminal)handle, point, &ref)
          != GHOSTTY_SUCCESS) {
        continue;
      }

      GhosttyCell cell = {0};
      bool has_text = false;
      GhosttyCellWide wide = GHOSTTY_CELL_WIDE_NARROW;
      uint32_t codepoint = 0;
      GhosttyStyle style = {0};
      ghostty_grid_ref_cell(&ref, &cell);
      ghostty_cell_get(cell, GHOSTTY_CELL_DATA_HAS_TEXT, &has_text);
      ghostty_cell_get(cell, GHOSTTY_CELL_DATA_WIDE, &wide);
      ghostty_cell_get(cell, GHOSTTY_CELL_DATA_CODEPOINT, &codepoint);
      ghostty_grid_ref_style(&ref, &style);

      dst->has_text = has_text ? 1 : 0;
      dst->codepoint = codepoint;
      if (wide == GHOSTTY_CELL_WIDE_SPACER_TAIL) {
        dst->width = 0;
      } else {
        dst->width = wide == GHOSTTY_CELL_WIDE_WIDE ? 2 : (has_text ? 1 : 0);
      }
      dst->fg = color(style.fg_color, palette, rgb(fg_default));
      dst->bg = color(style.bg_color, palette, rgb(bg_default));
      uint32_t flags = 0;
      if (style.flags & GHOSTTY_STYLE_FLAG_BOLD) flags |= 1;
      if (style.flags & GHOSTTY_STYLE_FLAG_ITALIC) flags |= 2;
      if (GHOSTTY_STYLE_UNDERLINE(style) != 0) flags |= 4;
      if (style.flags & GHOSTTY_STYLE_FLAG_INVERSE) flags |= 8;
      if (style.flags & GHOSTTY_STYLE_FLAG_FAINT) flags |= 16;
      if (style.flags & GHOSTTY_STYLE_FLAG_STRIKETHROUGH) flags |= 32;
      dst->flags = flags;
    }
  }
}
