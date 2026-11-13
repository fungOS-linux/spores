//! One document, open.
//!
//! A text view accepts edits as text. It does not produce ops and it does not
//! hold a copy of the document: what it holds is a buffer that came from the
//! document and can be handed back as text, at which point the file on disk is
//! written and [`DocFile::reconcile`] turns the difference into ops. That is the
//! whole authoring path — the same one a person in `$EDITOR` takes — and it is
//! why there is no second merge implementation here.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use fungos_vfs::DocFile;

use crate::doclayer::{Store, ThreadView};

/// How the document is being shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// The markdown source, editable.
    Source,
    /// The same document rendered. Read-only: this milestone has no in-block
    /// editing, so the preview is a view and not an editor.
    Preview,
}

impl View {
    pub fn label(self) -> &'static str {
        match self {
            View::Source => "source",
            View::Preview => "preview",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            View::Source => View::Preview,
            View::Preview => View::Source,
        }
    }
}

/// What the last save did, in the layer's own terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub ops: usize,
    pub wrote_file: bool,
    pub deferred: usize,
    pub replaced_unseen_text: bool,
}

impl Report {
    /// The line a status bar shows.
    pub fn line(&self) -> String {
        let mut line = format!("{} op{}", self.ops, if self.ops == 1 { "" } else { "s" });
        line.push_str(if self.wrote_file {
            " · file rewritten from the merged document"
        } else {
            " · file already said it"
        });
        if self.deferred > 0 {
            line.push_str(&format!(
                " · {} op(s) waiting for a block that never arrived",
                self.deferred
            ));
        }
        if self.replaced_unseen_text {
            line.push_str(" · replaced text this worktree had not seen");
        }
        line
    }
}

/// A markdown file and the document it is a view of.
pub struct OpenDocument {
    pub id: String,
    pub rel: String,
    pub path: PathBuf,
    pub file: DocFile,
    pub view: View,
    /// The text the source view is editing. The document, not this, is the
    /// record.
    pub buffer: String,
    pub last: Option<Report>,
    pub threads: Vec<ThreadView>,
    /// Why the threads could not be read, when they could not. An empty thread
    /// list and an unreadable one are different facts.
    pub threads_error: Option<String>,
    /// A file whose container is the authority: read-only here, because writing
    /// the merged markdown back into a `.docx` is a converter this window does not
    /// have and should not pretend to be.
    pub read_only: bool,
    /// What the import saw, for the footer: paragraphs, ids, what was not
    /// modelled. `None` for a document that was already text.
    pub imported: Option<String>,
}

impl OpenDocument {
    /// Open `id` at `path`, reading the file into the document on the first pass.
    pub fn open(store: &Store, id: &str, rel: &str, path: &Path) -> Result<Self> {
        let mut file = store.open_document(id, path)?;
        // The first reconcile is what makes the text already on disk the
        // document: nothing has been seen before, so the file is the base rather
        // than an edit.
        let report = store.reconcile(&mut file)?;
        let buffer = file.render();
        let (threads, threads_error) = threads_of(store, id, file.doc());
        Ok(Self {
            id: id.to_owned(),
            rel: rel.to_owned(),
            path: path.to_path_buf(),
            file,
            view: View::Preview,
            buffer,
            last: Some(Report::from(report)),
            threads,
            threads_error,
            read_only: false,
            imported: None,
        })
    }

