//! The one module that knows what the document layer's API looks like.
//!
//! Everything above this line — panes, layout, transcript view — talks to the
//! document layer through these functions and nowhere else. That is deliberate:
//! the layer lives in its own repository and lands on its own schedule, so when a
//! signature moves there is exactly one file to fix, and the rest of the app
//! cannot quietly grow a second opinion about what a document is.
//!
//! What is *not* here: any copy of the layer's logic. If a call is missing from
//! [`fungos_vfs`], the fix belongs upstream, not in a re-implementation here.
//! See README.md for the symbols this milestone found missing.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::{Context, Result};
use fungos_vfs::{
    Anchor, CommentFile, ContentId, Doc, DocFile, Docx, DocxImportReport, DocxReadReport, Expect,
    NodeRole, Op, Reconciled, Resolution, Session, SessionSummary, Thread, Turn, Vfs,
};

/// A vault and the document layer's records about it.
///
/// The records live under `<vault>/.doc-shell/`: a root of its own, so a vault is
/// one directory that can be moved, copied or deleted without stranding state
/// somewhere else on the machine. The content store is local for the same reason
/// — a demo vault should not write into the machine's shared one.
#[derive(Clone)]
pub struct Store {
    vfs: Rc<RefCell<Vfs>>,
    actor: String,
    state_dir: PathBuf,
}

