//! Search Spore over `soil:vfs` + `soil:index`.
//!
//! The host owns the workspace namespace, canonical paths, content bytes and
//! the derived index. This Spore owns only the query projection: it asks the
//! index for recall, reads the authoritative entry and a bounded preview
//! through `soil:vfs`, and emits one flat JSON snapshot that Canvas renders.
//!
//! An empty query browses the workspace root in path order; a non-empty query
//! searches the index in `substring` mode (the fast grep/FZF default). The
//! Spore never chooses a provider, endpoint, token or lease, and never writes.

use serde_json::{Value, json};

wit_bindgen::generate!({ path: "wit", world: "search-spore", generate_all });

use soil::index::index::{self, Mode, Order};
use soil::vfs::vfs::{self, Kind};

/// Host-bounded page width. The host caps `ls`/`browse`/`search` at 512.
const PAGE: u32 = 256;
/// How many result paths to materialise (stat each for authoritative metadata).
const MAX_RESULTS: usize = 256;
/// Bounded preview read for a text entry. The host caps a single range at 1 MiB.
const PREVIEW_LEN: u32 = 16 * 1024;

struct Entry {
    path: String,
    name: String,
    media_type: String,
    size: u64,
    kind: Kind,
    snippet: Option<String>,
    score: u32,
    matched_by: Option<String>,
}

impl Entry {
    fn is_dir(&self) -> bool {
        self.kind == Kind::Directory
    }
}

struct Snapshot {
    mode: &'static str,
    ready: bool,
    generation: u64,
    total: usize,
    results: Vec<Entry>,
    error: Option<String>,
}

struct State {
    query: String,
    snapshot: Snapshot,
}

struct Component;
export!(Component);

impl Guest for Component {
    fn snapshot(query: String) -> Result<Vec<u8>, String> {
        let trimmed = query.trim().to_string();
        let snapshot = if trimmed.is_empty() {
            browse()
        } else {
            search(&trimmed)
        };
        render(&State {
            query: trimmed,
            snapshot,
        })
    }
}

/// Browse the workspace root in path order when there is no query.
fn browse() -> Snapshot {
    let state = index::state().ok();
    let generation = state.as_ref().map(|s| s.generation).unwrap_or(0);
    let ready = state.map(|s| s.ready).unwrap_or(false);
    let mut results = Vec::new();
    let mut offset = 0u64;
    while let Ok(page) = index::browse("", Order::Path, offset, PAGE) {
        for hit in page.hits {
            if let Some(entry) = stat_entry(&hit.path) {
                results.push(entry);
            }
            if results.len() >= MAX_RESULTS {
                return done("browse", generation, ready, results, None);
            }
        }
        match page.next {
            Some(next) => offset = next,
            None => break,
        }
    }
    done("browse", generation, ready, results, None)
}

/// Search the derived index in substring mode and materialise each hit.
fn search(query: &str) -> Snapshot {
    let state = index::state().ok();
    let generation = state.as_ref().map(|s| s.generation).unwrap_or(0);
    let ready = state.map(|s| s.ready).unwrap_or(false);
    let mut results = Vec::new();
    let mut offset = 0u64;
    while let Ok(page) = index::search(&index::Query {
        text: query.to_string(),
        mode: Some(Mode::Substring),
        parent: None,
        media_type_prefix: None,
        offset,
        limit: PAGE,
    }) {
        for hit in page.hits {
            let snippet = hit.snippet.clone();
            let matched_by = hit.matched_by.map(|m| format!("{m:?}").to_lowercase());
            if let Some(entry) = stat_entry(&hit.path) {
                results.push(Entry {
                    snippet,
                    score: hit.score,
                    matched_by,
                    ..entry
                });
            }
            if results.len() >= MAX_RESULTS {
                return done("search", generation, ready, results, None);
            }
        }
        match page.next {
            Some(next) => offset = next,
            None => break,
        }
    }
    done("search", generation, ready, results, None)
}

