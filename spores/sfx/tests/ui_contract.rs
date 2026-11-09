//! The Lua UI is a shipped artifact with its own review gate.
//!
//! These skip cleanly when no Lua toolchain is installed, so the crate still
//! builds on a bare CI image. When Lua is present they prove the view parses,
//! paints the board, and cannot reach ambient authority.

use std::path::PathBuf;
use std::process::Command;

fn spore_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ui_source() -> String {
    std::fs::read_to_string(spore_dir().join("ui/board.lua")).expect("ui/board.lua")
}

fn lua() -> Option<String> {
    for binary in ["lua", "lua5.4", "lua5.3", "luajit"] {
        if Command::new(binary).arg("-v").output().is_ok() {
            return Some(binary.to_owned());
        }
    }
    None
}

#[test]
fn ui_uses_no_ambient_authority() {
    let source = ui_source();
    // `require` is the one import the view needs: composition resolves
    // `fungos.ui` into the module before Canvas runs it. Everything else is a
    // way out of the sandbox the view must not have. Match concrete globals,
    // not the bare prefixes: `require("fungos.ui")` contains `os.`.
    for forbidden in [
        "os.exit",
        "os.execute",
        "os.getenv",
        "os.remove",
        "os.rename",
        "os.time",
        "io.open",
        "io.read",
        "io.write",
        "io.lines",
        "io.popen",
        "dofile",
        "loadfile",
        "load(",
        "debug.",
        "collectgarbage",
        "coroutine",
    ] {
        assert!(
            !source.contains(forbidden),
            "ui/board.lua may not use {forbidden}: Canvas gives it no ambient authority"
        );
    }
}

#[test]
fn the_board_renders_a_fixture() {
    let Some(lua) = lua() else {
        eprintln!("skipping: no Lua interpreter on PATH");
        return;
    };
    let output = Command::new(lua)
        .arg("tests/ui_fixture.lua")
        .current_dir(spore_dir())
        .output()
        .expect("run lua");
    assert!(
        output.status.success(),
        "ui fixture failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("ui: ok"));
}
