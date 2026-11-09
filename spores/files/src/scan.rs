//! The census: one bounded walk of a namespace, and everything a storage view
//! can say from it without a second pass.
//!
//! The walk is the only place that decides what a file is worth. It is bounded in
//! three ways — entries visited, directory depth, and page size — and every bound
//! that actually bites reports itself (`truncated`) instead of quietly showing a
//! total that is smaller than the tree.

use crate::vfs::{Entry, Kind, Vfs};

/// Entries one scan may visit. A tree past this reports its total as truncated;
/// it never presents a partial sum as the whole.
pub const MAX_ENTRIES: u64 = 100_000;
pub const MAX_DEPTH: u32 = 24;
pub const LISTING_LIMIT: usize = 200;
pub const LARGEST_LIMIT: usize = 5;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_entries: u64,
    pub max_depth: u32,
    pub page: u32,
    pub listing_limit: usize,
    pub largest: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: MAX_ENTRIES,
            max_depth: MAX_DEPTH,
            page: crate::vfs::PAGE,
            listing_limit: LISTING_LIMIT,
            largest: LARGEST_LIMIT,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub bytes: u64,
    pub files: u64,
    pub folders: u64,
    /// Entries visited, which is what the budget counts.
    pub scanned: u64,
    /// Directories whose listing failed. The walk continues; the total says so.
    pub errors: u64,
    /// A budget stopped the walk before the tree ended.
    pub truncated: bool,
}

/// The classes a storage view groups by.
///
/// Classification is a function of the media type `soil:vfs` already carries.
/// It never looks at a filename: two hosts that report the same media type must
/// produce the same picture, and a name is not a type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(usize)]
pub enum KindClass {
    Images = 0,
    Video = 1,
    Audio = 2,
    Text = 3,
    Code = 4,
    Documents = 5,
    Archives = 6,
    #[default]
    Other = 7,
}

