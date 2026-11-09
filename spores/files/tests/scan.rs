//! The census is the part a person trusts with their disk; these tests hold it
//! to the numbers, not to the shape of the code.

use std::path::PathBuf;

use files::scan::{KindClass, Limits, scan};
use files::vfs::{Error, MemoryVfs, Page, StdFs, Vfs};

fn tree() -> MemoryVfs {
    let mut vfs = MemoryVfs::new();
    vfs.file_at("a.txt", 100, "text/plain", 1_700_000_000_000)
        .file_at("b.png", 300, "image/png", 0)
        .file_at("notes/c.md", 50, "text/markdown", 0)
        .file_at("notes/deep/d.mov", 5000, "video/quicktime", 0)
        .dir("empty");
    vfs
}

#[test]
fn totals_count_every_entry_once() {
    let mut vfs = tree();
    let result = scan(&mut vfs, "", "", Limits::default());
    assert_eq!(result.totals.files, 4, "four files");
    assert_eq!(result.totals.folders, 3, "notes, notes/deep and empty");
    assert_eq!(result.totals.bytes, 5450);
    assert_eq!(result.totals.scanned, 7);
    assert_eq!(result.totals.errors, 0);
    assert!(!result.totals.truncated);
}

#[test]
fn the_listing_is_directories_then_names() {
    let mut vfs = tree();
    let result = scan(&mut vfs, "", "", Limits::default());
    let names: Vec<&str> = result
        .listing
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, vec!["empty", "notes", "a.txt", "b.png"]);
    assert_eq!(result.listing_total, 4);
    assert!(!result.listing_truncated);

    let inner = scan(&mut vfs, "", "notes", Limits::default());
    let names: Vec<&str> = inner
        .listing
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, vec!["deep", "c.md"]);
    // The totals still describe the whole location, not the open folder.
    assert_eq!(inner.totals.files, 4);
    assert_eq!(inner.totals.bytes, 5450);
}

#[test]
fn the_census_is_by_media_type_not_by_filename() {
    let mut vfs = MemoryVfs::new();
    vfs.file("still.weird", 10, "image/png")
        .file("notes.dat", 20, "text/x-rust")
        .file("movie", 30, "video/mp4")
        .file("song", 40, "audio/flac")
        .file("paper", 50, "application/pdf")
        .file("bundle", 60, "application/zip")
        .file("app", 70, "application/wasm")
        .file("blob", 80, "application/x-unknown");
    let result = scan(&mut vfs, "", "", Limits::default());
    let by_class = |class: KindClass| {
        result
            .kinds
            .iter()
            .find(|stat| stat.class == class)
            .map(|stat| (stat.bytes, stat.files))
    };
    assert_eq!(by_class(KindClass::Images), Some((10, 1)));
    // text/x-rust and application/wasm are both code: 20 + 70 bytes.
    assert_eq!(by_class(KindClass::Code), Some((90, 2)));
    assert_eq!(by_class(KindClass::Video), Some((30, 1)));
    assert_eq!(by_class(KindClass::Audio), Some((40, 1)));
    assert_eq!(by_class(KindClass::Documents), Some((50, 1)));
    assert_eq!(by_class(KindClass::Archives), Some((60, 1)));
    assert_eq!(by_class(KindClass::Other), Some((80, 1)));

    // Byte order, not enum order: the bar the view draws is the same order.
    let sizes: Vec<u64> = result.kinds.iter().map(|stat| stat.bytes).collect();
    let mut sorted = sizes.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(sizes, sorted);
}

#[test]
fn the_largest_files_are_bounded_and_ordered() {
    let mut vfs = MemoryVfs::new();
    for size in [10, 900, 40, 700, 5, 3000, 62] {
        vfs.file(&format!("f{size}"), size, "application/octet-stream");
    }
    let limits = Limits {
        largest: 3,
        ..Limits::default()
    };
    let result = scan(&mut vfs, "", "", limits);
    let names: Vec<&str> = result
        .largest
        .iter()
        .map(|file| file.name.as_str())
        .collect();
    assert_eq!(names, vec!["f3000", "f900", "f700"]);
}

