//! A vault worth looking at.
//!
//! The window needs four things to be believable: documents, a document somebody
//! commented on in ways that have since moved, a file the layer does not model,
//! and a session record with a turn that used a tool and thought about it. This
//! builds all four through the same adapters the window uses — no shortcut, and
//! nothing the app itself could not have written.
//!
//! Running it twice leaves the same vault: the records are rebuilt from the files,
//! so the ops, the threads and the session are the same ones. It will not rebuild
//! records it did not write — a demo that silently deleted somebody's history
//! would be the worst kind of convenient.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Present only in a vault this fixture owns.
const MARKER: &str = "demo";

use crate::doclayer::Store;
use crate::vault::document_id;
use anyhow::{Context, Result};

const README: &str = "\
# Future Present Labs

One vault, read and edited as documents.

The left pane lists what is in the vault, grouped by the directory it is in. The
middle pane is one document: markdown source, or the same document rendered. The
right pane is a slim view of an agent working — turns, tool calls, and the thinking
that is collapsed by default.

The arrangement is not fixed. `cmd-shift-p` swaps the document and the transcript,
`cmd-shift-b` moves the vault to the other end, and both are remembered per
document.
";

const TODAY: &str = "\
# Today

The document layer is the model. This file is a view of it: a save is a batch of
ops, and the merge decides what two machines agree on.

A thread on this paragraph is exact, because the paragraph is still the paragraph.

An edit rewrites a block rather than replacing it, which is what keeps a comment
attached to the words it was written about.
";

const SCRATCH: &str = "\
# Scratch

first paragraph, still here

this paragraph is about to be rewritten so a thread over it has to move

last paragraph, untouched
";

// One block and no heading, so deleting it leaves a thread with nothing at all to
// point at — the `orphan` state, which the pane has to be able to show.
const RETIRED: &str = "\
one paragraph, and a thread that outlived it
";

const FUNGOS_VFS: &str = "\
# fungos-vfs at a glance

- **Blocks, not lines.** A document is an ordered set of blocks with identities.
- **One model, many views.** Markdown is a view; a rendered page is another; a
  transcript is a projection of a session record.
- **Anchors, not positions.** A comment points at a block, and is resolved when
  somebody looks.

The weakest link on purpose: a byte range inside a block clamps when the text
shortens, and says so.
";

/// Build (or top up) a demo vault under `out`, and say what is in it.
pub fn build(out: &Path) -> Result<Summary> {
    let out = out.to_path_buf();
    let vault = out.join("vault");
    let records = vault.join(".doc-shell");
    if records.exists() {
        if !records.join(MARKER).exists() {
            anyhow::bail!(
                "{} already holds document-layer records this fixture did not write; \
                 refusing to reset it",
                vault.display()
            );
        }
        fs::remove_dir_all(&records).with_context(|| format!("resetting {}", records.display()))?;
    }
    fs::create_dir_all(vault.join("notes"))?;
    fs::create_dir_all(vault.join("projects"))?;

    write(&vault, "README.md", README)?;
    write(&vault, "notes/today.md", TODAY)?;
    write(&vault, "notes/scratch.md", SCRATCH)?;
    write(&vault, "notes/retired.md", RETIRED)?;
    write(&vault, "projects/fungos-vfs.md", FUNGOS_VFS)?;
    // A Word file, so the pane has a document the layer imports rather than owns.
    // Written as a real container: the reader is the one in the document layer, and
    // a fixture that faked the blocks would prove nothing about it.
    write_docx(&vault, "projects/scope.docx")?;
    // A file the document layer does not model, so the pane can be honest about
    // it instead of drawing a stub.
    write(&vault, "projects/q3-review.pptx", "not really a deck\n")?;

    let store = Store::open(&vault, "studio").context("opening the vault's records")?;
    fs::write(
        records.join(MARKER),
        "written by doc-shell's demo fixture\n",
    )?;

    // 1. Every markdown file becomes a document, the same way the window does it.
    for rel in [
        "README.md",
        "notes/today.md",
        "notes/scratch.md",
        "notes/retired.md",
        "projects/fungos-vfs.md",
    ] {
        let id = document_id(rel);
        let path = vault.join(rel);
        let mut file = store.open_document(&id, &path)?;
        if rel != "notes/scratch.md" && rel != "notes/retired.md" {
            store.reconcile(&mut file)?;
        }
    }

    // 2. Annotations that resolve three different ways.
    annotate(&store, &vault)?;

    // 3. A session record with a tool call, a failure and a thought.
    let sessions = import_session(&out, &store)?;

    Ok(Summary {
        vault,
        records: store.state_dir().join("vfs"),
        sessions,
    })
}