impl KindClass {
    pub const ALL: [Self; 8] = [
        Self::Images,
        Self::Video,
        Self::Audio,
        Self::Text,
        Self::Code,
        Self::Documents,
        Self::Archives,
        Self::Other,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Text => "text",
            Self::Code => "code",
            Self::Documents => "documents",
            Self::Archives => "archives",
            Self::Other => "other",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Images => "Images",
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Text => "Text",
            Self::Code => "Code",
            Self::Documents => "Documents",
            Self::Archives => "Archives",
            Self::Other => "Other",
        }
    }

    pub fn from_media_type(media_type: &str) -> Self {
        let (family, subtype) = media_type.split_once('/').unwrap_or((media_type, ""));
        match family {
            "image" => Self::Images,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "text" => match subtype {
                "x-rust" | "x-python" | "x-lua" | "x-shellscript" | "x-c" | "x-c++" | "x-go"
                | "x-java" | "x-kotlin" | "x-ruby" | "x-php" | "x-sql" | "x-typescript"
                | "x-jsx" | "javascript" | "html" | "css" | "xml" => Self::Code,
                _ => Self::Text,
            },
            "application" => match subtype {
                "pdf"
                | "epub+zip"
                | "msword"
                | "rtf"
                | "vnd.ms-excel"
                | "vnd.ms-powerpoint"
                | "vnd.oasis.opendocument.text"
                | "vnd.oasis.opendocument.spreadsheet"
                | "x-iwork-pages"
                | "x-iwork-numbers"
                | "x-iwork-keynote" => Self::Documents,
                "zip" | "gzip" | "x-tar" | "x-bzip2" | "x-xz" | "zstd" | "x-7z-compressed"
                | "vnd.rar" | "x-apple-diskimage" | "x-iso9660-image" => Self::Archives,
                "wasm" | "json" | "toml" | "x-yaml" => Self::Code,
                _ if subtype.starts_with("vnd.openxmlformats-officedocument") => Self::Documents,
                _ => Self::Other,
            },
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct KindStat {
    pub class: KindClass,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub totals: Totals,
    /// Classes with at least one file, largest first.
    pub kinds: Vec<KindStat>,
    /// The largest files seen, largest first, bounded by `Limits::largest`.
    pub largest: Vec<FileRef>,
    /// Direct children of the requested directory, directories first, then by
    /// name. Truncated to `Limits::listing_limit`; `listing_total` is the count
    /// before truncation.
    pub listing: Vec<Entry>,
    pub listing_total: usize,
    pub listing_truncated: bool,
    /// The requested directory was opened. False means it does not exist (or is
    /// not a directory), which is different from an existing empty one.
    pub listing_seen: bool,
    pub first_error: Option<String>,
}

/// Walk `root` and collect totals, a class census, the largest files, and the
/// listing of `dir` (which must be `root` or below it).
pub fn scan(vfs: &mut dyn Vfs, root: &str, dir: &str, limits: Limits) -> Scan {
    let mut result = Scan::default();
    let mut counts = KindClass::ALL.map(|class| KindStat {
        class,
        ..KindStat::default()
    });
    let mut stack = vec![(root.to_string(), 0u32)];
    let mut stopped = false;

    'walk: while let Some((parent, depth)) = stack.pop() {
        let mut offset = 0u64;
        loop {
            let page = match vfs.ls(&parent, offset, limits.page) {
                Ok(page) => {
                    if parent == dir {
                        result.listing_seen = true;
                    }
                    page
                }
                Err(error) => {
                    result.totals.errors += 1;
                    if result.first_error.is_none() {
                        result.first_error = Some(format!("{parent}: {error}"));
                    }
                    break;
                }
            };
            for entry in page.entries {
                if result.totals.scanned >= limits.max_entries {
                    result.totals.truncated = true;
                    stopped = true;
                    break;
                }
                result.totals.scanned += 1;
                if parent == dir {
                    result.listing_total += 1;
                    if result.listing.len() < limits.listing_limit {
                        result.listing.push(entry.clone());
                    }
                }
                match entry.kind {
                    Kind::Directory => {
                        result.totals.folders += 1;
                        if depth >= limits.max_depth {
                            // The directory is counted; its children are not.
                            result.totals.truncated = true;
                        } else {
                            stack.push((entry.path, depth + 1));
                        }
                    }
                    Kind::File => {
                        result.totals.files += 1;
                        result.totals.bytes += entry.size;
                        let slot =
                            &mut counts[KindClass::from_media_type(&entry.media_type) as usize];
                        slot.bytes += entry.size;
                        slot.files += 1;
                        keep_largest(
                            &mut result.largest,
                            FileRef {
                                name: entry.name,
                                path: entry.path,
                                bytes: entry.size,
                            },
                            limits.largest,
                        );
                    }
                }
            }
            if stopped {
                break 'walk;
            }
            match page.next {
                // A host that answers with the same offset twice would loop
                // forever; a page that does not advance is the end.
                Some(next) if next > offset => offset = next,
                _ => break,
            }
        }
    }

    // Directories first, then case-insensitive name. The listing is already the
    // host's path order, so this is a stable re-sort, not a second traversal.
    result.listing.sort_by(|a, b| {
        let a_dir = a.kind == Kind::Directory;
        let b_dir = b.kind == Kind::Directory;
        b_dir
            .cmp(&a_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    result.listing_truncated = result.listing_total > result.listing.len();

    result.kinds = counts.into_iter().filter(|stat| stat.files > 0).collect();
    result.kinds.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.class.label().cmp(b.class.label()))
    });

    result
        .largest
        .sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));

    result
}

fn keep_largest(largest: &mut Vec<FileRef>, entry: FileRef, limit: usize) {
    if limit == 0 {
        return;
    }
    let position = largest
        .iter()
        .position(|other| entry.bytes > other.bytes)
        .unwrap_or(largest.len());
    largest.insert(position, entry);
    largest.truncate(limit);
}
