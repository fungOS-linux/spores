//! The Lua worklog UI is a shipped artifact with its own parse/render gate.

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
    let source =
        std::fs::read_to_string(spore_dir().join("ui/worklog.lua")).expect("ui/worklog.lua");

    // A name inside another word is not that name: `fungos.ui` is a library, not
    // the `os` library, and a substring check cannot tell them apart.
    fn calls(source: &str, needle: &str) -> bool {
        source.match_indices(needle).any(|(index, _)| {
            source[..index]
                .chars()
                .next_back()
                .is_none_or(|before| !(before.is_alphanumeric() || before == '_'))
        })
    }

    // `require("fungos.ui")` is the one call allowed, and it names a declared
    // dependency: the composer inlines the library and hands the view a `require`
    // whose single entry is that file, so the module Canvas loads makes no call
    // at all. Any *other* require would be reaching for the sandbox.
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
            "ui/worklog.lua may not use {needle}: Canvas gives it no ambient authority"
        );
    }
}

#[test]
fn ui_names_roles_not_colours() {
    let source =
        std::fs::read_to_string(spore_dir().join("ui/worklog.lua")).expect("ui/worklog.lua");
    // docs/design-tokens.md: a view names roles, a theme supplies values. A hex
    // literal in a view is a token gap being papered over.
    //
    // `#` alone is Lua's length operator, so this looks for the shape of a
    // colour (`#` followed by six hex digits) rather than for the character.
    for (index, _) in source.match_indices('#') {
        let candidate: String = source[index + 1..].chars().take(6).collect();
        if candidate.len() == 6 && candidate.chars().all(|digit| digit.is_ascii_hexdigit()) {
            panic!("ui/worklog.lua contains a literal colour: #{candidate}");
        }
    }
}

#[test]
fn ui_renders_the_mock_snapshot() {
    let Some(lua) = lua() else {
        eprintln!("skipping: no Lua interpreter on PATH");
        return;
    };
    // The view names its library through `require`, so rendering it needs an SDK
    // checkout. A machine without one skips here the same way the interpreter
    // check above does; CI runs this fixture as its own step, with the SDK
    // checked out, so the view is not quietly untested.
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

/// A fungos-sdk checkout, named by `FUNGOS_SDK` and nothing else.
///
/// A fallback to `$HOME/src/...` was tempting and is a trap: this machine has an
/// SDK checkout that is checked out on a stale branch, so the fixture would run
/// against a library nobody meant to test and fail here for reasons that are not
/// this Spore's.
fn sdk() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var("FUNGOS_SDK").ok()?);
    path.join("lua/fungos/ui.lua").exists().then_some(path)
}

/// The browser page shipped once without ever being executed. It read
/// `scene.nodes` off a document that was a bare array, drew nothing, and said so
/// only in a status line on someone else's screen. This renders a scene the way
/// the host does and then runs the page's own `<script>` against it.
#[test]
fn page_draws_the_served_scene() {
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

    let scene = std::env::temp_dir().join(format!("worklog-page-{}.json", std::process::id()));
    let render = Command::new(lua)
        .arg("../../tools/shot.lua")
        .arg("ui/worklog.lua")
        .arg("examples/worklog_mock.lua")
        .arg("../themes/graphite.lua")
        .arg(&scene)
        .arg("1182")
        .arg("1569")
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
        "the page did not draw the scene:\n{}\n{}",
        String::from_utf8_lossy(&smoke.stdout),
        String::from_utf8_lossy(&smoke.stderr)
    );
}
