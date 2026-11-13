//! The tree: what is in the vault, grouped by the directory it is in.
//!
//! This is a listing of the vault's files, not of the document layer's records.
//! A file is what a person opens, and the doc layer's `docs/<id>/…` records are
//! keyed by the document id derived here — the same id, every run, from nothing
//! but the path.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// What the document layer can do with a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A markdown file: the layer's own text view, editable.
    Markdown,
    /// A Word file: imported into a view, read-only, because the container is the
    /// authority for everything the reader does not model.
    Word,
    /// Something this build has no reader for.
    Other,
}

impl Kind {
    fn of(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "md" | "markdown" => Self::Markdown,
            "docx" => Self::Word,
            _ => Self::Other,
        }
    }
}

/// One file a person can click.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Path inside the vault, with `/` separators. `notes/today.md`.
    pub rel: String,
    pub path: PathBuf,
    /// The document layer's identity for this file. Stable across runs.
    pub id: String,
    /// What the layer can do with it. A file it has no reader for is still a
    /// file, and the pane says so instead of pretending.
    pub kind: Kind,
}

impl Document {
    /// The name a row shows.
    pub fn name(&self) -> &str {
        self.rel.rsplit('/').next().unwrap_or(&self.rel)
    }

    /// Whether the layer knows what this file is.
    pub fn modelled(&self) -> bool {
        !matches!(self.kind, Kind::Other)
    }
}

/// One directory's documents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// The directory inside the vault; empty for files at the root.
    pub dir: String,
    pub documents: Vec<Document>,
}

impl Group {
    pub fn title(&self) -> &str {
        if self.dir.is_empty() {
            "vault root"
        } else {
            &self.dir
        }
    }
}

/// Directories that are never part of the vault: build output, version control,
/// the doc layer's own records, and anything else that hides.
const SKIP: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    ".doc-shell",
    ".fungos",
    ".delta",
];

/// How deep a vault is walked. A vault is a place a person reads, not a
/// filesystem root; a depth this generous covers any real one and stops a
/// symlinked home directory from being listed in full.
const MAX_DEPTH: usize = 12;

/// Everything under `root`, grouped by directory.
///
/// Groups are sorted by directory, documents by name, and a hidden directory is
/// skipped rather than shown empty.
pub fn scan(root: &Path) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    walk(root, root, 0, &mut groups);
    groups.sort_by(|a, b| a.dir.cmp(&b.dir));
    for group in &mut groups {
        group
            .documents
            .sort_by_key(|document| document.rel.to_lowercase());
    }
    groups
}

fn walk(root: &Path, dir: &Path, depth: usize, groups: &mut Vec<Group>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut found: Vec<Document> = Vec::new();
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if SKIP.contains(&name.as_str()) || name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        match fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => subdirs.push(path),
            Ok(meta) if meta.is_file() => {
                if let Ok(rel) = path.strip_prefix(root) {
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    found.push(Document {
                        id: document_id(&rel),
                        kind: Kind::of(&path),
                        rel,
                        path: path.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    if !found.is_empty() {
        let rel_dir = dir
            .strip_prefix(root)
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        groups.push(Group {
            dir: rel_dir,
            documents: found,
        });
    }
    subdirs.sort();
    for subdir in subdirs {
        walk(root, &subdir, depth + 1, groups);
    }
}

/// The document layer's id for a path: one VFS path segment, so it cannot escape
/// `docs/<id>/`, and a function of the path alone, so the same file is the same
/// document in every session.
pub fn document_id(rel: &str) -> String {
    let mut id = String::with_capacity(rel.len());
    for character in rel.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            id.push(character);
        } else if character == '.' {
            id.push('.');
        } else {
            id.push('-');
        }
    }
    let id = id.trim_matches('.').to_owned();
    if id.is_empty() {
        "document".to_owned()
    } else {
        id
    }
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_becomes_a_legal_document_id() {
        assert_eq!(document_id("notes/today.md"), "notes-today.md");
        assert_eq!(document_id("Future Present.md"), "Future-Present.md");
        assert_eq!(document_id("a/../../b"), "a-..-..-b");
        assert_eq!(document_id(""), "document");
    }

    #[test]
    fn files_are_grouped_by_directory_and_hidden_directories_are_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("notes/deep")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("README.md"), "# read me").unwrap();
        fs::write(root.join("notes/today.md"), "# today").unwrap();
        fs::write(root.join("notes/deep/idea.md"), "# idea").unwrap();
        fs::write(root.join("notes/data.json"), "{}").unwrap();
        fs::write(root.join(".git/config"), "x").unwrap();
        fs::write(root.join("target/junk.md"), "x").unwrap();

        let groups = scan(root);
        let dirs: Vec<&str> = groups.iter().map(|group| group.dir.as_str()).collect();
        assert_eq!(dirs, vec!["", "notes", "notes/deep"]);

        let root_group = &groups[0];
        assert_eq!(root_group.title(), "vault root");
        assert_eq!(root_group.documents.len(), 1);

        let notes = &groups[1];
        let names: Vec<&str> = notes.documents.iter().map(|doc| doc.name()).collect();
        assert_eq!(names, vec!["data.json", "today.md"]);
        assert!(
            !notes.documents[0].modelled(),
            "a json file is not modelled"
        );
        assert!(notes.documents[1].modelled());
    }

    #[test]
    fn markdown_is_recognised_by_extension_not_by_case() {
        assert!(is_markdown(Path::new("a.md")));
        assert!(is_markdown(Path::new("a.MD")));
        assert!(!is_markdown(Path::new("a.txt")));
    }
}
