//! Document semantics and the Lua-facing snapshot. Presentation lives in
//! `ui/editor.lua` and is tested by `ui_contract.rs`.

use spore_markdown::{DocId, Editor, MemoryVfs, Mode, Target, parse_inline, project, snapshot};

fn document() -> &'static str {
    "# Field notes\n\nA *small* document with **one** `code` span.\n\n- first\n- second\n\n1. ordered\n2. items\n\n```mermaid\ngraph TD\n  A --> B\n```\n\n```rust\nlet n = 1;\n```\n\n![site map](asset:sha256:00ff)\n\n> quoted\n\n---\n"
}

fn session() -> Editor {
    let mut editor = Editor::new(document()).expect("valid document");
    editor.set_mode(Mode::Edit);
    for c in "opening line\n".chars() {
        editor.insert_char(c).expect("editable");
    }
    editor.move_down();
    editor.move_end();
    editor.insert_str(" (edited)").expect("editable");
    editor
}

#[test]
fn snapshot_carries_semantics_and_no_presentation() {
    let snapshot = project(&session());

    assert_eq!(snapshot.mode, Mode::Edit);
    assert!(snapshot.dirty);
    let heading = snapshot
        .blocks
        .iter()
        .find(|block| block.kind == "heading")
        .expect("heading");
    assert_eq!(heading.level, 1);
    assert_eq!(heading.text, "Field notes");

    let paragraph = snapshot
        .blocks
        .iter()
        .find(|block| block.kind == "paragraph" && block.text.contains("small"))
        .expect("paragraph");
    assert_eq!(
        paragraph
            .spans
            .iter()
            .map(|span| span.style)
            .collect::<Vec<_>>(),
        vec!["text", "emphasis", "text", "strong", "text", "code", "text"]
    );

    // No colours, fonts, coordinates, or widget names anywhere in the payload.
    let rendered = format!("{snapshot:?}").to_lowercase();
    for forbidden in [
        "ink",
        "colour",
        "color",
        "transform",
        "font",
        "pixel",
        "rgb",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "snapshot leaked presentation: {forbidden}"
        );
    }
}

#[test]
fn diagrams_are_not_code_and_images_are_assets() {
    let snapshot = project(&session());

    let diagram = snapshot
        .blocks
        .iter()
        .find(|block| block.kind == "diagram")
        .expect("mermaid fence is a diagram");
    assert_eq!(diagram.language.as_deref(), Some("mermaid"));
    assert!(diagram.text.contains("A --> B"));

    assert!(
        snapshot
            .blocks
            .iter()
            .any(|block| block.kind == "code" && block.language.as_deref() == Some("rust"))
    );

    let image = snapshot
        .blocks
        .iter()
        .flat_map(|block| block.spans.iter())
        .find(|span| span.style == "image")
        .expect("image span");
    assert_eq!(image.text, "site map");
    assert_eq!(image.target.as_deref(), Some("asset:sha256:00ff"));

    assert_eq!(snapshot.assets.len(), 2);
    assert!(
        snapshot
            .assets
            .iter()
            .any(|request| matches!(request.target, Target::Asset { .. }))
    );
}

#[test]
fn list_items_are_blocks_not_a_rendered_string() {
    let snapshot = project(&session());

    let ordered: Vec<_> = snapshot
        .blocks
        .iter()
        .filter(|block| block.kind == "list_item" && block.ordered)
        .collect();
    assert_eq!(ordered.len(), 2);
    assert_eq!(ordered[0].item, 0);
    assert_eq!(ordered[1].item, 1);

    let unordered = snapshot
        .blocks
        .iter()
        .filter(|block| block.kind == "list_item" && !block.ordered)
        .count();
    assert_eq!(unordered, 2);
}

#[test]
fn the_vfs_boundary_is_the_real_one() {
    let mut vfs = MemoryVfs::new();
    let id = DocId::parse("notes.md").expect("valid id");

    let mut editor = session();
    editor.save(&mut vfs, &id).expect("stored");
    assert!(!editor.is_dirty());

    let reloaded = Editor::open(&mut vfs, &id).expect("loaded");
    assert_eq!(reloaded.source(), editor.source());
    assert_eq!(reloaded.revision(), 0, "a reload starts a new revision");

    let missing = DocId::parse("absent.md").expect("valid id");
    assert!(Editor::open(&mut vfs, &missing).is_err());
}

#[test]
fn identifiers_refuse_paths_and_unfetchable_targets() {
    for raw in ["../etc/passwd", "a/b", "a\\b", "", "x:y", "a..b"] {
        assert!(DocId::parse(raw).is_err(), "{raw} must be refused");
    }

    assert!(Target::parse("../../etc/passwd").is_err());
    assert!(Target::parse("https//nope").is_err());
    assert_eq!(
        Target::parse("doc:notes.md").expect("document"),
        Target::Document {
            id: "notes.md".to_owned()
        }
    );
    let external = Target::parse("https://example.com").expect("external");
    assert_eq!(
        external,
        Target::External {
            scheme: "https".to_owned(),
            reference: "//example.com".to_owned()
        }
    );
    assert!(!external.spore_may_fetch());
}

#[test]
fn editing_respects_utf8_boundaries() {
    let mut editor = Editor::new("héllo 日本語").expect("valid");
    editor.set_mode(Mode::Edit);
    editor.move_end();
    editor.backspace().expect("editable");
    assert_eq!(editor.source(), "héllo 日本");

    editor.move_home();
    editor.move_right();
    editor.insert_char('x').expect("editable");
    assert_eq!(editor.source(), "hxéllo 日本");
    assert_eq!(editor.cursor().column, 2);
}

#[test]
fn lists_continue_and_undo_is_bounded() {
    let mut editor = Editor::new("1. one").expect("valid");
    editor.set_mode(Mode::Edit);
    editor.move_end();
    editor.newline().expect("editable");
    editor.insert_str("two").expect("editable");
    assert_eq!(editor.source(), "1. one\n2. two");

    for _ in 0..(spore_markdown::MAX_UNDO + 8) {
        editor.insert_char('x').expect("editable");
    }
    let mut undos = 0;
    while editor.undo() {
        undos += 1;
    }
    assert!(undos <= spore_markdown::MAX_UNDO);
}

#[test]
fn read_mode_refuses_edits() {
    let mut editor = Editor::new("text").expect("valid");
    assert_eq!(
        editor.insert_char('x'),
        Err(spore_markdown::Error::ReadOnly)
    );
    assert_eq!(editor.backspace(), Err(spore_markdown::Error::ReadOnly));
}

#[test]
fn inline_links_and_images_are_typed() {
    let spans = parse_inline("see [notes](doc:notes.md) and ![alt](attach:figure)");
    assert!(spans.iter().any(|span| matches!(
        span,
        spore_markdown::Inline::Link {
            target: Target::Document { .. },
            ..
        }
    )));
    assert!(spans.iter().any(|span| matches!(
        span,
        spore_markdown::Inline::Image {
            target: Target::Attachment { .. },
            ..
        }
    )));
}

#[test]
fn the_ui_is_a_separate_artifact_identity() {
    assert_eq!(spore_markdown::ui::ENTRY, "ui/editor.lua");
    assert_eq!(spore_markdown::Ui::digest().len(), 16);
    assert_eq!(snapshot::project(&session()).revision, session().revision());
    assert!(!spore_markdown::Ui::source().contains("require"));
}
