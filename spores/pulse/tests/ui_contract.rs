//! The Lua view and the browser page are shipped artifacts with their own gates.

use std::path::PathBuf;
use std::process::Command;

fn spore_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn tool(binaries: &[&str], flag: &str) -> Option<String> {
    binaries
        .iter()
        .find(|binary| Command::new(binary).arg(flag).output().is_ok())
        .map(|binary| (*binary).to_owned())
}

/// A fungos-sdk checkout, named by `FUNGOS_SDK` and nothing else: a fallback to
/// a path on this machine would test whatever that checkout happened to be.
fn sdk() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var("FUNGOS_SDK").ok()?);
    path.join("lua/fungos/ui.lua").exists().then_some(path)
}

#[test]
fn ui_uses_no_ambient_authority() {
    let source =
        std::fs::read_to_string(spore_dir().join("ui/console.lua")).expect("ui/console.lua");

    fn calls(source: &str, needle: &str) -> bool {
        source.match_indices(needle).any(|(index, _)| {
            source[..index]
                .chars()
                .next_back()
                .is_none_or(|before| !(before.is_alphanumeric() || before == '_'))
        })
    }

    // `require("fungos.ui")` is the one call allowed: the composer inlines the
    // library the view declared. Any other require would be reaching for the
    // sandbox, and a view that can load code is a Spore without a manifest.
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
            "ui/console.lua may not use {needle}: Canvas gives it no ambient authority"
        );
    }
}

#[test]
fn ui_names_roles_not_colours() {
    let source =
        std::fs::read_to_string(spore_dir().join("ui/console.lua")).expect("ui/console.lua");
    // A hex literal in a view is a token gap being papered over.
    for (index, _) in source.match_indices('#') {
        let candidate: String = source[index + 1..].chars().take(6).collect();
        if candidate.len() == 6 && candidate.chars().all(|digit| digit.is_ascii_hexdigit()) {
            panic!("ui/console.lua contains a literal colour: #{candidate}");
        }
    }
}

#[test]
fn ui_fixture_renders() {
    let Some(lua) = tool(&["lua", "lua5.4", "lua5.3", "luajit"], "-v") else {
        eprintln!("skipping: no Lua interpreter on PATH");
        return;
    };
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

/// The page shipped once without ever being executed, so this renders the scene
/// the way `pulse-serve` does and then runs the page's own `<script>` against
/// it. A press has to reach `/apply`; a selection must not.
#[test]
fn page_draws_and_routes_presses() {
    let Some(lua) = tool(&["lua", "lua5.4", "lua5.3", "luajit"], "-v") else {
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

    let scene = std::env::temp_dir().join(format!("pulse-page-{}.json", std::process::id()));
    let render = Command::new(lua)
        .arg("../../tools/shot.lua")
        .arg("ui/console.lua")
        .arg("ui/snapshot.example.lua")
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
        "the page did not draw or route:\n{}\n{}",
        String::from_utf8_lossy(&smoke.stdout),
        String::from_utf8_lossy(&smoke.stderr)
    );
}
