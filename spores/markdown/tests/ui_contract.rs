//! The Lua UI is a shipped artifact with its own review gate.
//!
//! These tests skip cleanly when no Lua toolchain is installed, so the crate
//! still builds on a bare CI image. When Lua is present they prove the file
//! parses, renders a fixture, and cannot reach ambient authority.

use std::path::PathBuf;
use std::process::Command;

fn spore_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn find(binaries: &[&str]) -> Option<(String, Vec<String>)> {
    for binary in binaries {
        let probe = Command::new(binary).arg("-v").output();
        if probe.is_ok() {
            return Some(((*binary).to_owned(), Vec::new()));
        }
    }
    None
}

fn ui_source() -> String {
    std::fs::read_to_string(spore_dir().join("ui/editor.lua")).expect("ui/editor.lua")
}

#[test]
fn ui_uses_no_ambient_authority() {
    let source = ui_source();
    for forbidden in [
        "os.",
        "io.",
        "require",
        "package",
        "dofile",
        "loadfile",
        "load(",
        "debug.",
        "collectgarbage",
        "coroutine",
    ] {
        assert!(
            !source.contains(forbidden),
            "ui/editor.lua may not use {forbidden}: Canvas gives it no ambient authority"
        );
    }
}

#[test]
fn ui_is_what_the_crate_ships() {
    assert_eq!(
        ui_source(),
        spore_markdown::Ui::source(),
        "the crate must embed the reviewed UI file"
    );
    assert_eq!(spore_markdown::ui::ENTRY, "ui/editor.lua");
}

#[test]
fn ui_parses_and_renders_a_fixture() {
    let Some((lua, _)) = find(&["lua", "lua5.4", "lua5.3", "luajit"]) else {
        eprintln!("skipping: no Lua interpreter on PATH");
        return;
    };
    let dir = spore_dir();

    if let Some((luac, _)) = find(&["luac", "luac5.4", "luac5.3"]) {
        let status = Command::new(luac)
            .args(["-p", "ui/editor.lua"])
            .current_dir(&dir)
            .status()
            .expect("run luac");
        assert!(status.success(), "ui/editor.lua failed to compile");
    }

    let output = Command::new(lua)
        .arg("tests/ui_fixture.lua")
        .current_dir(&dir)
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

#[test]
fn changing_the_ui_changes_only_its_digest() {
    let original = spore_markdown::Ui::source();
    let restyled = original.replace("ink = \"primary\"", "ink = \"accent\"");
    assert_ne!(restyled, original, "fixture must actually change");
    assert_ne!(
        spore_markdown::ui::digest(&restyled),
        spore_markdown::Ui::digest()
    );
}
