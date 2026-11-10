//! The namespace this Spore reads.
//!
//! [`Vfs`] is the shape of `soil:vfs@1.0.0`'s `ls`/`page` calls without the WIT
//! types: the host names a workspace-relative path, returns one bounded page in
//! path order, and says where the next page starts. The Spore never sees a host
//! path, a file descriptor, or a URL — that is the whole of the read grant.
//!
//! Two implementations exist today. [`StdFs`] is the host-side stand-in that
//! `files-serve` supplies until a Soil host links the module to the real
//! `soil:vfs`; [`MemoryVfs`] is what the tests drive. The trait is the seam the
//! real host plugs into, so moving to `soil:vfs` changes an adapter, not the
//! census.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

/// Host-bounded page width. `soil:vfs` caps a page at 512; 256 leaves room for
/// an adapter that returns fewer than asked.
pub const PAGE: u32 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
}

impl Kind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
        }
    }
}

/// One namespace entry, as `soil:vfs.entry` describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Canonical workspace-relative path; empty is the workspace root.
    pub path: String,
    pub name: String,
    pub kind: Kind,
    /// Bytes for a file; 0 for a directory.
    pub size: u64,
    /// Host-derived media type. Empty for a directory or an unknown file.
    pub media_type: String,
    pub updated_at_ms: Option<u64>,
    /// Direct children, when the host can say. `None` is "unknown", not zero.
    pub child_count: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub entries: Vec<Entry>,
    /// Offset for the next page, or `None` when this page is the end.
    pub next: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The `soil:vfs` grant was not linked for this Spore.
    Denied,
    NotFound,
    NotDirectory,
    OverBudget,
    /// The path is not a canonical workspace-relative path.
    Invalid,
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied => f.write_str("vfs grant denied"),
            Self::NotFound => f.write_str("not found"),
            Self::NotDirectory => f.write_str("not a directory"),
            Self::OverBudget => f.write_str("page budget exceeded"),
            Self::Invalid => f.write_str("invalid workspace path"),
            Self::Io(message) => write!(f, "host i/o: {message}"),
        }
    }
}

impl std::error::Error for Error {}

pub trait Vfs {
    fn ls(&mut self, parent: &str, offset: u64, limit: u32) -> Result<Page, Error>;
}

/// The host-side namespace: a real directory tree, reached without granting the
/// Spore a path.
///
/// The workspace root is chosen once, by the host; every path the Spore asks for
/// is relative to it and is rejected if it escapes (`..`, an absolute path, or
/// an empty component other than the root). Symlinks are entries, not doors: a
/// link is a file whose size is the link, so a tree cannot loop or escape
/// through one.
pub struct StdFs {
    base: PathBuf,
}

impl StdFs {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, Error> {
        if path.starts_with('/') || path.starts_with('\\') {
            return Err(Error::Invalid);
        }
        let mut out = self.base.clone();
        for part in path.split('/') {
            match part {
                "" | "." => continue,
                ".." => return Err(Error::Invalid),
                _ => out.push(part),
            }
        }
        Ok(out)
    }
}

impl Vfs for StdFs {
    fn ls(&mut self, parent: &str, offset: u64, limit: u32) -> Result<Page, Error> {
        if limit == 0 {
            return Err(Error::Invalid);
        }
        let path = self.resolve(parent)?;
        let read = std::fs::read_dir(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound,
            std::io::ErrorKind::PermissionDenied => Error::Denied,
            std::io::ErrorKind::NotADirectory => Error::NotDirectory,
            _ => Error::Io(error.to_string()),
        })?;

        let mut entries = Vec::new();
        for item in read {
            let item = match item {
                Ok(item) => item,
                // One unreadable entry must not hide its readable siblings; the
                // host keeps the policy, this adapter keeps the page.
                Err(_) => continue,
            };
            // `DirEntry::metadata` does not follow a symlink, which is exactly
            // the policy this namespace states: a link is a cheap file.
            let meta = match item.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            let name = item.file_name().to_string_lossy().into_owned();
            let entry_path = if parent.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", parent.trim_end_matches('/'), name)
            };
            let directory = meta.is_dir();
            entries.push(Entry {
                path: entry_path,
                name: name.clone(),
                kind: if directory {
                    Kind::Directory
                } else {
                    Kind::File
                },
                size: if directory { 0 } else { meta.len() },
                media_type: if directory {
                    String::new()
                } else {
                    media_type(&name).to_string()
                },
                updated_at_ms: meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|age| age.as_millis() as u64),
                // `soil:vfs` reports this; a host that walks its own tree on
                // demand does not pay for one `read_dir` per child.
                child_count: None,
            });
        }
        // `soil:vfs` promises listing in path order; the adapter has to sort,
        // because `read_dir` order is whatever the filesystem feels like.
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        let limit = usize::min(limit as usize, 4096);
        // The next offset is relative to the page we just served, not to the
        // directory's length: `entries.len() > limit` is true for every page of
        // a 300-entry directory, so it kept handing back a next offset forever
        // and the census never finished.
        let next = if offset as usize + limit < entries.len() {
            Some(offset + limit as u64)
        } else {
            None
        };
        let entries = entries
            .into_iter()
            .skip(offset as usize)
            .take(limit)
            .collect();
        Ok(Page { entries, next })
    }
}

