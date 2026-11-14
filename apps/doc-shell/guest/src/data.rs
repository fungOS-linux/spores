//! Where the vault listing and the document bytes come from.
//!
//! Two builds, one window. The shipped Spore reads one granted workspace
//! through `soil:vfs`: names and bytes both come from the host, and a path the
//! host does not grant does not exist. The `demo` build renders the same window
//! against a vault it writes in memory, because a host that has not linked
//! `soil:vfs` cannot instantiate a component that imports it.

use crate::Row;

/// How much of a document the reader will hold. A Word file past this is not a
/// document this pane can show, and it says so instead of drawing half of it.
pub const DOC_BYTES: u32 = 4 * 1024 * 1024;

/// Read a directory's entries, sorted with directories first.
#[cfg(not(feature = "demo"))]
pub fn ls(path: &str) -> Result<Vec<Row>, String> {
    use crate::soil::vfs::vfs;
    match vfs::ls(path, 0, 500) {
        Ok(page) => Ok(crate::sort(
            page.entries
                .into_iter()
                .map(|entry| Row {
                    name: entry.name.clone(),
                    path: entry.path,
                    directory: matches!(entry.kind, vfs::Kind::Directory),
                    size: entry.size,
                })
                .collect(),
        )),
        Err(error) => Err(format!("{error:?}")),
    }
}

/// Read a file's bytes, bounded.
#[cfg(not(feature = "demo"))]
pub fn read(path: &str, max: u32) -> Result<Vec<u8>, String> {
    use crate::soil::vfs::vfs;
    match vfs::read(path, 0, max) {
        Ok(chunk) => Ok(chunk.data),
        Err(error) => Err(format!("{error:?}")),
    }
}

/// A fixed vault, so the window can be rendered without a namespace.
///
/// The document is a real `.docx` — a zip with Word's own `document.xml` — so
/// the demo runs the shipped reader rather than a stub. Building it here is
/// what keeps a binary fixture out of the repository.
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
    };
    let rows = match path {
        "" => vec![
            row("contracts", true, 0),
            row("nda.docx", false, 6_412),
            row("minutes.docx", false, 2_208),
            row("scope.md", false, 914),
        ],
        _ => vec![row("acme-nda.docx", false, 6_412)],
    };
    Ok(crate::sort(rows))
}

#[cfg(feature = "demo")]
pub fn read(path: &str, _max: u32) -> Result<Vec<u8>, String> {
    match path {
        "minutes.docx" => docx(&[
            (Some("1A2B3C4D"), Some("Heading1"), "Minutes"),
            (Some("5E6F7A8B"), None, "Two people read one document."),
        ]),
        _ => docx(&[
            (Some("1A2B3C4D"), Some("Heading1"), "Mutual non-disclosure"),
            (Some("5E6F7A8B"), None, "The parties agree as follows."),
            (None, None, "Neither party shall disclose it."),
            (Some("9C0D1E2F"), Some("Heading2"), "Term"),
            (
                Some("A1B2C3D4"),
                None,
                "Two years from the date of signature.",
            ),
        ]),
    }
}

/// Build a `.docx` the way Word would: a zip whose `word/document.xml` holds
/// paragraphs, each with the identity Word maintains.
#[cfg(feature = "demo")]
fn docx(paragraphs: &[(Option<&str>, Option<&str>, &str)]) -> Result<Vec<u8>, String> {
    use std::io::{Cursor, Write as _};
    use zip::write::SimpleFileOptions;

    let mut body = String::new();
    for (id, style, text) in paragraphs {
        let identity = match id {
            Some(hex) => format!(r#" w14:paraId="{hex}""#),
            None => String::new(),
        };
        let style = match style {
            Some(name) => format!(r#"<w:pPr><w:pStyle w:val="{name}"/></w:pPr>"#),
            None => String::new(),
        };
        body.push_str(&format!(
            "<w:p{identity}>{style}<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>"
        ));
    }
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"><w:body>{body}</w:body></w:document>"#
    );

    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let options = SimpleFileOptions::default();
        zip.start_file("word/document.xml", options)
            .map_err(|error| error.to_string())?;
        zip.write_all(xml.as_bytes())
            .map_err(|error| error.to_string())?;
        zip.finish().map_err(|error| error.to_string())?;
    }
    Ok(out.into_inner())
}
