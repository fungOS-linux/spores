//! Read the fungOS VFS namespace directly from its SQLite workspace.
//!
//! `soil:vfs` is the contract the Spore sees; this is the host-side adapter for
//! the stand-in that has no Soil host yet. It reads `vfs.sqlite3` read-only and
//! answers the same `Scan` the directory walker does, but from the namespace
//! table rather than from filesystem metadata. A folder click is one bounded
//! query, not a walk of every descendant's `read_dir`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::scan::{FileRef, KindClass, KindStat, Limits, Scan, Totals};
use crate::vfs::{Entry, Kind};

/// A `fungos-vfs` workspace: a root containing `.fungos/vfs.sqlite3`.
#[derive(Clone, Debug)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, String> {
        let root = root.into();
        if !root.join(".fungos/vfs.sqlite3").is_file() {
            return Err(format!(
                "{} is not a fungOS VFS workspace (no .fungos/vfs.sqlite3)",
                root.display()
            ));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Build the census for `dir` (workspace-relative, empty is the root).
    pub fn scan(&self, dir: &str, limits: Limits) -> Result<Scan, String> {
        let dir = normalize(dir)?;
        let connection = Connection::open_with_flags(
            self.root.join(".fungos/vfs.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|error| format!("open VFS {}: {error}", self.root.display()))?;
        let _ = connection.busy_timeout(Duration::from_millis(500));

        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        // Every descendant begins with `prefix`. `dir0` is the smallest string
        // greater than the prefix: `/` sorts before `0`, so `dir/...` < `dir0`.
        let upper = format!("{dir}0");
        let limit = limits.max_entries.saturating_add(1).min(i64::MAX as u64) as i64;

        let mut file_rows = read_files(&connection, &prefix, &upper, limit)?;
        let mut dir_rows = read_dirs(&connection, &prefix, &upper)?;
        let truncated = file_rows.len() as u64 > limits.max_entries
            || dir_rows.len() as u64 > limits.max_entries;
        file_rows.truncate(limits.max_entries as usize);
        dir_rows.truncate(limits.max_entries as usize);
        let exact_file = is_file(&connection, &dir)?;
        let exact_dir = dir.is_empty() || is_dir(&connection, &dir)?;
        let seen = exact_dir || !file_rows.is_empty() || !dir_rows.is_empty();
        if exact_file {
            return Ok(Scan {
                first_error: Some(format!("{dir}: not a directory")),
                ..Scan::default()
            });
        }
        if !seen {
            return Ok(Scan {
                first_error: Some(format!("{dir}: not found")),
                ..Scan::default()
            });
        }

        let mut totals = Totals::default();
        let mut kinds = KindClass::ALL.map(|class| KindStat {
            class,
            ..KindStat::default()
        });
        let mut largest = Vec::new();
        let mut folders: BTreeSet<String> = BTreeSet::new();
        let mut children: BTreeMap<String, Child> = BTreeMap::new();

        for path in &dir_rows {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let (first, tail) = split_child(rest);
            let child = children.entry(first.to_owned()).or_default();
            child.directory = true;
            child.updated = child.updated.max(Some(0));
            if let Some(tail) = tail {
                child.children.insert(first_segment(tail).to_owned());
            }
        }
        for row in &file_rows {
            let path = &row.path;
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            totals.files += 1;
            totals.bytes = totals.bytes.saturating_add(row.size);
            let class = KindClass::from_media_type(&row.media_type);
            let stat = &mut kinds[class as usize];
            stat.bytes = stat.bytes.saturating_add(row.size);
            stat.files += 1;
            keep_largest(
                &mut largest,
                FileRef {
                    name: last_segment(path).to_owned(),
                    path: path.clone(),
                    bytes: row.size,
                },
                limits.largest,
            );

            // Every implicit directory between `dir` and this file is a folder
            // in the census. The immediate child is the one the listing shows;
            // the rest are what make the folder total honest.
            let mut accumulated = String::new();
            let parts: Vec<&str> = rest.split('/').collect();
            for part in parts.iter().take(parts.len().saturating_sub(1)) {
                if !accumulated.is_empty() {
                    accumulated.push('/');
                }
                accumulated.push_str(part);
                folders.insert(format!("{prefix}{accumulated}"));
            }
            let (first, tail) = split_child(rest);
            let child = children.entry(first.to_owned()).or_default();
            if let Some(tail) = tail {
                child.directory = true;
                child.children.insert(first_segment(tail).to_owned());
                if let Some(updated) = row.updated {
                    child.updated = child.updated.max(Some(updated));
                }
            } else {
                child.directory = false;
                child.size = row.size;
                child.media_type = row.media_type.clone();
                child.updated = row.updated;
            }
        }

        // Explicit empty directories are children too, even though no file
        // under them contributed to the totals above.
        for path in &dir_rows {
            if let Some(rest) = path.strip_prefix(&prefix) {
                if rest.is_empty() {
                    continue;
                }
                folders.insert(path.clone());
                let (first, tail) = split_child(rest);
                let child = children.entry(first.to_owned()).or_default();
                child.directory = true;
                if let Some(tail) = tail {
                    child.children.insert(first_segment(tail).to_owned());
                }
            }
        }

        totals.folders = folders.len() as u64;
        totals.scanned = totals.files.saturating_add(totals.folders);
        totals.truncated = truncated;

        let mut listing: Vec<Entry> = children
            .iter()
            .map(|(name, child)| Entry {
                path: format!("{prefix}{name}"),
                name: name.clone(),
                kind: if child.directory {
                    Kind::Directory
                } else {
                    Kind::File
                },
                size: if child.directory { 0 } else { child.size },
                media_type: if child.directory {
                    String::new()
                } else {
                    child.media_type.clone()
                },
                updated_at_ms: child.updated.filter(|updated| *updated > 0),
                child_count: child.directory.then_some(child.children.len() as u64),
            })
            .collect();
        let listing_total = listing.len();
        listing.sort_by(|a, b| {
            let a_dir = a.kind == Kind::Directory;
            let b_dir = b.kind == Kind::Directory;
            b_dir
                .cmp(&a_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.path.cmp(&b.path))
        });
        let listing_truncated = listing_total > limits.listing_limit;
        listing.truncate(limits.listing_limit);

        let mut kinds: Vec<KindStat> = kinds.into_iter().filter(|stat| stat.files > 0).collect();
        kinds.sort_by(|a, b| {
            b.bytes
                .cmp(&a.bytes)
                .then_with(|| a.class.label().cmp(b.class.label()))
        });
        largest.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));

        Ok(Scan {
            totals,
            kinds,
            largest,
            listing,
            listing_total,
            listing_truncated,
            listing_seen: true,
            first_error: None,
        })
    }
}