/// Extension to media type, for the host that has no content sniffer.
///
/// This table belongs to the host, not the Spore: `soil:vfs` carries the media
/// type a host derived, and a different host may derive it from content instead
/// of a name. Keeping it here means the `files` view groups by the one field it
/// is given rather than guessing from filenames.
pub fn media_type(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" | "heif" => "image/heic",
        "tif" | "tiff" => "image/tiff",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "dng" => "image/x-adobe-dng",
        "raw" | "cr2" | "nef" => "image/x-raw",
        "mp4" => "video/mp4",
        "m4v" => "video/x-m4v",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mpg" | "mpeg" => "video/mpeg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "ogg" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aif" | "aiff" => "audio/aiff",
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "rtf" => "application/rtf",
        "pages" => "application/x-iwork-pages",
        "numbers" => "application/x-iwork-numbers",
        "key" => "application/x-iwork-keynote",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "bz2" => "application/x-bzip2",
        "xz" => "application/x-xz",
        "zst" => "application/zstd",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "dmg" => "application/x-apple-diskimage",
        "iso" => "application/x-iso9660-image",
        "wasm" => "application/wasm",
        "json" | "jsonl" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/x-yaml",
        "txt" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "rst" => "text/x-rst",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "jsx" => "text/x-jsx",
        "ts" | "tsx" => "text/x-typescript",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "lua" => "text/x-lua",
        "sh" | "bash" | "zsh" | "fish" => "text/x-shellscript",
        "c" | "h" => "text/x-c",
        "cc" | "cpp" | "cxx" | "hpp" => "text/x-c++",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "kt" | "kts" => "text/x-kotlin",
        "rb" => "text/x-ruby",
        "php" => "text/x-php",
        "sql" => "text/x-sql",
        "xml" => "text/xml",
        "ini" | "conf" | "cfg" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// An in-memory namespace for tests and fixtures.
///
/// It implements the same paging contract as the host, including the errors a
/// wrong caller gets, so a test of the census is a test of the code that ships.
pub struct MemoryVfs {
    children: BTreeMap<String, Vec<Entry>>,
}

impl Default for MemoryVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryVfs {
    pub fn new() -> Self {
        let mut children = BTreeMap::new();
        children.insert(String::new(), Vec::new());
        Self { children }
    }

    pub fn dir(&mut self, path: &str) -> &mut Self {
        self.ensure_dir(path);
        self
    }

    pub fn file(&mut self, path: &str, size: u64, media_type: &str) -> &mut Self {
        self.file_at(path, size, media_type, 0)
    }

    pub fn file_at(
        &mut self,
        path: &str,
        size: u64,
        media_type: &str,
        updated_at_ms: u64,
    ) -> &mut Self {
        let parent = parent_of(path);
        self.ensure_dir(parent);
        let entry = Entry {
            path: path.to_string(),
            name: name_of(path),
            kind: Kind::File,
            size,
            media_type: media_type.to_string(),
            updated_at_ms: (updated_at_ms > 0).then_some(updated_at_ms),
            child_count: None,
        };
        let siblings = self.children.entry(parent.to_string()).or_default();
        siblings.retain(|other| other.path != path);
        siblings.push(entry);
        self
    }

    fn ensure_dir(&mut self, path: &str) {
        if path.is_empty() || self.children.contains_key(path) {
            return;
        }
        let parent = parent_of(path);
        self.ensure_dir(parent);
        self.children.insert(path.to_string(), Vec::new());
        let entry = Entry {
            path: path.to_string(),
            name: name_of(path),
            kind: Kind::Directory,
            size: 0,
            media_type: String::new(),
            updated_at_ms: None,
            child_count: None,
        };
        let siblings = self.children.entry(parent.to_string()).or_default();
        if !siblings.iter().any(|other| other.path == path) {
            siblings.push(entry);
        }
    }
}

impl Vfs for MemoryVfs {
    fn ls(&mut self, parent: &str, offset: u64, limit: u32) -> Result<Page, Error> {
        if limit == 0 {
            return Err(Error::Invalid);
        }
        if parent.starts_with('/') || parent.split('/').any(|part| part == "..") {
            return Err(Error::Invalid);
        }
        let Some(children) = self.children.get(parent) else {
            // A path that names a file is not a missing directory.
            let is_file = self
                .children
                .get(parent_of(parent))
                .is_some_and(|siblings| {
                    siblings
                        .iter()
                        .any(|entry| entry.path == parent && entry.kind == Kind::File)
                });
            return Err(if is_file {
                Error::NotDirectory
            } else {
                Error::NotFound
            });
        };
        let mut entries = children.clone();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let limit = usize::min(limit as usize, 4096);
        let next = if entries.len() > offset as usize + limit {
            Some(offset + limit as u64)
        } else {
            None
        };
        let entries = entries
            .into_iter()
            .skip(offset as usize)
            .take(limit)
            .collect();
        Ok(Page { entries, next })
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}