#[test]
fn a_budget_stops_the_walk_and_says_so() {
    let mut vfs = tree();
    let limits = Limits {
        max_entries: 3,
        ..Limits::default()
    };
    let result = scan(&mut vfs, "", "", limits);
    assert_eq!(result.totals.scanned, 3);
    assert!(result.totals.truncated);
    assert!(result.totals.bytes < 5450, "a partial sum is not the whole");
}

#[test]
fn the_tree_exactly_fitting_the_budget_is_not_truncated() {
    let mut vfs = tree();
    let limits = Limits {
        max_entries: 7,
        ..Limits::default()
    };
    let result = scan(&mut vfs, "", "", limits);
    assert_eq!(result.totals.scanned, 7);
    assert!(!result.totals.truncated);
}

#[test]
fn a_depth_limit_counts_the_directory_it_does_not_enter() {
    let mut vfs = tree();
    let limits = Limits {
        max_depth: 1,
        ..Limits::default()
    };
    let result = scan(&mut vfs, "", "", limits);
    // notes is entered; deep is counted but not entered, so d.mov is missing.
    assert!(result.totals.folders >= 3);
    assert!(!result.largest.iter().any(|file| file.name == "d.mov"));
    assert!(result.totals.truncated);
}

struct Unreadable {
    inner: MemoryVfs,
    denied: &'static str,
}

impl Vfs for Unreadable {
    fn ls(&mut self, parent: &str, offset: u64, limit: u32) -> Result<Page, Error> {
        if parent == self.denied {
            Err(Error::Denied)
        } else {
            self.inner.ls(parent, offset, limit)
        }
    }
}

#[test]
fn one_unreadable_directory_does_not_blank_the_census() {
    let mut vfs = Unreadable {
        inner: tree(),
        denied: "notes",
    };
    let result = scan(&mut vfs, "", "", Limits::default());
    assert_eq!(result.totals.errors, 1);
    assert!(
        result
            .first_error
            .as_deref()
            .is_some_and(|error| error.contains("notes"))
    );
    // a.txt, b.png and the empty directory are still counted.
    assert_eq!(result.totals.files, 2);
    assert_eq!(result.totals.bytes, 400);
    assert_eq!(result.totals.folders, 2);
}

#[test]
fn an_opened_directory_is_distinguishable_from_a_missing_one() {
    let mut vfs = tree();
    assert!(scan(&mut vfs, "", "", Limits::default()).listing_seen);
    assert!(scan(&mut vfs, "", "notes", Limits::default()).listing_seen);
    assert!(!scan(&mut vfs, "", "gone", Limits::default()).listing_seen);
    assert!(!scan(&mut vfs, "", "a.txt", Limits::default()).listing_seen);
}

#[test]
fn paging_is_the_hosts_contract_not_an_assumption() {
    let mut vfs = tree();
    let first = vfs.ls("", 0, 2).expect("first page");
    assert_eq!(first.entries.len(), 2);
    assert!(first.next.is_some());
    let offset = first.next.unwrap();
    let second = vfs.ls("", offset, 2).expect("second page");
    assert_eq!(second.entries.len(), 2);
    assert_eq!(second.next, None);
    // The scan over a paged source is the same scan.
    let mut vfs = tree();
    let result = scan(
        &mut vfs,
        "",
        "",
        Limits {
            page: 1,
            ..Limits::default()
        },
    );
    assert_eq!(result.totals.bytes, 5450);
}

#[test]
fn a_directory_larger_than_one_page_is_counted_once_and_terminates() {
    let mut vfs = MemoryVfs::new();
    for index in 0..300 {
        vfs.file(&format!("f{index:03}.txt"), 1, "text/plain");
    }
    let result = scan(
        &mut vfs,
        "",
        "",
        Limits {
            page: 64,
            ..Limits::default()
        },
    );
    assert_eq!(result.totals.files, 300);
    assert_eq!(result.totals.bytes, 300);
    assert_eq!(result.listing_total, 300);
    assert_eq!(result.listing.len(), 200, "the listing keeps its own bound");
    assert!(result.listing_truncated);
}