#[derive(Default)]
struct Child {
    directory: bool,
    size: u64,
    media_type: String,
    updated: Option<u64>,
    children: BTreeSet<String>,
}

struct FileRow {
    path: String,
    size: u64,
    media_type: String,
    updated: Option<u64>,
}

fn read_files(
    connection: &Connection,
    prefix: &str,
    upper: &str,
    limit: i64,
) -> Result<Vec<FileRow>, String> {
    let sql = if prefix.is_empty() {
        "SELECT path,size,media_type,updated_at_ms FROM files ORDER BY path LIMIT ?1"
    } else {
        "SELECT path,size,media_type,updated_at_ms FROM files \
         WHERE path >= ?1 AND path < ?2 ORDER BY path LIMIT ?3"
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("read VFS namespace: {error}"))?;
    let map = |row: &rusqlite::Row<'_>| -> rusqlite::Result<FileRow> {
        let size: i64 = row.get(1)?;
        let updated: i64 = row.get(3)?;
        Ok(FileRow {
            path: row.get(0)?,
            size: size.max(0) as u64,
            media_type: row.get(2)?,
            updated: (updated > 0).then_some(updated as u64),
        })
    };
    let rows = if prefix.is_empty() {
        statement.query_map(params![limit], map)
    } else {
        statement.query_map(params![prefix, upper, limit], map)
    }
    .map_err(|error| format!("read VFS namespace: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read VFS namespace: {error}"))
}

fn read_dirs(connection: &Connection, prefix: &str, upper: &str) -> Result<Vec<String>, String> {
    let sql = if prefix.is_empty() {
        "SELECT path FROM directories ORDER BY path"
    } else {
        "SELECT path FROM directories WHERE path >= ?1 AND path < ?2 ORDER BY path"
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("read VFS directories: {error}"))?;
    let rows = if prefix.is_empty() {
        statement.query_map([], path_column)
    } else {
        statement.query_map(params![prefix, upper], path_column)
    }
    .map_err(|error| format!("read VFS directories: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read VFS directories: {error}"))
}

fn path_column(row: &rusqlite::Row<'_>) -> rusqlite::Result<String> {
    row.get(0)
}

fn is_file(connection: &Connection, path: &str) -> Result<bool, String> {
    connection
        .query_row("SELECT 1 FROM files WHERE path=?1", params![path], |_| {
            Ok(())
        })
        .optional()
        .map(|found| found.is_some())
        .map_err(|error| format!("read VFS: {error}"))
}