impl Store {
    /// Open (or create) the records for `vault`, writing as `actor`.
    pub fn open(vault: &Path, actor: &str) -> Result<Self> {
        let state_dir = vault.join(".doc-shell");
        let root = state_dir.join("vfs");
        let cas = state_dir.join("cas");
        let vfs = Vfs::open_with(&root, NodeRole::Full, Some(&cas))
            .with_context(|| format!("opening the document layer's root at {}", root.display()))?;
        Ok(Self {
            vfs: Rc::new(RefCell::new(vfs)),
            actor: actor.to_owned(),
            state_dir,
        })
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// A document bound to the file that displays it.
    pub fn open_document(&self, id: &str, path: &Path) -> Result<DocFile> {
        let vfs = self.vfs.borrow();
        DocFile::open(&vfs, id, &self.actor, path)
    }

    /// Read the file's edits, merge them, and write the merged text back.
    pub fn reconcile(&self, file: &mut DocFile) -> Result<Reconciled> {
        let mut vfs = self.vfs.borrow_mut();
        file.reconcile(&mut vfs)
    }

    /// Publish ops this replica authored, and write the merged text.
    pub fn extend(&self, file: &mut DocFile, ops: Vec<Op>) -> Result<Reconciled> {
        let mut vfs = self.vfs.borrow_mut();
        file.extend(&mut vfs, ops)
    }

    /// Where an imported file's markdown view lives.
    ///
    /// A Word file is not written back: the container stays the authority, and the
    /// thing this window can edit, snapshot and comment on is the view it was
    /// imported into. Keeping it under `.doc-shell/` means the vault never gains
    /// files the person did not put there.
    #[must_use]
    pub fn view_path(&self, id: &str) -> PathBuf {
        let safe: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        self.state_dir.join("views").join(format!("{safe}.md"))
    }

    /// Import a Word file as `id`, and hand back the document bound to its view.
    ///
    /// The read is the layer's (`Docx::read`), the identity is Word's own
    /// paragraph ids, so importing an edited file matches paragraphs rather than
    /// diffing text. Returns the document plus what the read and the import saw.
    pub fn import_word(
        &self,
        id: &str,
        source: &Path,
    ) -> Result<(DocFile, DocxReadReport, DocxImportReport)> {
        let bytes =
            std::fs::read(source).with_context(|| format!("reading {}", source.display()))?;
        let docx = Docx::read(&bytes, id)?;
        let document = self.doc(id).unwrap_or_else(|_| Doc::new(&self.actor));
        let (ops, imported) = docx.ops(&document, &self.actor);
        let view = self.view_path(id);
        if let Some(parent) = view.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut file = self.open_document(id, &view)?;
        self.reconcile(&mut file)?;
        self.extend(&mut file, ops)?;
        Ok((file, docx.report, imported))
    }

    /// A document as the root holds it, without binding it to a file.
    pub fn doc(&self, id: &str) -> Result<Doc> {
        let vfs = self.vfs.borrow();
        fungos_vfs::load_doc(&vfs, id, &self.actor)
    }

    /// The threads on `id`, resolved against `doc` as it stands now.
    ///
    /// Read-only this milestone: opening a thread and replying are writes, and a
    /// reader that could write would be a second author of the annotations.
    pub fn threads(&self, id: &str, doc: &Doc) -> Result<Vec<ThreadView>> {
        let vfs = self.vfs.borrow();
        match CommentFile::open(&vfs, id, &self.actor) {
            Ok(comments) => Ok(comments
                .threads(doc)
                .into_iter()
                .map(|(thread, resolution)| ThreadView::new(doc, thread, &resolution))
                .collect()),
            // A root that has never been commented on has no `comments/` prefix
            // at all; that is an empty list, not a failure to show.
            Err(error) if is_missing(&error) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    /// Every session record the root holds, oldest first.
    pub fn sessions(&self) -> Vec<SessionSummary> {
        let vfs = self.vfs.borrow();
        vfs.sessions(None).unwrap_or_else(|error| {
            eprintln!("doc-shell: sessions not listed: {error}");
            Vec::new()
        })
    }

    pub fn session(&self, harness: &str, native_id: &str) -> Option<Session> {
        let vfs = self.vfs.borrow();
        match vfs.session(harness, native_id) {
            Ok(session) => session,
            Err(error) => {
                eprintln!("doc-shell: session {harness}/{native_id} not read: {error}");
                None
            }
        }
    }

    /// A payload filed by digest, as text: a turn's words, a thought, a tool's
    /// arguments or the output it answered with.
    pub fn text(&self, id: Option<&ContentId>) -> Option<String> {
        let id = id?;
        let vfs = self.vfs.borrow();
        match vfs.content(id) {
            Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
            Err(error) => {
                eprintln!("doc-shell: payload {} not read: {error}", id.hex());
                None
            }
        }
    }

    /// Open a thread on the `index`-th live block of `id` and say `body` in it.
    ///
    /// The window is read-only this milestone; this exists so the demo fixture can
    /// put an annotation where a reader can see it without a second
    /// implementation of the comment record. `range` is bytes into the block, as
    /// the layer defines it.
    pub fn annotate(
        &self,
        id: &str,
        index: usize,
        range: Option<(usize, usize)>,
        body: &str,
    ) -> Result<()> {
        let doc = self.doc(id)?;
        let block = doc
            .live()
            .get(index)
            .map(|block| block.id.clone())
            .with_context(|| format!("document {id} has no live block {index}"))?;
        let anchor = match range {
            Some((start, end)) => Anchor::Range { block, start, end },
            None => Anchor::Block { block },
        };
        let mut comments = {
            let vfs = self.vfs.borrow();
            CommentFile::open(&vfs, id, &self.actor)?
        };
        let mut vfs = self.vfs.borrow_mut();
        comments.open_thread(&mut vfs, anchor, body, now_ms())?;
        Ok(())
    }

    /// Import a harness's own session store. A fixture tool: the window reads
    /// records, it does not import them.
    pub fn import_pi(&self, pi_store: &Path) -> Result<usize> {
        let mut vfs = self.vfs.borrow_mut();
        let report = fungos_vfs::import_pi(&mut vfs, pi_store, false)?;
        Ok(report.sessions)
    }

    /// Write bytes the layer would write, for the demo vault only.
    ///
    /// Kept here rather than in the fixture script so the fixture uses the same
    /// root, the same actor and the same id derivation as the app.
    pub fn put(&self, path: &str, bytes: &[u8], media_type: &str) -> Result<()> {
        let mut vfs = self.vfs.borrow_mut();
        vfs.put(path, bytes, media_type, Expect::Any)?;
        Ok(())
    }
}

/// Milliseconds since the epoch, as this process reads the clock.
///
/// A comment carries when it was said, because a counter cannot order two
/// writers: the layer takes the reading rather than reaching for a clock of its
/// own, so a caller (and a test) can supply one.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn is_missing(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<fungos_vfs::VfsError>()
            .is_some_and(|error| matches!(error, fungos_vfs::VfsError::NotFound(_)))
    })
}