    /// Import a Word file and open the view of it.
    ///
    /// The paragraph identities are Word's own, so opening the file again after
    /// somebody edited it in Word matches paragraphs instead of rewriting them —
    /// which is what keeps a thread on a clause attached to that clause.
    pub fn open_word(store: &Store, id: &str, rel: &str, path: &Path) -> Result<Self> {
        let (file, read, imported) = store.import_word(id, path)?;
        let buffer = file.render();
        let (threads, threads_error) = threads_of(store, id, file.doc());
        let mut imported_line = format!(
            "imported · {} paragraphs ({} by id, {} by position) · {} new blocks, {} rewritten",
            read.paragraphs, read.identified, read.synthetic, imported.inserted, imported.updated
        );
        if !read.unmodeled.is_empty() {
            let named: Vec<String> = read
                .unmodeled
                .iter()
                .map(|(name, count)| format!("{name} ×{count}"))
                .collect();
            imported_line.push_str(&format!(" · not modelled: {}", named.join(", ")));
        }
        Ok(Self {
            id: id.to_owned(),
            rel: rel.to_owned(),
            path: path.to_path_buf(),
            file,
            view: View::Preview,
            buffer,
            last: None,
            threads,
            threads_error,
            read_only: true,
            imported: Some(imported_line),
        })
    }

    /// The document as the merge layer renders it. Never the buffer: what the
    /// preview shows is the document, so a preview cannot disagree with a save.
    pub fn rendered(&self) -> String {
        self.file.render()
    }

    /// Write the edited text to the file and let the layer turn it into ops.
    pub fn save(&mut self, store: &Store) -> Result<Report> {
        if self.read_only {
            anyhow::bail!(
                "{} is imported, not edited here: the container stays the authority",
                self.rel
            );
        }
        fs::write(&self.path, &self.buffer)
            .with_context(|| format!("writing {}", self.path.display()))?;
        let report = store.reconcile(&mut self.file)?;
        // The merged text is what stands now; the view is handed it back so it
        // is not showing an edit that lost.
        self.buffer = self.file.render();
        let (threads, threads_error) = threads_of(store, &self.id, self.file.doc());
        self.threads = threads;
        self.threads_error = threads_error;
        let report = Report::from(report);
        self.last = Some(report.clone());
        Ok(report)
    }

    pub fn blocks(&self) -> usize {
        self.file.doc().len()
    }
}

/// The threads on a document, or why there are none to show.
///
/// An empty list and an unreadable comments file are two different answers and
/// the pane shows them differently.
fn threads_of(store: &Store, id: &str, doc: &fungos_vfs::Doc) -> (Vec<ThreadView>, Option<String>) {
    match store.threads(id, doc) {
        Ok(threads) => (threads, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    }
}

impl From<fungos_vfs::Reconciled> for Report {
    fn from(reconciled: fungos_vfs::Reconciled) -> Self {
        Self {
            ops: reconciled.ops,
            wrote_file: reconciled.wrote_file,
            deferred: reconciled.deferred,
            replaced_unseen_text: reconciled.replaced_unseen_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, Store) {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), "studio").unwrap();
        (temp, store)
    }

    #[test]
    fn opening_a_file_makes_it_the_document_and_editing_it_produces_ops() {
        let (temp, store) = vault();
        let path = temp.path().join("NOTES.md");
        fs::write(&path, "# Notes\n\nfirst paragraph\n").unwrap();

        let mut open = OpenDocument::open(&store, "notes", "NOTES.md", &path).unwrap();
        assert_eq!(open.blocks(), 2, "a heading and a paragraph");

        open.buffer = "# Notes\n\nfirst paragraph, edited\n".to_owned();
        let report = open.save(&store).unwrap();
        assert_eq!(report.ops, 1, "one block rewritten");
        assert!(!report.wrote_file, "the file already said it");
        assert!(open.rendered().contains("first paragraph, edited"));
    }

    #[test]
    fn the_preview_shows_the_document_not_the_buffer() {
        let (temp, store) = vault();
        let path = temp.path().join("NOTES.md");
        fs::write(&path, "# Notes\n\nbody\n").unwrap();
        let mut open = OpenDocument::open(&store, "notes", "NOTES.md", &path).unwrap();
        open.buffer = "unsaved nonsense".to_owned();
        assert!(open.rendered().contains("body"));
        assert!(!open.rendered().contains("nonsense"));
    }
}
