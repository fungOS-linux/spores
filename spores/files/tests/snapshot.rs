//! The snapshot is the only thing the view is given, so its shape and its
//! escaping are part of the contract, not formatting details.

use std::process::Command;

use files::scan::{Limits, Scan, Totals, scan};
use files::snapshot::{Root, Snapshot, format_date};
use files::vfs::{Entry, Kind, MemoryVfs};

fn loaded() -> (Snapshot, MemoryVfs) {
    let mut vfs = MemoryVfs::new();
    vfs.file_at("readme.md", 12, "text/markdown", 1_700_000_000_000)
        .file_at("photo.png", 4096, "image/png", 0)
        .dir("notes");
    let scan = scan(&mut vfs, "", "", Limits::default());
    let roots = vec![Root {
        id: "studio".to_string(),
        name: "Studio".to_string(),
        totals: scan.totals,
        available: true,
    }];
    (
        Snapshot::new(
            "Studio",
            "studio",
            "",
            roots,
            &scan,
            1_700_000_000_000,
            None,
        ),
        vfs,
    )
}

#[test]
fn the_literal_is_a_lua_table_with_the_documented_fields() {
    let (snapshot, _) = loaded();
    let lua = snapshot.to_lua();
    for expected in [
        "return {",
        "title = \"files\"",
        "workspace = \"Studio\"",
        "root = \"studio\"",
        "crumbs = {",
        "{ name = \"Studio\", path = \"\" }",
        "totals = { bytes = 4108, files = 2, folders = 1",
        "kinds = {",
        "listing = {",
        "listing_total = 3",
        "largest = {",
    ] {
        assert!(lua.contains(expected), "missing {expected:?} in:\n{lua}");
    }
}

#[test]
fn the_literal_loads_in_a_lua_interpreter() {
    let Some(lua) = ["lua", "lua5.4", "lua5.3", "luajit"]
        .into_iter()
        .find(|binary| Command::new(binary).arg("-v").output().is_ok())
    else {
        eprintln!("skipping: no Lua interpreter on PATH");
        return;
    };

    // A name with a quote, a backslash, a newline-ish control byte and a
    // non-ASCII character: a Lua literal that any of these breaks is a literal
    // that silently truncates someone's filename.
    let mut vfs = MemoryVfs::new();
    vfs.file("odd\"name\\here.txt", 1, "text/plain")
        .file("snowman \u{2603}.md", 2, "text/markdown")
        .file("control\u{1}byte.txt", 3, "text/plain");
    let scan = scan(&mut vfs, "", "", Limits::default());
    let roots = vec![Root {
        id: "odd".to_string(),
        name: "Odd".to_string(),
        totals: scan.totals,
        available: true,
    }];
    let snapshot = Snapshot::new("Odd", "odd", "", roots, &scan, 0, None);
    let path = std::env::temp_dir().join(format!("files-literal-{}.lua", std::process::id()));
    std::fs::write(&path, snapshot.to_lua()).unwrap();
    let script = format!(
        "local t = assert(loadfile({:?}))(); assert(t.title == 'files'); \
         assert(#t.listing == 3); assert(t.listing[1].name == 'control\\1byte.txt');",
        path.to_str().unwrap()
    );
    let output = Command::new(lua)
        .arg("-e")
        .arg(script)
        .output()
        .expect("run lua");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "the generated literal did not load:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_fingerprint_changes_with_the_tree_and_not_with_the_scan_clock() {
    let (first, _) = loaded();
    let (second, _) = loaded();
    assert_eq!(first.fingerprint(), second.fingerprint());

    let scan = Scan {
        totals: Totals {
            bytes: 4109,
            files: 2,
            folders: 1,
            ..Totals::default()
        },
        ..Scan::default()
    };
    let changed = Snapshot::new(
        "Studio",
        "studio",
        "",
        first.roots.clone(),
        &scan,
        first.updated_ms,
        None,
    );
    assert_ne!(first.fingerprint(), changed.fingerprint());
}

#[test]
fn a_directory_row_carries_its_item_count_and_a_file_its_date() {
    let directory = Entry {
        path: "notes".into(),
        name: "notes".into(),
        kind: Kind::Directory,
        size: 0,
        media_type: String::new(),
        updated_at_ms: None,
        child_count: Some(3),
    };
    let file = Entry {
        path: "readme.md".into(),
        name: "readme.md".into(),
        kind: Kind::File,
        size: 12,
        media_type: "text/markdown".into(),
        updated_at_ms: Some(1_700_000_000_000),
        child_count: None,
    };
    let scan = Scan {
        listing: vec![directory, file],
        listing_total: 2,
        ..Scan::default()
    };
    let snapshot = Snapshot::new("Studio", "studio", "", Vec::new(), &scan, 0, None);
    let lua = snapshot.to_lua();
    assert!(lua.contains("items = 3"), "{lua}");
    assert!(lua.contains("modified = \"2023-11-14\""), "{lua}");
}

#[test]
fn dates_are_civil_utc() {
    assert_eq!(format_date(0), "1970-01-01");
    assert_eq!(format_date(1_700_000_000_000), "2023-11-14");
}
