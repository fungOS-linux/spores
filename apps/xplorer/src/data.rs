//! Where the list and the preview come from.
//!
//! Two builds, one window. The shipped Spore reads one granted workspace through
//! `soil:vfs`: paths and bytes both come from the host, and a path the host does
//! not grant does not exist. The `demo` build exists because a host that has not
//! linked that grant cannot instantiate a component which imports it — so the
//! arrangement can still be looked at, against a fixed listing.

use crate::Row;

/// Read a directory's entries, sorted with directories first.
#[cfg(not(feature = "demo"))]
pub fn ls(path: &str) -> Result<Vec<Row>, String> {
    use soil::vfs::vfs;
    match vfs::ls(path, 0, crate::PAGE) {
        Ok(page) => Ok(sort(
            page.entries
                .into_iter()
                .map(|entry| Row {
                    name: if entry.name.is_empty() {
                        entry.path.clone()
                    } else {
                        entry.name.clone()
                    },
                    path: entry.path,
                    directory: matches!(entry.kind, vfs::Kind::Directory),
                    size: entry.size,
                    hydrated: matches!(entry.hydration, vfs::Hydration::Present),
                })
                .collect(),
        )),
        Err(error) => Err(format!("{error:?}")),
    }
}

/// Read the first `max` bytes of a file: bytes, content id, and whether that was
/// the whole object.
#[cfg(not(feature = "demo"))]
pub fn read(path: &str, max: u32) -> Result<(Vec<u8>, String, bool), String> {
    use soil::vfs::vfs;
    match vfs::read(path, 0, max) {
        Ok(chunk) => Ok((chunk.data, chunk.content_id, chunk.eof)),
        Err(error) => Err(format!("{error:?}")),
    }
}

fn sort(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

/// A fixed listing, so the window can be rendered without a namespace.
#[cfg(feature = "demo")]
pub fn ls(path: &str) -> Result<Vec<Row>, String> {
    let row = |name: &str, directory: bool, size: u64| Row {
        name: name.to_owned(),
        path: if path.is_empty() {
            name.to_owned()
        } else {
            format!("{path}/{name}")
        },
        directory,
        size,
        hydrated: true,
    };
    let rows = match path {
        "" => vec![
            row("notes", true, 0),
            row("projects", true, 0),
            row("vendor", true, 0),
            row("README.md", false, 1_842),
            row("tend.lua", false, 6),
            row("wit-provenance.txt", false, 914),
            row("design-notes.md", false, 2_310),
            row("render.rs", false, 4_190),
            row("Cargo.toml", false, 402),
            row("budget.xlsx", false, 22_110),
            row("launch.pptx", false, 1_204_300),
            row("thesis.pdf", false, 3_908_772),
            row("sketch.png", false, 88_400),
            row("voice-memo.m4a", false, 512_000),
        ],
        "projects" => vec![
            row("fungos-vfs.md", false, 436),
            row("q3-review.pptx", false, 18),
            row("scope.docx", false, 762),
        ],
        _ => vec![row("today.md", false, 512), row("scratch.md", false, 66)],
    };
    Ok(sort(rows))
}

#[cfg(feature = "demo")]
pub fn read(_path: &str, _max: u32) -> Result<(Vec<u8>, String, bool), String> {
    Ok((
        b"# Scope\n\nA document layer that a Spore can read: blocks with identities,\n\
          a file as a view of them, and threads anchored to blocks rather than to\n\
          byte offsets.\n\nThis pane is what a preview looks like when the bytes are\n\
          already on this machine.\n"
            .to_vec(),
        "blake3:demo".to_owned(),
        true,
    ))
}