fn is_dir(connection: &Connection, path: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT 1 FROM directories WHERE path=?1",
            params![path],
            |_| Ok(()),
        )
        .optional()
        .map(|found| found.is_some())
        .map_err(|error| format!("read VFS: {error}"))
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

fn split_child(rest: &str) -> (&str, Option<&str>) {
    match rest.split_once('/') {
        Some((first, tail)) => (first, Some(tail)),
        None => (rest, None),
    }
}

fn first_segment(rest: &str) -> &str {
    rest.split('/').next().unwrap_or(rest)
}

fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn normalize(dir: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    for part in dir.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err("path escapes the VFS workspace".into()),
            _ => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn workspace() -> (PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "files-vfs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join(".fungos")).unwrap();
        let connection = Connection::open(root.join(".fungos/vfs.sqlite3")).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE files(
                   path TEXT PRIMARY KEY, content_id TEXT NOT NULL, size INTEGER NOT NULL,
                   media_type TEXT NOT NULL, version INTEGER NOT NULL, hydration TEXT NOT NULL,
                   pinned INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL
                 );
                 CREATE TABLE directories(path TEXT PRIMARY KEY, created_at_ms INTEGER NOT NULL);",
            )
            .unwrap();
        let mut insert = connection
            .prepare(
                "INSERT INTO files(path,content_id,size,media_type,version,hydration,pinned,updated_at_ms)
                 VALUES(?1,'blake3:test',?2,?3,1,'present',0,?4)",
            )
            .unwrap();
        for (path, size, media, updated) in [
            ("notes.txt", 12u64, "text/plain", 1_700_000_000_000i64),
            ("photos/greece-01.jpg", 100, "image/jpeg", 1_700_000_000_100),
            ("photos/greece-02.jpg", 50, "image/jpeg", 1_700_000_000_200),
            (
                "photos/raw/big.mov",
                1_000,
                "video/quicktime",
                1_700_000_000_300,
            ),
        ] {
            insert
                .execute(params![path, size as i64, media, updated])
                .unwrap();
        }
        drop(insert);
        connection
            .execute(
                "INSERT INTO directories(path,created_at_ms) VALUES('empty',1700000000400)",
                [],
            )
            .unwrap();
        drop(connection);
        let workspace = Workspace::open(&root).unwrap();
        (root, workspace)
    }

    #[test]
    fn root_lists_directories_and_files_with_totals() {
        let (_root, workspace) = workspace();
        let scan = workspace.scan("", Limits::default()).unwrap();
        assert!(scan.listing_seen);
        assert_eq!(scan.totals.files, 4);
        assert_eq!(scan.totals.bytes, 1_162);
        assert_eq!(scan.totals.folders, 3); // photos, photos/raw, empty
        assert_eq!(scan.listing_total, 3);
        let names: Vec<_> = scan
            .listing
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, ["empty", "photos", "notes.txt"]);
        assert_eq!(scan.listing[1].kind, Kind::Directory);
        assert_eq!(scan.listing[1].child_count, Some(3));
        assert_eq!(scan.listing[2].size, 12);
        assert_eq!(scan.kinds[0].class, KindClass::Video);
        assert_eq!(scan.largest[0].path, "photos/raw/big.mov");
    }

    #[test]
    fn a_subtree_and_an_empty_directory_are_distinct() {
        let (_root, workspace) = workspace();
        let scan = workspace.scan("photos", Limits::default()).unwrap();
        assert_eq!(scan.totals.files, 3);
        assert_eq!(scan.totals.bytes, 1_150);
        assert_eq!(scan.listing_total, 3); // raw, greece-01, greece-02
        let empty = workspace.scan("empty", Limits::default()).unwrap();
        assert!(empty.listing_seen);
        assert_eq!(empty.totals.files, 0);
        assert!(empty.listing.is_empty());
        let missing = workspace.scan("missing", Limits::default()).unwrap();
        assert!(!missing.listing_seen);
        assert!(missing.first_error.is_some());
        let file = workspace.scan("notes.txt", Limits::default()).unwrap();
        assert!(!file.listing_seen);
        assert!(
            file.first_error
                .as_deref()
                .unwrap()
                .contains("not a directory")
        );
    }

    #[test]
    fn traversal_out_of_the_workspace_is_rejected() {
        let (_root, workspace) = workspace();
        assert!(workspace.scan("../etc", Limits::default()).is_err());
    }
}
