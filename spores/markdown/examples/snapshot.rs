//! Emit a representative snapshot as a Lua table for the preview harness.
//!
//! `cargo run --example snapshot > /tmp/md_snapshot.lua`

use spore_markdown::{Editor, Mode, project, snapshot};

const SOURCE: &str = r#"# Field notes

A *small* document with **one** `code` span, a [link](doc:notes.md),
and an inline ![site map](asset:sha256:00ff).

## What works

- host-rendered diagrams
- images and attachments by target
- hot-reloadable UI

1. Rust owns semantics
2. Lua owns presentation
3. Canvas renders every surface

```rust
fn main() {
    println!("one scene, three hosts");
}
```

```mermaid
graph TD
  A[Spore] --> B[Snapshot]
  B --> C[Lua UI]
  C --> D[Canvas]
```

> The snapshot carries no colours, fonts, or coordinates.

---

Editing above a diagram must not invalidate its host-rendered copy.
"#;

fn main() {
    let mut editor = Editor::new(SOURCE).expect("valid document");
    editor.set_mode(Mode::Edit);
    editor.move_down();
    editor.move_end();
    editor.insert_str(" (live edit)").expect("editable");

    let snapshot = project(&editor);
    print!("return {}", snapshot::to_lua(&snapshot));
}
