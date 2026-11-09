//! A chrome Spore is a package, so these tests go through the package: manifest
//! first, then the Lua it names.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use chrome_host::{ChromeSpore, Host, REGIONS};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn divebar() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../divebar")
}

fn sdk() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FUNGOS_SDK") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join("src/fungOS/fungos-sdk");
    path.join("lua/fungos/ui.lua").is_file().then_some(path)
}

fn host(grants: &BTreeSet<String>) -> Host {
    Host::new(1200.0, 44.0, None, grants).expect("host")
}

/// A package on disk, written from the parts a test cares about.
fn package(name: &str, manifest: &str, lua: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("chrome-host-{}-{name}-{id}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("ui")).unwrap();
    fs::write(dir.join("spore.toml"), manifest).unwrap();
    fs::write(dir.join("ui/bar.lua"), lua).unwrap();
    dir
}

const HEADER: &str = r#"schema = 1

[spore]
id = "dev.fungos.fixture"
name = "fixture"
version = "0.1.0"
license = "MIT"
summary = "test"

[engine]
soil = ">=0.1"
target = "soil"

[ui]
entry = "ui/bar.lua"

[chrome]
regions = ["top-bar"]

[launch]
default = "canvas"
surfaces = ["canvas"]
"#;

#[test]
fn divebar_is_a_package_the_host_can_read() {
    let spore = ChromeSpore::load(&divebar()).expect("divebar loads");
    assert_eq!(spore.id, "dev.fungos.divebar");
    assert_eq!(spore.regions, vec!["top-bar"]);
    assert!(spore.grants.is_empty(), "a projection asks for nothing");
    assert_eq!(
        spore.source_for("top-bar"),
        Some(divebar().join("dist/top-bar.lua").as_path()),
        "an explicit region artifact wins over the authored view"
    );
    assert!(
        spore.source_for("dock").is_none(),
        "an unclaimed region has no source"
    );
}

#[test]
fn the_shipped_package_renders_without_an_sdk() {
    // The point of shipping the composed artifact: no checkout, no `require`,
    // no build step at load. A compositor has none of those.
    let spore = ChromeSpore::load(&divebar()).expect("load");
    let host = Host::new(1200.0, 44.0, None, &spore.grants).expect("host");
    let snapshot = host
        .load_snapshot(Some(&divebar().join("ui/snapshot.lua")))
        .expect("fixture");
    let scene = host.render(&spore, "top-bar", snapshot).expect("render");
    assert_eq!(scene.len(), 17, "the fixture frame draws 17 primitives");
    let ids: Vec<&str> = scene
        .iter()
        .filter_map(|node| node.get("id").and_then(|id| id.as_str()))
        .collect();
    for expected in ["ground", "ws-2", "active-title", "clock"] {
        assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
    }
}

#[test]
fn composing_refuses_a_version_the_sdk_is_not() {
    let Some(sdk) = sdk() else {
        eprintln!("skipping: no fungos-sdk checkout (set FUNGOS_SDK)");
        return;
    };
    let dir = package(
        "wrongversion",
        &HEADER.replace("[launch]", "[deps]\n\"fungos.ui\" = \"9.9.9\"\n\n[launch]"),
        "local ui = require(\"fungos.ui\")\nreturn { render = function() end }\n",
    );
    let spore = ChromeSpore::load(&dir).expect("load");
    let error = chrome_host::compose(&spore, "top-bar", &sdk).expect_err("version mismatch");
    let message = format!("{error:#}");
    assert!(message.contains("9.9.9"), "{message}");
    assert!(
        message.contains("0.1.0"),
        "the error names what the SDK is: {message}"
    );
}

#[test]
fn a_range_is_not_an_artifact_pin() {
    let dir = package(
        "range",
        &HEADER.replace("[launch]", "[deps]\n\"fungos.ui\" = \"^0.1.0\"\n\n[launch]"),
        "return { render = function() end }\n",
    );
    let spore = ChromeSpore::load(&dir).expect("load");
    assert_eq!(
        spore.deps.get("fungos.ui").map(String::as_str),
        Some("^0.1.0")
    );
}

#[test]
fn the_committed_artifact_matches_the_authored_view() {
    let Some(sdk) = sdk() else {
        eprintln!("skipping: no fungos-sdk checkout (set FUNGOS_SDK)");
        return;
    };
    let spore = ChromeSpore::load(&divebar()).expect("load");
    let composed = chrome_host::compose(&spore, "top-bar", &sdk).expect("compose");
    let committed = std::fs::read_to_string(divebar().join("dist/top-bar.lua")).expect("artifact");
    assert_eq!(
        composed, committed,
        "dist/top-bar.lua is stale: run `cargo run -p chrome-host -- compose spores/divebar --sdk <fungos-sdk>`"
    );
}

#[test]
fn the_host_fills_the_clock_when_the_package_ships_no_fixture() {
    let spore = ChromeSpore::load(&divebar()).expect("load");
    let host = Host::new(1200.0, 44.0, None, &spore.grants).expect("host");
    let scene = host
        .render(&spore, "top-bar", host.load_snapshot(None).expect("empty"))
        .expect("render");
    let clock = scene
        .iter()
        .find(|node| node.get("id").and_then(|id| id.as_str()) == Some("clock"))
        .and_then(|node| node.get("text"))
        .and_then(|text| text.as_str())
        .expect("a clock node");
    assert_eq!(clock.len(), 5, "HH:MM, from the platform: {clock}");
    assert_eq!(&clock[2..3], ":");
}

#[test]
fn an_ungranted_call_is_refused_at_the_call_site() {
    let dir = package(
        "ungranted",
        HEADER,
        "local M = {}\nfunction M.render()\n  __soil.invoke(\"soil:time\", \"soil:time/time@1.1.0\", \"wall-clock\", {})\nend\nreturn M\n",
    );
    let spore = ChromeSpore::load(&dir).expect("load");
    let host = host(&spore.grants);
    let error = host
        .render(&spore, "top-bar", host.load_snapshot(None).unwrap())
        .expect_err("no grant, no call");
    let message = format!("{error:#}");
    assert!(message.contains("denied"), "{message}");
    assert!(
        message.contains("soil:time"),
        "the refusal names the grant: {message}"
    );
}

#[test]
fn a_granted_call_reaches_the_host_instead_of_being_denied() {
    let dir = package(
        "granted",
        &HEADER.replace(
            "[launch]",
            "[grants]\n\"soil:time\" = \"the bar shows a clock\"\n\n[launch]",
        ),
        "local M = {}\nfunction M.render()\n  __soil.invoke(\"soil:time\", \"soil:time/time@1.1.0\", \"wall-clock\", {})\nend\nreturn M\n",
    );
    let spore = ChromeSpore::load(&dir).expect("load");
    let host = host(&spore.grants);
    let error = host
        .render(&spore, "top-bar", host.load_snapshot(None).unwrap())
        .expect_err("this host implements no capability");
    let message = format!("{error:#}");
    assert!(message.contains("unimplemented"), "{message}");
    assert!(
        !message.contains("denied"),
        "a granted call must not read as denied: {message}"
    );
}

#[test]
fn a_manifest_without_a_region_is_refused() {
    let dir = package(
        "nochrome",
        &HEADER.replace("[chrome]\nregions = [\"top-bar\"]\n", ""),
        "return { render = function() end }\n",
    );
    let error = ChromeSpore::load(&dir).expect_err("no [chrome]");
    assert!(format!("{error:#}").contains("claims no chrome region"));
}

#[test]
fn an_unknown_region_is_refused() {
    let dir = package(
        "unknown",
        &HEADER.replace("\"top-bar\"", "\"statusbar\""),
        "return { render = function() end }\n",
    );
    let error = ChromeSpore::load(&dir).expect_err("unknown region");
    let message = format!("{error:#}");
    assert!(message.contains("statusbar"), "{message}");
    assert!(
        message.contains(REGIONS[0]),
        "the error lists what a host can draw: {message}"
    );
}

#[test]
fn a_region_naming_a_missing_file_is_refused() {
    let dir = package(
        "missing",
        &HEADER.replace(
            "[chrome]\nregions = [\"top-bar\"]\n",
            "[chrome]\nregions = [\"top-bar\"]\n[chrome.sources]\ntop-bar = \"dist/absent.lua\"\n",
        ),
        "return { render = function() end }\n",
    );
    let error = ChromeSpore::load(&dir).expect_err("missing artifact");
    assert!(format!("{error:#}").contains("dist/absent.lua"));
}
