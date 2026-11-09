//! The flat table the Lua view paints.
//!
//! Nothing here draws: it decides names, dates and order, then writes one Lua
//! literal. The view turns bytes into `12.4 GB` and a share into a bar, so any
//! host that hands the view this table draws the same picture. The table is
//! deliberately string-keyed and flat for the same reason the other Spores' are:
//! a host can materialise it without a binding generator, and an agent can read
//! it.

use std::fmt::Write as _;

use crate::scan::{FileRef, KindStat, Scan, Totals};
use crate::vfs::Entry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub id: String,
    pub name: String,
    pub totals: Totals,
    /// False while a background scan has not produced this location's totals
    /// yet. The rail shows "scanning" rather than an empty directory that is
    /// actually a tree nobody has counted.
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crumb {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingRow {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub bytes: u64,
    pub media_type: String,
    /// ISO 8601 date, UTC. Empty when the host did not report a time.
    pub modified: String,
    pub items: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub title: String,
    /// Display name of the selected location.
    pub workspace: String,
    /// Id of the selected location, matching one `roots` entry.
    pub root: String,
    /// Location-relative path of the directory shown. Empty is the location
    /// root.
    pub path: String,
    pub crumbs: Vec<Crumb>,
    pub roots: Vec<Root>,
    pub totals: Totals,
    pub kinds: Vec<KindStat>,
    pub listing: Vec<ListingRow>,
    pub listing_total: usize,
    pub listing_truncated: bool,
    pub largest: Vec<FileRef>,
    pub updated_ms: u64,
    pub error: Option<String>,
}

impl Snapshot {
    pub fn new(
        workspace: &str,
        root: &str,
        path: &str,
        roots: Vec<Root>,
        scan: &Scan,
        updated_ms: u64,
        error: Option<String>,
    ) -> Self {
        Self {
            title: "files".to_string(),
            workspace: workspace.to_string(),
            root: root.to_string(),
            path: path.to_string(),
            crumbs: crumbs(workspace, path),
            roots,
            totals: scan.totals,
            kinds: scan.kinds.clone(),
            listing: scan.listing.iter().map(row).collect(),
            listing_total: scan.listing_total,
            listing_truncated: scan.listing_truncated,
            largest: scan.largest.clone(),
            updated_ms,
            error,
        }
    }

    /// A stable hash of everything that changes the picture, excluding
    /// `updated_ms`. A scanner that found the same tree twice must not make the
    /// page repaint, and a page that repaints an unchanged scene is flicker.
    pub fn fingerprint(&self) -> u64 {
        let mut hash = Fnv::default();
        hash.text(&self.title);
        hash.text(&self.workspace);
        hash.text(&self.root);
        hash.text(&self.path);
        for crumb in &self.crumbs {
            hash.text(&crumb.name);
            hash.text(&crumb.path);
        }
        for root in &self.roots {
            hash.text(&root.id);
            hash.text(&root.name);
            hash.totals(root.totals);
            hash.number(root.available as u64);
        }
        hash.totals(self.totals);
        for kind in &self.kinds {
            hash.text(kind.class.id());
            hash.number(kind.bytes);
            hash.number(kind.files);
        }
        for row in &self.listing {
            hash.text(&row.name);
            hash.text(&row.path);
            hash.text(row.kind);
            hash.number(row.bytes);
            hash.text(&row.media_type);
            hash.text(&row.modified);
            hash.number(row.items.unwrap_or(u64::MAX));
        }
        hash.number(self.listing_total as u64);
        hash.number(self.listing_truncated as u64);
        for file in &self.largest {
            hash.text(&file.path);
            hash.number(file.bytes);
        }
        if let Some(error) = &self.error {
            hash.text(error);
        }
        hash.finish()
    }

    /// The Lua literal `ui/files.lua` is handed.
    pub fn to_lua(&self) -> String {
        let mut out = String::with_capacity(2048);
        out.push_str("return {\n");
        let _ = writeln!(out, "  revision = 1,");
        let _ = writeln!(out, "  title = {},", lua_string(&self.title));
        let _ = writeln!(out, "  workspace = {},", lua_string(&self.workspace));
        let _ = writeln!(out, "  root = {},", lua_string(&self.root));
        let _ = writeln!(out, "  path = {},", lua_string(&self.path));
        let _ = writeln!(out, "  updated_ms = {},", self.updated_ms);

        out.push_str("  crumbs = {\n");
        for crumb in &self.crumbs {
            let _ = writeln!(
                out,
                "    {{ name = {}, path = {} }},",
                lua_string(&crumb.name),
                lua_string(&crumb.path)
            );
        }
        out.push_str("  },\n");

        out.push_str("  roots = {\n");
        for root in &self.roots {
            let _ = writeln!(
                out,
                "    {{ id = {}, name = {}, bytes = {}, files = {}, folders = {}, current = {}, available = {}, errors = {}, truncated = {} }},",
                lua_string(&root.id),
                lua_string(&root.name),
                root.totals.bytes,
                root.totals.files,
                root.totals.folders,
                root.id == self.root,
                root.available,
                root.totals.errors,
                root.totals.truncated
            );
        }
        out.push_str("  },\n");

        let totals = &self.totals;
        let _ = writeln!(
            out,
            "  totals = {{ bytes = {}, files = {}, folders = {}, scanned = {}, errors = {}, truncated = {} }},",
            totals.bytes,
            totals.files,
            totals.folders,
            totals.scanned,
            totals.errors,
            totals.truncated
        );

        out.push_str("  kinds = {\n");
        for kind in &self.kinds {
            let _ = writeln!(
                out,
                "    {{ id = {}, label = {}, bytes = {}, files = {} }},",
                lua_string(kind.class.id()),
                lua_string(kind.class.label()),
                kind.bytes,
                kind.files
            );
        }
        out.push_str("  },\n");

        out.push_str("  listing = {\n");
        for row in &self.listing {
            match row.items {
                Some(items) => {
                    let _ = writeln!(
                        out,
                        "    {{ name = {}, path = {}, kind = {}, bytes = {}, items = {} }},",
                        lua_string(&row.name),
                        lua_string(&row.path),
                        lua_string(row.kind),
                        row.bytes,
                        items
                    );
                }
                None => {
                    let _ = writeln!(
                        out,
                        "    {{ name = {}, path = {}, kind = {}, bytes = {}, media_type = {}, modified = {} }},",
                        lua_string(&row.name),
                        lua_string(&row.path),
                        lua_string(row.kind),
                        row.bytes,
                        lua_string(&row.media_type),
                        lua_string(&row.modified)
                    );
                }
            }
        }
        out.push_str("  },\n");

        let _ = writeln!(out, "  listing_total = {},", self.listing_total);
        let _ = writeln!(out, "  listing_truncated = {},", self.listing_truncated);

        out.push_str("  largest = {\n");
        for file in &self.largest {
            let _ = writeln!(
                out,
                "    {{ name = {}, path = {}, bytes = {} }},",
                lua_string(&file.name),
                lua_string(&file.path),
                file.bytes
            );
        }
        out.push_str("  },\n");

        if let Some(error) = &self.error {
            let _ = writeln!(out, "  error = {},", lua_string(error));
        }
        out.push_str("}\n");
        out
    }
}

/// FNV-1a, 64-bit. The value is only ever compared to itself, so the shortest
/// collider-resistant hash is the right one — this is not a content address.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
        self.bytes(&[0]);
    }

    fn number(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    fn totals(&mut self, totals: Totals) {
        self.number(totals.bytes);
        self.number(totals.files);
        self.number(totals.folders);
        self.number(totals.scanned);
        self.number(totals.errors);
        self.number(totals.truncated as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

fn row(entry: &Entry) -> ListingRow {
    ListingRow {
        name: entry.name.clone(),
        path: entry.path.clone(),
        kind: entry.kind.as_str(),
        bytes: entry.size,
        media_type: entry.media_type.clone(),
        modified: entry.updated_at_ms.map(format_date).unwrap_or_default(),
        items: entry.child_count,
    }
}

/// Location name, then one crumb per path component.
fn crumbs(workspace: &str, path: &str) -> Vec<Crumb> {
    let mut crumbs = vec![Crumb {
        name: workspace.to_string(),
        path: String::new(),
    }];
    let mut walked = String::new();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        if !walked.is_empty() {
            walked.push('/');
        }
        walked.push_str(part);
        crumbs.push(Crumb {
            name: part.to_string(),
            path: walked.clone(),
        });
    }
    crumbs
}

/// An ISO 8601 UTC date. A time of day is not what a file browser shows in a
/// list, and a relative age would need a clock the snapshot does not own.
pub fn format_date(ms: u64) -> String {
    // Day count to civil date, Hinnant's `civil_from_days`
    // <http://howardhinnant.github.io/date_algorithms.html#civil_from_days>.
    // Proleptic Gregorian, no diverging month table to keep in step.
    let days = (ms / 86_400_000) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let mp = (5 * day_of_year + 2) / 153; // [0, 11]
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// A Lua string literal. One argument, `%q`-free: Lua's `%q` is not JSON and
/// this is not Lua's parser either — every control character is written as a
/// three-digit decimal escape so a following digit cannot join it.
fn lua_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                let _ = write!(out, "\\{code:03}", code = control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc_and_gregorian() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_700_000_000_000), "2023-11-14");
        // 2024-02-29: the leap day is the one a hand-rolled month table loses.
        assert_eq!(format_date(1_709_164_800_000), "2024-02-29");
        assert_eq!(format_date(4_102_444_800_000), "2100-01-01");
    }

    #[test]
    fn a_control_character_cannot_join_the_next_digit() {
        assert_eq!(lua_string("a\u{1}2"), "\"a\\0012\"");
        assert_eq!(lua_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn crumbs_start_at_the_location_name() {
        assert_eq!(
            crumbs("Studio", "Photos/2024"),
            vec![
                Crumb {
                    name: "Studio".into(),
                    path: "".into()
                },
                Crumb {
                    name: "Photos".into(),
                    path: "Photos".into()
                },
                Crumb {
                    name: "2024".into(),
                    path: "Photos/2024".into()
                },
            ]
        );
    }
}
