//! Minimal Rust FFI over libghostty-vt.
//!
//! The C shim (`native/ghostty_shim.c`) hides Ghostty structs and returns a
//! flat cell array. The archive is linked only into this Spore.

use std::ffi::c_void;
use terminal_core::{Cell, Grid, Style};

#[repr(C)]
#[derive(Clone, Copy)]
struct RawCell {
    codepoint: u32,
    width: u8,
    has_text: u8,
    reserved: u16,
    fg: u32,
    bg: u32,
    flags: u32,
}

unsafe extern "C" {
    fn soil_ghostty_new(cols: u16, rows: u16) -> *mut c_void;
    fn soil_ghostty_free(handle: *mut c_void);
    fn soil_ghostty_write(handle: *mut c_void, bytes: *const u8, len: usize);
    fn soil_ghostty_resize(handle: *mut c_void, cols: u16, rows: u16);
    fn soil_ghostty_cursor_x(handle: *mut c_void) -> u32;
    fn soil_ghostty_cursor_y(handle: *mut c_void) -> u32;
    fn soil_ghostty_cursor_visible(handle: *mut c_void) -> u32;
    fn soil_ghostty_title(handle: *mut c_void, out: *mut u8, cap: usize) -> u32;
    fn soil_ghostty_grid(handle: *mut c_void, cols: u16, rows: u16, out: *mut RawCell);
}

pub struct Terminal {
    handle: *mut c_void,
    cols: u16,
    rows: u16,
}

impl Terminal {
    pub fn new(cols: u32, rows: u32) -> Result<Self, String> {
        let cols = u16::try_from(cols).map_err(|_| "columns over budget")?;
        let rows = u16::try_from(rows).map_err(|_| "rows over budget")?;
        let handle = unsafe { soil_ghostty_new(cols, rows) };
        if handle.is_null() {
            return Err("libghostty-vt could not create a terminal".into());
        }
        Ok(Self { handle, cols, rows })
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        unsafe { soil_ghostty_write(self.handle, bytes.as_ptr(), bytes.len()) };
    }

    pub fn resize(&mut self, cols: u32, rows: u32) {
        if let (Ok(cols), Ok(rows)) = (u16::try_from(cols), u16::try_from(rows)) {
            unsafe { soil_ghostty_resize(self.handle, cols, rows) };
            self.cols = cols;
            self.rows = rows;
        }
    }

    pub fn grid(&self) -> Grid {
        let cols = u32::from(self.cols);
        let rows = u32::from(self.rows);
        let mut raw = vec![
            RawCell {
                codepoint: 0,
                width: 0,
                has_text: 0,
                reserved: 0,
                fg: 0,
                bg: 0,
                flags: 0,
            };
            (cols as usize) * (rows as usize)
        ];
        unsafe { soil_ghostty_grid(self.handle, self.cols, self.rows, raw.as_mut_ptr()) };

        let cells = raw
            .into_iter()
            .map(|cell| {
                let text = if cell.has_text != 0 {
                    char::from_u32(cell.codepoint)
                        .map(|ch| ch.to_string())
                        .unwrap_or_default()
                } else {
                    " ".to_string()
                };
                Cell {
                    text,
                    width: cell.width.max(1),
                    style: Style {
                        fg: cell.fg,
                        bg: cell.bg,
                        bold: cell.flags & 1 != 0,
                        italic: cell.flags & 2 != 0,
                        underline: cell.flags & 4 != 0,
                        reverse: cell.flags & 8 != 0,
                        dim: cell.flags & 16 != 0,
                        strikethrough: cell.flags & 32 != 0,
                    },
                }
            })
            .collect();

        let mut title = vec![0u8; 256];
        let title_len = unsafe { soil_ghostty_title(self.handle, title.as_mut_ptr(), title.len()) };
        title.truncate(title_len as usize);
        let title = String::from_utf8(title)
            .ok()
            .filter(|value| !value.is_empty());

        Grid {
            cols,
            rows,
            cells,
            cursor_row: unsafe { soil_ghostty_cursor_y(self.handle) },
            cursor_col: unsafe { soil_ghostty_cursor_x(self.handle) },
            cursor_visible: unsafe { soil_ghostty_cursor_visible(self.handle) } != 0,
            title,
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        unsafe { soil_ghostty_free(self.handle) };
    }
}
