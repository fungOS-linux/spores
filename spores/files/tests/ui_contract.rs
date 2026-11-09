//! The Lua files UI is a shipped artifact with its own parse/render gate.

use std::path::PathBuf;
use std::process::Command;

fn spore_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn lua() -> Option<String> {
    tool(&["lua", "lua5.4", "lua5.3", "luajit"], "-v")
}

/// The first of `binaries` that runs. A gate that skips when its interpreter is
/// missing still fails on the machine that has it, which is where the bug would
/// have been caught.
fn tool(binaries: &[&str], flag: &str) -> Option<String> {
    binaries
        .iter()
        .find(|binary| Command::new(binary).arg(flag).output().is_ok())
        .map(|binary| (*binary).to_owned())
}

#[test]
fn ui_uses_no_ambient_authority() {
    let source = std::fs::read_to_string(spore_dir().join("ui/files.lua")).expect("ui/files.lua");

    fn calls(source: &str, needle: &str) -> bool {
        source.match_indices(needle).any(|(index, _)| {
            source[..index]
                .chars()
                .next_back()
                .is_none_or(|before| !(before.is_alphanumeric() || before == '_'))
        })
    }

    // `require("fungos.ui")` is the one call allowed, and it names a declared
    // dependency: the composer inlines the library and hands the view a
    // `require` whose single entry is that file. Any other require reaches for
    // the sandbox, and `os.date` would be a clock the view does not own.
    let stripped = source.replace("require(\"fungos.ui\")", "");
    for needle in [
        "require(",
        "os.",
        "io.",
        "package.",
        "dofile(",
        "loadfile(",
        "load(",
    ] {
        assert!(
            !calls(&stripped, needle),
            "ui/files.lua may not use {needle}: Canvas gives it no ambient authority"
        );
    }
}

#[test]
fn ui_names_roles_not_colours() {
    let source = std::fs::read_to_string(spore_dir().join("ui/files.lua")).expect("ui/files.lua");
    // docs/design-tokens.md: a view names roles, a theme supplies values. A hex
    // literal in a view is a token gap being papered over.
    for (index, _) in source.match_indices('#') {
        let candidate: String = source[index + 1..].chars().take(6).collect();
        if candidate.len() == 6 && candidate.chars().all(|digit| digit.is_ascii_hexdigit()) {
            panic!("ui/files.lua contains a literal colour: #{candidate}");
        }
    }
}

#[test]
fn ui_renders_the_mock_snapshot() {
    let Some(lua) = lua() else {
        eprintln!("skipping: no Lua interpreter on PATH");
        return;
    };
    // The view names its library through `require`, so rendering needs an SDK
    // checkout. CI runs this fixture as its own step with the SDK checked out.
    let Some(sdk) = sdk() else {
        eprintln!("skipping: no fungos-sdk checkout (set FUNGOS_SDK)");
        return;
    };
    let output = Command::new(lua)
        .arg("tests/ui_fixture.lua")
        .current_dir(spore_dir())
        .env("FUNGOS_SDK", sdk)
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

/// The page draws the served scene and turns a press into a request. It shipped
/// in the worklog Spore as a page that read `scene.nodes` off a bare array and
/// drew nothing, which only the person looking at the window noticed.
#[test]
fn page_draws_and_navigates_the_served_scene() {
    let Some(lua) = lua() else {
        eprintln!("skipping: no Lua interpreter on PATH");
        return;
    };
    let Some(node) = tool(&["node"], "--version") else {
        eprintln!("skipping: no node on PATH");
        return;
    };
    let Some(sdk) = sdk() else {
        eprintln!("skipping: no fungos-sdk checkout (set FUNGOS_SDK)");
        return;
    };

    let scene = std::env::temp_dir().join(format!("files-page-{}.json", std::process::id()));
    let render = Command::new(lua)
        .arg("../../tools/shot.lua")
        .arg("ui/files.lua")
        .arg("examples/files_mock.lua")
        .arg("../themes/graphite.lua")
        .arg(&scene)
        .arg("1182")
        .arg("820")
        .current_dir(spore_dir())
        .env("FUNGOS_SDK", &sdk)
        .output()
        .expect("run lua");
    assert!(
        render.status.success(),
        "scene render failed:\n{}\n{}",
        String::from_utf8_lossy(&render.stdout),
        String::from_utf8_lossy(&render.stderr)
    );

    let smoke = Command::new(node)
        .arg("tests/page_smoke.mjs")
        .arg(&scene)
        .current_dir(spore_dir())
        .output()
        .expect("run node");
    let _ = std::fs::remove_file(&scene);
    assert!(
        smoke.status.success(),
        "the page did not draw or route the scene:\n{}\n{}",
        String::from_utf8_lossy(&smoke.stdout),
        String::from_utf8_lossy(&smoke.stderr)
    );
}

/// A fungos-sdk checkout, named by `FUNGOS_SDK` and nothing else: a fallback to
/// `$HOME/src/...` would run against whatever branch happens to be checked out
/// there.
fn sdk() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var("FUNGOS_SDK").ok()?);
    path.join("lua/fungos/ui.lua").exists().then_some(path)
}
