//! What the demo vault is supposed to be true of, checked against the layer.
//!
//! These are the claims the panes make: a thread reports one of four resolution
//! states, a transcript has a turn with a tool call and a thought, and a document
//! is the markdown file it is bound to.

use doc_shell::{
    demo,
    doclayer::Store,
    transcript::{TranscriptSource, VfsTranscripts},
    vault::document_id,
};

fn fixture() -> (tempfile::TempDir, demo::Summary, Store) {
    let temp = tempfile::tempdir().unwrap();
    let summary = demo::build(temp.path()).unwrap();
    let store = Store::open(&summary.vault, "studio").unwrap();
    (temp, summary, store)
}

#[test]
fn the_vault_shows_every_resolution_state_the_layer_can_report() {
    let (_temp, _summary, store) = fixture();
    let mut states = Vec::new();
    for rel in ["notes/today.md", "notes/scratch.md", "notes/retired.md"] {
        let id = document_id(rel);
        let doc = store.doc(&id).unwrap();
        for thread in store.threads(&id, &doc).unwrap() {
            states.push(thread.state);
        }
    }
    for expected in ["exact", "clamped", "neighbour", "orphan"] {
        assert!(
            states.contains(&expected),
            "the demo vault has no {expected} thread: {states:?}"
        );
    }
}

#[test]
fn a_thread_that_moved_says_so_and_keeps_the_text_it_was_about() {
    let (_temp, _summary, store) = fixture();
    let id = document_id("notes/scratch.md");
    let doc = store.doc(&id).unwrap();
    let threads = store.threads(&id, &doc).unwrap();
    let moved = threads
        .iter()
        .find(|thread| thread.state == "neighbour")
        .expect("the scratch document has a thread whose block was deleted");
    assert!(
        moved.block.contains("about to be rewritten"),
        "a reader has to see what went away: {:?}",
        moved.block
    );
    assert!(moved.tombstoned);
    assert_eq!(moved.comments.len(), 1);
}

#[test]
fn the_transcript_carries_a_tool_call_its_result_and_a_thought() {
    let (_temp, _summary, store) = fixture();
    let source = VfsTranscripts::new(store);
    let choices = source.sessions();
    assert_eq!(choices.len(), 1);
    let view = source.transcript(&choices[0]).unwrap();
    assert_eq!(view.title, "document layer smoke test");

    let tools: Vec<&str> = view
        .turns
        .iter()
        .flat_map(|turn| turn.tools.iter().map(|tool| tool.name.as_str()))
        .collect();
    assert_eq!(tools, vec!["read", "bash"]);

    let tool_turn = view
        .turns
        .iter()
        .find(|turn| !turn.tools.is_empty())
        .unwrap();
    assert_eq!(tool_turn.tools[0].arguments, r#"{"path":"notes/today.md"}"#);
    assert!(
        tool_turn
            .thinking
            .as_deref()
            .is_some_and(|thought| !thought.is_empty())
    );
    assert!(
        view.turns
            .iter()
            .any(|turn| turn.role == "tool" && turn.text.contains("2 ops")),
        "the tool result is text on its own turn"
    );
}

#[test]
fn a_document_is_the_markdown_file_it_is_bound_to() {
    let (_temp, summary, store) = fixture();
    let id = document_id("notes/today.md");
    let path = summary.vault.join("notes/today.md");
    let mut file = store.open_document(&id, &path).unwrap();
    let first = store.reconcile(&mut file).unwrap();
    assert_eq!(first.ops, 0, "the file and the document already agree");
    assert!(!first.wrote_file);
    assert!(file.render().contains("The document layer is the model"));
}

#[test]
fn rebuilding_the_demo_leaves_the_same_vault() {
    let temp = tempfile::tempdir().unwrap();
    let first = demo::build(temp.path()).unwrap();
    let id = document_id("notes/today.md");
    let store = Store::open(&first.vault, "studio").unwrap();
    let doc = store.doc(&id).unwrap();
    let threads_before = store.threads(&id, &doc).unwrap();
    let blocks_before = doc.len();

    let second = demo::build(temp.path()).unwrap();
    let store = Store::open(&second.vault, "studio").unwrap();
    let doc = store.doc(&id).unwrap();
    assert_eq!(doc.len(), blocks_before);
    assert_eq!(store.threads(&id, &doc).unwrap(), threads_before);
    assert_eq!(second.sessions, 1);
}

#[test]
fn the_fixture_refuses_to_reset_records_it_did_not_write() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(vault.join(".doc-shell")).unwrap();
    let error = demo::build(temp.path()).unwrap_err().to_string();
    assert!(error.contains("refusing to reset"), "{error}");
}