fn done(
    mode: &'static str,
    generation: u64,
    ready: bool,
    mut results: Vec<Entry>,
    error: Option<String>,
) -> Snapshot {
    // Browse is path-ordered by the host index. Search is relevance-ordered and
    // must not be re-sorted here: doing that silently threw away the ranking the
    // user just asked for.
    if mode == "browse" {
        results.sort_by(|a, b| a.path.cmp(&b.path));
    }
    Snapshot {
        mode: if generation == 0 && !ready { "not-ready" } else { mode },
        ready,
        generation,
        total: results.len(),
        results,
        error,
    }
}

/// Read the authoritative entry for `path`; `None` means it dropped away.
fn stat_entry(path: &str) -> Option<Entry> {
    let entry = vfs::stat(path).ok()?;
    Some(Entry {
        name: entry.name,
        media_type: entry.media_type,
        size: entry.size,
        kind: entry.kind,
        snippet: None,
        score: 0,
        matched_by: None,
        ..path_entry(path)
    })
}

fn path_entry(path: &str) -> Entry {
    let name = path
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string());
    Entry {
        path: path.to_string(),
        name,
        media_type: String::new(),
        size: 0,
        kind: Kind::File,
        snippet: None,
        score: 0,
        matched_by: None,
    }
}

fn render(state: &State) -> Result<Vec<u8>, String> {
    let results: Vec<Value> = state
        .snapshot
        .results
        .iter()
        .map(|r| {
            json!({
                "path": r.path,
                "name": r.name,
                "media_type": r.media_type,
                "size": r.size,
                "kind": kind_label(r.kind),
                "is_dir": r.is_dir(),
                "snippet": r.snippet,
                "score": r.score,
                "matched_by": r.matched_by,
            })
        })
        .collect();

    let selected = state.snapshot.results.first();
    let detail = selected.map(|r| {
        let mut detail = json!({
            "path": r.path,
            "name": r.name,
            "media_type": r.media_type,
            "size": r.size,
            "kind": kind_label(r.kind),
            "is_dir": r.is_dir(),
        });
        if !r.is_dir() {
            if let Some(preview) = preview(&r.path, &r.media_type) {
                detail
                    .as_object_mut()
                    .expect("object")
                    .insert("preview".into(), Value::String(preview));
            }
        }
        detail
    });

    let snapshot = json!({
        "title": "Search",
        "query": state.query,
        "workspace": "vfs",
        "mode": state.snapshot.mode,
        "ready": state.snapshot.ready,
        "generation": state.snapshot.generation,
        "total": state.snapshot.total,
        "error": state.snapshot.error,
        "selected_path": selected.map(|r| r.path.as_str()).unwrap_or(""),
        "filters": [
            {"id": "all", "label": "All files", "count": state.snapshot.total, "selected": true},
        ],
        "results": results,
        "detail": detail,
    });
    serde_json::to_vec(&snapshot).map_err(|error| error.to_string())
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Directory => "directory",
        Kind::File => "file",
    }
}

/// Decode a bounded range as lossy UTF-8 for a text entry, dropping raw control
/// bytes so the preview is safe to lay out. Returns `None` for binary media.
fn preview(path: &str, media_type: &str) -> Option<String> {
    if !is_text(media_type) {
        return None;
    }
    let chunk = vfs::read(path, 0, PREVIEW_LEN).ok()?;
    let text = String::from_utf8_lossy(&chunk.data);
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control() || *c == '\t' || *c == '\n' || *c == '\r')
        .collect();
    Some(cleaned.trim().to_string())
}

fn is_text(media_type: &str) -> bool {
    media_type == "text/plain"
        || media_type.starts_with("text/")
        || media_type == "application/json"
        || media_type == "application/xml"
        || media_type.ends_with("+json")
        || media_type.ends_with("+xml")
}