/// One thread, flattened for a view: what it is attached to, where that is now,
/// and what was said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadView {
    /// The anchor as the layer names it: `12@studio`, plus a byte range when the
    /// thread points inside the block.
    pub anchor: String,
    /// `exact` / `clamped` / `neighbour` / `orphan` — the layer's own words.
    pub state: &'static str,
    /// The text of the block the thread points at now. Empty when orphaned.
    pub block: String,
    /// The anchored block is a tombstone: the thread outlived what it was about.
    pub tombstoned: bool,
    pub resolved: bool,
    pub comments: Vec<ReplyView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyView {
    pub author: String,
    pub body: String,
}

impl ThreadView {
    fn new(doc: &Doc, thread: &Thread, resolution: &Resolution) -> Self {
        let anchored = doc.block(thread.anchor.block());
        // Show the block the thread is *on*, even if it was deleted: a reader
        // looking at an orphan needs to see what went away, and the neighbour is
        // already named by the resolution state.
        let block = anchored.map(|block| block.text.clone()).unwrap_or_default();
        let tombstoned = anchored.is_none_or(|block| block.deleted);
        let range = match &thread.anchor {
            fungos_vfs::Anchor::Range { start, end, .. } => Some(format!("{start}–{end}")),
            fungos_vfs::Anchor::Block { .. } => None,
        };
        let mut anchor = format!("block {}", thread.anchor.block());
        if let Some(range) = range {
            anchor.push_str(&format!(", bytes {range}"));
        }
        Self {
            anchor,
            state: resolution.state(),
            block,
            tombstoned,
            resolved: thread.resolved(),
            comments: thread
                .comments
                .iter()
                .map(|comment| ReplyView {
                    author: comment.author.clone(),
                    body: comment.body.clone(),
                })
                .collect(),
        }
    }
}

/// One tool call, with its payloads read out of the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolView {
    pub name: String,
    pub arguments: String,
    pub result: Option<String>,
    pub failed: bool,
}

/// One turn, with the payloads the record points at resolved to text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnView {
    pub index: u32,
    pub role: &'static str,
    pub at_ms: i64,
    pub text: String,
    pub thinking: Option<String>,
    pub tools: Vec<ToolView>,
    pub model: Option<String>,
}

impl TurnView {
    pub fn build(store: &Store, turn: &Turn) -> Self {
        let tools = turn
            .tools
            .iter()
            .map(|tool| ToolView {
                name: tool.name.clone(),
                arguments: store
                    .text(Some(&tool.arguments))
                    .map(|text| tidy_json(&text))
                    .unwrap_or_else(|| "(arguments missing)".to_owned()),
                result: store.text(tool.result.as_ref()),
                failed: tool.failed,
            })
            .collect();
        Self {
            index: turn.index,
            role: match turn.role {
                fungos_vfs::Role::System => "system",
                fungos_vfs::Role::User => "user",
                fungos_vfs::Role::Assistant => "assistant",
                fungos_vfs::Role::Tool => "tool",
            },
            at_ms: turn.at_ms,
            text: store.text(turn.text.as_ref()).unwrap_or_default(),
            thinking: store.text(turn.thinking.as_ref()),
            tools,
            model: turn.model.clone(),
        }
    }
}

/// JSON on one line, if it is JSON at all: a tool's arguments are a value, not
/// prose, and an unparsed blob is shown exactly as it was filed.
fn tidy_json(text: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => serde_json::to_string(&value).unwrap_or_else(|_| text.to_owned()),
        Err(_) => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_argument_json_blob_is_kept_on_one_line() {
        assert_eq!(tidy_json("{\n  \"path\": \"a\"\n}"), r#"{"path":"a"}"#);
        assert_eq!(tidy_json("not json"), "not json");
    }
}