/// Where the demo landed.
#[derive(Clone, Debug)]
pub struct Summary {
    pub vault: PathBuf,
    pub records: PathBuf,
    pub sessions: usize,
}

/// Threads that show each resolution state the layer can report.
fn annotate(store: &Store, vault: &std::path::Path) -> Result<()> {
    // `exact`: the paragraph the thread is on is still there.
    store.annotate(
        &document_id("notes/today.md"),
        2,
        None,
        "Exact: the block this hangs on is still the block this hangs on.",
    )?;
    // `clamped`: a range recorded when the block was longer than it is now.
    store.annotate(
        &document_id("notes/today.md"),
        3,
        Some((0, 4_000)),
        "Clamped: this was a range in a longer paragraph. The words moved and the \
         thread says so rather than pointing at the wrong ones.",
    )?;
    // `neighbour`: annotate a paragraph, then rewrite the file without it. The
    // rewrite is a delete, the delete leaves a tombstone, and the thread resolves
    // to the block that took its place.
    let scratch = document_id("notes/scratch.md");
    let path = vault.join("notes/scratch.md");
    let mut file = store.open_document(&scratch, &path)?;
    store.reconcile(&mut file)?;
    store.annotate(
        &scratch,
        2,
        None,
        "Neighbour: the paragraph this was written about was deleted afterwards.",
    )?;
    fs::write(
        &path,
        "# Scratch\n\nfirst paragraph, still here\n\nlast paragraph, untouched\n",
    )?;
    let mut reopened = store.open_document(&scratch, &path)?;
    store.reconcile(&mut reopened)?;

    // `orphan`: a document with one block, whose block is deleted.
    let retired = document_id("notes/retired.md");
    let retired_path = vault.join("notes/retired.md");
    let mut file = store.open_document(&retired, &retired_path)?;
    store.reconcile(&mut file)?;
    store.annotate(
        &retired,
        0,
        None,
        "Orphan: the only block in this document went away, so there is nothing \
         left to point at.",
    )?;
    fs::write(&retired_path, "")?;
    let mut reopened = store.open_document(&retired, &retired_path)?;
    store.reconcile(&mut reopened)?;
    Ok(())
}