#[test]
fn the_real_adapter_pages_a_large_directory_and_stops() {
    let base = temp_dir("pages");
    for index in 0..300 {
        std::fs::write(base.join(format!("f{index:03}.txt")), b"x").unwrap();
    }
    let mut vfs = StdFs::new(&base);
    let mut offset = 0u64;
    let mut seen = 0usize;
    let mut pages = 0usize;
    loop {
        let page = vfs.ls("", offset, 128).expect("page");
        seen += page.entries.len();
        pages += 1;
        match page.next {
            Some(next) => {
                assert!(next > offset, "a page that does not advance is the end");
                offset = next;
            }
            None => break,
        }
        assert!(pages < 10, "paging did not terminate");
    }
    assert_eq!(seen, 300);
    assert_eq!(pages, 3);

    // The census over the same adapter sees every file exactly once.
    let result = scan(
        &mut vfs,
        "",
        "",
        Limits {
            page: 128,
            ..Limits::default()
        },
    );
    assert_eq!(result.totals.files, 300);
    assert_eq!(result.totals.bytes, 300);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a_file_is_not_a_directory() {
    let mut vfs = tree();
    assert_eq!(vfs.ls("a.txt", 0, 10).unwrap_err(), Error::NotDirectory);
    assert_eq!(vfs.ls("nope", 0, 10).unwrap_err(), Error::NotFound);
    assert_eq!(vfs.ls("", 0, 0).unwrap_err(), Error::Invalid);
    assert_eq!(vfs.ls("../", 0, 10).unwrap_err(), Error::Invalid);
}

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("spore-files-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

#[test]
fn stdfs_reports_real_sizes_types_and_times() {
    let base = temp_dir("walk");
    std::fs::create_dir_all(base.join("images")).unwrap();
    std::fs::write(base.join("readme.md"), b"# notes\n").unwrap();
    std::fs::write(base.join("images/logo.png"), vec![0u8; 2048]).unwrap();

    let mut vfs = StdFs::new(&base);
    let page = vfs.ls("", 0, 10).expect("list root");
    let names: Vec<&str> = page
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, vec!["images", "readme.md"]);
    let readme = page
        .entries
        .iter()
        .find(|entry| entry.name == "readme.md")
        .expect("readme");
    assert_eq!(readme.size, 8);
    assert_eq!(readme.media_type, "text/markdown");
    assert!(readme.updated_at_ms.is_some());

    let inner = vfs.ls("images", 0, 10).expect("list images");
    assert_eq!(inner.entries[0].size, 2048);
    assert_eq!(inner.entries[0].media_type, "image/png");

    // The boundary is enforced here, not by the caller.
    assert_eq!(vfs.ls("../", 0, 10).unwrap_err(), Error::Invalid);
    assert_eq!(vfs.ls("/etc", 0, 10).unwrap_err(), Error::Invalid);
    assert_eq!(vfs.ls("readme.md", 0, 10).unwrap_err(), Error::NotDirectory);

    std::fs::remove_dir_all(&base).ok();
}

#[cfg(unix)]
#[test]
fn a_symlink_is_an_entry_not_a_door() {
    let base = temp_dir("link");
    let outside = temp_dir("outside");
    std::fs::write(outside.join("secret.txt"), b"do not count me").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.txt"), base.join("link.txt")).unwrap();
    std::fs::write(base.join("real.txt"), b"count me").unwrap();

    let mut vfs = StdFs::new(&base);
    let page = vfs.ls("", 0, 10).expect("list root");
    let names: Vec<&str> = page
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, vec!["link.txt", "real.txt"]);
    let link = page
        .entries
        .iter()
        .find(|entry| entry.name == "link.txt")
        .expect("link");
    // The link's own size, not the target's bytes: a census that follows a link
    // counts the same bytes twice, or leaves the workspace through one.
    assert_eq!(
        link.size,
        outside.join("secret.txt").as_os_str().len() as u64
    );

    std::fs::remove_dir_all(&base).ok();
    std::fs::remove_dir_all(&outside).ok();
}
