//! Pane order and widths, remembered per document.
//!
//! A layout is a property of reading a particular document — the transcript
//! matters more on some than on others — so the record is keyed by document id,
//! with one default for documents nobody has arranged yet. A missing or
//! unreadable file is not an error the user has to solve before reading: it
//! starts from the default and says so, once, where they can see it.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::layout::Layout;

/// What one file holds.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Saved {
    /// Documents nobody has arranged use this; absent means the delta default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<Layout>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    docs: BTreeMap<String, Layout>,
}

/// The layout record on disk, and where it lives.
pub struct Store {
    path: PathBuf,
    saved: Saved,
    /// A load that had to fall back, so the UI can say why the arrangement is not
    /// the one that was left.
    note: Option<String>,
}

impl Store {
    /// Read the record from its usual place.
    ///
    /// `$XDG_CONFIG_HOME/doc-shell/layouts.json`, else `~/.config/...`. A UI
    /// arrangement is per-user state, not project state, so it does not belong in
    /// the vault where it would show up in somebody's diff.
    pub fn load() -> Self {
        match config_path() {
            Some(path) => Self::load_from(path),
            None => Self {
                path: PathBuf::from("layouts.json"),
                saved: Saved::default(),
                note: Some("no config directory: this session's layout is not saved".to_owned()),
            },
        }
    }

    pub fn load_from(path: PathBuf) -> Self {
        let mut store = Self {
            path,
            saved: Saved::default(),
            note: None,
        };
        let bytes = match fs::read_to_string(&store.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return store,
            Err(error) => {
                store.note = Some(format!("layout not read: {error}"));
                return store;
            }
        };
        match serde_json::from_str::<Saved>(&bytes) {
            Ok(saved) => {
                store.saved = saved;
                // A file can hold an arrangement this build cannot render — an
                // older one, a hand edit — and rendering it anyway would put a
                // pane in a place the code does not expect.
                let invalid: Vec<String> = store
                    .saved
                    .docs
                    .iter()
                    .filter(|(_, layout)| !layout.is_valid())
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in invalid {
                    store.saved.docs.remove(&id);
                }
                if store.saved.default.as_ref().is_some_and(|l| !l.is_valid()) {
                    store.saved.default = None;
                }
            }
            Err(error) => {
                store.note = Some(format!(
                    "layout file not understood ({error}); using defaults"
                ));
            }
        }
        store
    }

    /// The note from the last load, when there was something to say.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The arrangement for `document`, or the default.
    ///
    /// `document` is `None` when no document is open: the panes still have an
    /// order, and flipping them with nothing open is still a thing a person
    /// means.
    pub fn layout(&self, document: Option<&str>) -> Layout {
        document
            .and_then(|id| self.saved.docs.get(id))
            .or(self.saved.default.as_ref())
            .cloned()
            .unwrap_or_default()
    }

    /// Record `layout` for `document` and write the file.
    pub fn remember(&mut self, document: Option<&str>, layout: &Layout) -> io::Result<()> {
        match document {
            Some(id) => {
                if self.saved.docs.get(id) == Some(layout) {
                    return Ok(());
                }
                self.saved.docs.insert(id.to_owned(), layout.clone());
            }
            None => {
                if self.saved.default.as_ref() == Some(layout) {
                    return Ok(());
                }
                self.saved.default = Some(layout.clone());
            }
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.saved)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&self.path, bytes)
    }
}

fn config_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let home = std::env::var_os("HOME")?;
            PathBuf::from(home).join(".config")
        }
    };
    Some(base.join("doc-shell").join("layouts.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Pane;

    fn temp_store() -> (tempfile::TempDir, Store) {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::load_from(temp.path().join("layouts.json"));
        (temp, store)
    }

    #[test]
    fn an_absent_file_is_the_default_layout_and_not_a_note() {
        let (_temp, store) = temp_store();
        assert!(store.note().is_none());
        assert_eq!(store.layout(Some("notes")), Layout::delta());
        assert_eq!(store.layout(None), Layout::delta());
    }

    #[test]
    fn a_layout_is_remembered_per_document() {
        let (_temp, mut store) = temp_store();
        let mut flipped = Layout::delta();
        flipped.flip();
        store.remember(Some("notes"), &flipped).unwrap();

        let mut narrow = Layout::delta();
        narrow.set_width(Pane::Sidebar, 200.0);
        store.remember(Some("journal"), &narrow).unwrap();

        let reloaded = Store::load_from(store.path().to_path_buf());
        assert_eq!(reloaded.layout(Some("notes")), flipped);
        assert_eq!(reloaded.layout(Some("journal")), narrow);
        assert_eq!(
            reloaded.layout(Some("unseen")),
            Layout::delta(),
            "a document nobody arranged starts from the default"
        );
    }

    #[test]
    fn flipping_with_no_document_open_is_remembered_too() {
        let (_temp, mut store) = temp_store();
        let mut flipped = Layout::delta();
        flipped.flip();
        store.remember(None, &flipped).unwrap();
        let reloaded = Store::load_from(store.path().to_path_buf());
        assert_eq!(reloaded.layout(Some("anything")), flipped);
    }

    #[test]
    fn a_corrupt_file_keeps_the_defaults_and_says_so() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("layouts.json");
        fs::write(&path, "{ not json").unwrap();
        let store = Store::load_from(path);
        assert!(
            store
                .note()
                .is_some_and(|note| note.contains("not understood"))
        );
        assert_eq!(store.layout(None), Layout::delta());
    }

    #[test]
    fn an_arrangement_that_is_not_a_permutation_is_dropped() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("layouts.json");
        fs::write(
            &path,
            r#"{"docs":{"broken":{"order":["sidebar","document"],"widths":{"sidebar":260.0,"transcript":340.0}}}}"#,
        )
        .unwrap();
        let store = Store::load_from(path);
        assert_eq!(store.layout(Some("broken")), Layout::delta());
    }
}
