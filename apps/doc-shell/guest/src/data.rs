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
    let row = |name: &str, directory: bool| {
        let path = if path.is_empty() {
            name.to_owned()
        } else {
            format!("{path}/{name}")
        };
        // A size is the length of the bytes the reader would actually get, not
        // a number written next to the name.
        let size = if directory {
            0
        } else {
            read(&path, DOC_BYTES).map_or(0, |bytes| bytes.len() as u64)
        };
        Row {
            name: name.to_owned(),
            path,
            directory,
            size,
        }
    };
    let rows = match path {
        "" => vec![
            row("contracts", true),
            row("minutes.docx", false),
            row("nda.docx", false),
            row("scope.md", false),
        ],
        _ => vec![row("acme-nda.docx", false)],
    };
    Ok(crate::sort(rows))
}

#[cfg(feature = "demo")]
pub fn read(path: &str, _max: u32) -> Result<Vec<u8>, String> {
    match path {
        "minutes.docx" => docx(&MINUTES),
        _ => docx(&NDA),
    }
}

/// An agreement with a heading, two sections and a signature block: enough
/// shape to show section rhythm, an identity Word did not write, and a thread
/// in each of its states.
#[cfg(feature = "demo")]
const NDA: [(Option<&str>, Option<&str>, &str); 12] = [
    (Some("1A2B3C4D"), Some("Heading1"), "Mutual non-disclosure"),
    (
        Some("5E6F7A8B"),
        None,
        "Between Acme Corporation and the studio, covering what each party learns while the work is being scoped.",
    ),
    (
        None,
        None,
        "Neither party shall disclose it to a third party without written consent.",
    ),
    (Some("9C0D1E2F"), Some("Heading2"), "Scope"),
    (
        Some("7F0A1B2C"),
        None,
        "Confidential is anything marked as such, or anything a reasonable person would treat as private: drafts, pricing, roadmaps, source.",
    ),
    (
        Some("3D4E5F60"),
        None,
        "Feedback on a published draft is not confidential.",
    ),
    (Some("8A9B0C1D"), Some("Heading2"), "Term"),
    (
        Some("A1B2C3D4"),
        None,
        "Two years from the date of signature.",
    ),
    (Some("2C3D4E5F"), Some("Heading2"), "Governing law"),
    (
        Some("6B7C8D9E"),
        None,
        "Washington State, and the courts in King County.",
    ),
    (Some("4E5F6071"), Some("Heading1"), "Signatures"),
    (
        Some("1F2E3D4C"),
        None,
        "Acme Corporation and the studio, 14 March.",
    ),
];

/// Longer than the pane: what the block cursor keeps in view.
#[cfg(feature = "demo")]
const MINUTES: [(Option<&str>, Option<&str>, &str); 26] = [
    (Some("0A1B2C3D"), Some("Heading1"), "Minutes"),
    (Some("1B2C3D4E"), None, "Two people read one document."),
    (Some("2C3D4E5F"), Some("Heading2"), "Present"),
    (Some("3D4E5F60"), None, "studio, neo."),
    (Some("4E5F6071"), Some("Heading2"), "Decisions"),
    (
        Some("5F607182"),
        None,
        "The reader shows a paragraph by the identity Word wrote into it.",
    ),
    (
        Some("60718293"),
        None,
        "A comment is an entity, so it lives in the records book and not in the file.",
    ),
    (
        Some("718293A4"),
        None,
        "Ordering comes from the revision the host assigns, never from a clock.",
    ),
    (
        Some("8293A4B5"),
        None,
        "The rail narrows before a line of prose does.",
    ),
    (Some("93A4B5C6"), Some("Heading2"), "Open questions"),
    (
        Some("A4B5C6D7"),
        None,
        "Does a resolved thread still print? Yes, and it dims.",
    ),
    (Some("B5C6D7E8"), None, "Who signs the second copy?"),
    (
        Some("C6D7E8F9"),
        None,
        "Whether the carve-out covers roadmaps.",
    ),
    (Some("D7E8F9A0"), Some("Heading2"), "Next"),
    (
        Some("E8F9A0B1"),
        None,
        "Read the term against the schedule.",
    ),
    (Some("F9A0B1C2"), None, "Ask about the signature block."),
    (Some("A0B1C2D3"), None, "Nothing else is open."),
    (Some("B1C2D3E4"), Some("Heading2"), "Notes"),
    (
        None,
        None,
        "This paragraph has no paraId, so its identity is its position.",
    ),
    (
        Some("C2D3E4F5"),
        None,
        "That is a different claim from one Word made, and it reads differently.",
    ),
    (Some("D3E4F5A6"), None, "The margin says which it is."),
    (
        Some("E4F5A6B7"),
        None,
        "A reader that cannot say which is guessing.",
    ),
    (Some("F5A6B7C8"), Some("Heading2"), "Filed"),
    (
        Some("A6B7C8D9"),
        None,
        "Against the workspace root, not a database.",
    ),
    (Some("B7C8D9EA"), None, "Nothing was written."),
    (Some("C8D9EAFB"), None, "The end."),
];

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
