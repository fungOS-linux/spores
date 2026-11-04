//! Editor state machine: text, cursor, selection, undo, and mode.
//!
//! Every mutation is bounded and every offset is a UTF-8 character boundary.

use crate::doc::{DocId, Vfs};
use crate::snapshot;
use crate::{Error, MAX_SOURCE_BYTES, MAX_UNDO};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Read,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Zero-based line.
    pub line: usize,
    /// Zero-based character column within the line.
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UndoPoint {
    source: String,
    cursor: usize,
    anchor: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Editor {
    source: String,
    cursor: usize,
    anchor: Option<usize>,
    revision: u64,
    mode: Mode,
    scroll: usize,
    dirty: bool,
    undo: Vec<UndoPoint>,
    redo: Vec<UndoPoint>,
}

impl Editor {
    pub fn new(source: impl Into<String>) -> Result<Self, Error> {
        let source = source.into();
        if source.len() > MAX_SOURCE_BYTES {
            return Err(Error::OverBudget);
        }
        Ok(Self {
            source,
            cursor: 0,
            anchor: None,
            revision: 0,
            mode: Mode::Read,
            scroll: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// Load a document through the VFS boundary.
    pub fn open<V: Vfs>(vfs: &mut V, id: &DocId) -> Result<Self, Error> {
        match vfs.load(id) {
            Ok(Some(source)) => Self::new(source),
            Ok(None) => Err(Error::NotFound),
            Err(_) => Err(Error::Denied),
        }
    }

    /// Persist the document through the VFS boundary.
    pub fn save<V: Vfs>(&mut self, vfs: &mut V, id: &DocId) -> Result<(), Error> {
        vfs.store(id, &self.source).map_err(|_| Error::Denied)?;
        self.dirty = false;
        Ok(())
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn cursor(&self) -> Position {
        self.position_at(self.cursor)
    }

    pub fn line_count(&self) -> usize {
        self.source.split('\n').count()
    }

    pub fn snapshot(&self) -> snapshot::Snapshot {
        snapshot::project(self)
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.touch();
        }
    }

    pub fn set_cursor(&mut self, byte: usize) {
        self.cursor = clamp_boundary(&self.source, byte);
        self.anchor = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.source.len();
    }

    pub fn selection(&self) -> Option<(Position, Position)> {
        let (start, end) = self.selection_range();
        (start != end).then(|| (self.position_at(start), self.position_at(end)))
    }

    pub fn selected_text(&self) -> Option<&str> {
        let (start, end) = self.selection_range();
        (start != end).then(|| &self.source[start..end])
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let last = self.line_count().saturating_sub(1);
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, last as isize) as usize;
    }

    // ---- mutations -------------------------------------------------------

    pub fn insert_str(&mut self, text: &str) -> Result<(), Error> {
        self.require_edit()?;
        if text.is_empty() {
            return Ok(());
        }
        let (start, end) = self.selection_range();
        let new_len = self.source.len() - (end - start) + text.len();
        if new_len > MAX_SOURCE_BYTES {
            return Err(Error::OverBudget);
        }
        self.record_undo();
        self.source.replace_range(start..end, text);
        self.cursor = start + text.len();
        self.anchor = None;
        self.changed();
        Ok(())
    }

    pub fn insert_char(&mut self, c: char) -> Result<(), Error> {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf))
    }

    pub fn newline(&mut self) -> Result<(), Error> {
        self.require_edit()?;
        let text = match self.continuation_prefix() {
            Some(prefix) => format!("\n{prefix}"),
            None => "\n".to_owned(),
        };
        self.insert_str(&text)
    }

    pub fn backspace(&mut self) -> Result<(), Error> {
        self.require_edit()?;
        let (start, end) = self.selection_range();
        if start != end {
            return self.delete_range(start, end);
        }
        if self.cursor == 0 {
            return Ok(());
        }
        let previous = prev_boundary(&self.source, self.cursor);
        self.delete_range(previous, self.cursor)
    }

    pub fn delete_forward(&mut self) -> Result<(), Error> {
        self.require_edit()?;
        let (start, end) = self.selection_range();
        if start != end {
            return self.delete_range(start, end);
        }
        if self.cursor >= self.source.len() {
            return Ok(());
        }
        let next = next_boundary(&self.source, self.cursor);
        self.delete_range(self.cursor, next)
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.undo_point());
        self.restore(previous);
        self.bump_revision();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.undo_point());
        self.restore(next);
        self.bump_revision();
        true
    }

    // ---- movement --------------------------------------------------------

    pub fn move_left(&mut self) {
        self.anchor = None;
        self.cursor = prev_boundary(&self.source, self.cursor);
    }

    pub fn move_right(&mut self) {
        self.anchor = None;
        self.cursor = next_boundary(&self.source, self.cursor);
    }

    pub fn move_home(&mut self) {
        self.anchor = None;
        self.cursor = self.line_start_of(self.cursor);
    }

    pub fn move_end(&mut self) {
        self.anchor = None;
        self.cursor = self.line_end_of(self.cursor);
    }

    pub fn move_up(&mut self) {
        self.anchor = None;
        let start = self.line_start_of(self.cursor);
        if start == 0 {
            self.cursor = 0;
            return;
        }
        let column = self.source[start..self.cursor].chars().count();
        let previous_start = self.line_start_of(start - 1);
        let previous_end = self.line_end_of(previous_start);
        self.cursor =
            previous_start + byte_for_chars(&self.source[previous_start..previous_end], column);
    }

    pub fn move_down(&mut self) {
        self.anchor = None;
        let start = self.line_start_of(self.cursor);
        let end = self.line_end_of(start);
        if end >= self.source.len() {
            self.cursor = self.source.len();
            return;
        }
        let column = self.source[start..self.cursor].chars().count();
        let next_start = end + 1;
        let next_end = self.line_end_of(next_start);
        self.cursor = next_start + byte_for_chars(&self.source[next_start..next_end], column);
    }

    pub fn select_to(&mut self, byte: usize) {
        let target = clamp_boundary(&self.source, byte);
        if self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
        self.cursor = target;
    }

    // ---- internals -------------------------------------------------------

    fn require_edit(&self) -> Result<(), Error> {
        if self.mode == Mode::Edit {
            Ok(())
        } else {
            Err(Error::ReadOnly)
        }
    }

    fn delete_range(&mut self, start: usize, end: usize) -> Result<(), Error> {
        if start >= end {
            return Ok(());
        }
        self.record_undo();
        self.source.replace_range(start..end, "");
        self.cursor = start;
        self.anchor = None;
        self.changed();
        Ok(())
    }

    fn selection_range(&self) -> (usize, usize) {
        match self.anchor {
            Some(anchor) => (anchor.min(self.cursor), anchor.max(self.cursor)),
            None => (self.cursor, self.cursor),
        }
    }

    fn position_at(&self, byte: usize) -> Position {
        let byte = clamp_boundary(&self.source, byte);
        let before = &self.source[..byte];
        let line = before.bytes().filter(|b| *b == b'\n').count();
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        Position {
            line,
            column: self.source[line_start..byte].chars().count(),
        }
    }

    fn line_start_of(&self, byte: usize) -> usize {
        let byte = clamp_boundary(&self.source, byte);
        self.source[..byte].rfind('\n').map_or(0, |index| index + 1)
    }

    fn line_end_of(&self, byte: usize) -> usize {
        let byte = clamp_boundary(&self.source, byte);
        self.source[byte..]
            .find('\n')
            .map_or(self.source.len(), |index| byte + index)
    }

    /// Continue a list marker on newline. This is the only place the editor
    /// inspects Markdown structure, and it never rewrites existing text.
    fn continuation_prefix(&self) -> Option<String> {
        let start = self.line_start_of(self.cursor);
        let end = self.line_end_of(self.cursor);
        let line = &self.source[start..end];
        let indent_len = line.len() - line.trim_start_matches(' ').len();
        let indent = &line[..indent_len];
        let rest = &line[indent_len..];

        for marker in ["- ", "* ", "+ "] {
            if rest.starts_with(marker) {
                return Some(format!("{indent}{marker}"));
            }
        }

        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return None;
        }
        rest[digits.len()..].strip_prefix(". ")?;
        let number: u64 = digits.parse().ok()?;
        Some(format!("{indent}{}. ", number + 1))
    }

    fn record_undo(&mut self) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(self.undo_point());
        self.redo.clear();
    }

    fn undo_point(&self) -> UndoPoint {
        UndoPoint {
            source: self.source.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    fn restore(&mut self, point: UndoPoint) {
        self.source = point.source;
        self.cursor = point.cursor;
        self.anchor = point.anchor;
        self.dirty = true;
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.bump_revision();
    }

    fn touch(&mut self) {
        self.bump_revision();
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

fn clamp_boundary(source: &str, mut byte: usize) -> usize {
    byte = byte.min(source.len());
    while byte > 0 && !source.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn prev_boundary(source: &str, byte: usize) -> usize {
    let byte = clamp_boundary(source, byte);
    if byte == 0 {
        return 0;
    }
    let mut index = byte - 1;
    while index > 0 && !source.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn next_boundary(source: &str, byte: usize) -> usize {
    let byte = clamp_boundary(source, byte);
    if byte >= source.len() {
        return source.len();
    }
    let mut index = byte + 1;
    while index < source.len() && !source.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn byte_for_chars(text: &str, count: usize) -> usize {
    text.char_indices()
        .nth(count)
        .map_or(text.len(), |(index, _)| index)
}