/// One Pi session, in Pi's own shape, imported the way a real one would be.
fn import_session(out: &std::path::Path, store: &Store) -> Result<usize> {
    let store_dir = out.join("pi-store/--work-vault--");
    fs::create_dir_all(&store_dir)?;
    let lines = [
        r#"{"type":"session","version":3,"id":"demo-1","timestamp":"2026-10-21T12:12:14.980Z","cwd":"/work/vault"}"#,
        r#"{"type":"session_info","id":"e1","parentId":null,"timestamp":"2026-10-21T12:12:14.981Z","name":"document layer smoke test"}"#,
        r#"{"type":"model_change","id":"e2","parentId":"e1","timestamp":"2026-10-21T12:12:15.000Z","provider":"bifrost","modelId":"fpl/coder"}"#,
        r#"{"type":"message","id":"e3","parentId":"e2","timestamp":"2026-10-21T12:12:15.100Z","message":{"role":"system","content":"","sections":{"preamble":"You are pi.","rules":"<rules>be brief</rules>","tools":"<tools>(none)</tools>"}}}"#,
        r#"{"type":"message","id":"e4","parentId":"e3","timestamp":"2026-10-21T12:12:16.000Z","message":{"role":"user","content":[{"type":"text","text":"reconcile notes/today.md and tell me what changed"}]}}"#,
        r#"{"type":"message","id":"e5","parentId":"e4","timestamp":"2026-10-21T12:12:17.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Read the file first, then reconcile; reconciling without the file on disk would be reading a document nobody wrote."},{"type":"toolCall","id":"call-1","name":"read","arguments":{"path":"notes/today.md"}}],"model":"fpl/coder","usage":{"input":10,"output":20,"reasoning":5}}}"#,
        r##"{"type":"message","id":"e6","parentId":"e5","timestamp":"2026-10-21T12:12:18.000Z","message":{"role":"toolResult","toolCallId":"call-1","toolName":"read","content":[{"type":"text","text":"# Today\n\nThe document layer is the model..."}]}}"##,
        r#"{"type":"message","id":"e7","parentId":"e6","timestamp":"2026-10-21T12:12:19.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"One block changed: the merge turned the edit into two updates and an insert."},{"type":"toolCall","id":"call-2","name":"bash","arguments":{"command":"fungos-vfs doc sync --id notes-today.md --path notes/today.md"}}],"model":"fpl/coder","usage":{"input":14,"output":33,"reasoning":9}}}"#,
        r#"{"type":"message","id":"e8","parentId":"e7","timestamp":"2026-10-21T12:12:20.000Z","message":{"role":"toolResult","toolCallId":"call-2","toolName":"bash","content":[{"type":"text","text":"notes/today.md\t2 ops, file unchanged"}],"isError":false}}"#,
        r#"{"type":"message","id":"e9","parentId":"e8","timestamp":"2026-10-21T12:12:21.000Z","message":{"role":"assistant","content":[{"type":"text","text":"Two blocks changed and the file already said it, so the merge had nothing to write back. The thread on the middle paragraph is still exact."}],"model":"fpl/coder","usage":{"input":18,"output":41,"reasoning":0}}}"#,
    ];
    fs::write(
        store_dir.join("2026-10-21T12-12-14_demo-1.jsonl"),
        lines.join("\n") + "\n",
    )?;
    store.import_pi(&out.join("pi-store"))
}

fn write(vault: &std::path::Path, rel: &str, text: &str) -> Result<()> {
    let path = vault.join(rel);
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// A small but real `.docx`: the parts Word writes, paragraphs with `w14:paraId`,
/// a heading style, and a table the reader will count rather than model.
fn write_docx(vault: &std::path::Path, rel: &str) -> Result<()> {
    use std::io::Write as _;

    const HEAD: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#,
        r#" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"><w:body>"#
    );
    let paragraph = |id: &str, style: Option<&str>, text: &str| {
        let style = style.map_or_else(String::new, |style| {
            format!(r#"<w:pPr><w:pStyle w:val="{style}"/></w:pPr>"#)
        });
        format!(
            r#"<w:p w14:paraId="{id}">{style}<w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#
        )
    };
    let body = format!(
        "{}{}{}{}{}",
        paragraph("0A1B2C3D", Some("Heading1"), "Scope"),
        paragraph(
            "1A2B3C4E",
            None,
            "What this vault is for, and what it is not."
        ),
        paragraph(
            "2A2B3C4F",
            None,
            "A paragraph Word numbered, so a thread on it survives an edit."
        ),
        r#"<w:tbl><w:tr><w:tc><w:p w14:paraId="3A2B3C40"><w:r><w:t>a table the reader counts</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        paragraph("4A2B3C41", None, "Closing paragraph."),
    );
    let document = format!("{HEAD}{body}</w:body></w:document>");

    let path = vault.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(&path).with_context(|| format!("writing {}", path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in [
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", document.as_str()),
        ("word/styles.xml", "<styles/>"),
    ] {
        zip.start_file(name, options)
            .with_context(|| format!("adding {name}"))?;
        zip.write_all(content.as_bytes())?;
    }
    zip.finish().with_context(|| "finishing the docx")?;
    Ok(())
}
