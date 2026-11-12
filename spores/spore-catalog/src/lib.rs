//! Installed-Spore catalogue: discovery, validation, and the host-owned launch
//! decision.
//!
//! This is the piece that used to live in soil as `crates/soil-spore-host`,
//! pinned by both hosts at `4d4271b` — a commit that is not on soil's `main`,
//! from before `Manifest` grew `[[surfaces]]`. Building against that copy is why
//! `launcher-host` and `spore-browser-host` could not see a declared surface,
//! and why bumping the vendored `soil:spore` contract under them did not
//! compile.
//!
//! Discovery is host-side work and always was: a Spore is installed by a host
//! and does not know where. It belongs with the hosts, and it depends on soil's
//! manifest rather than restating `spore.toml` — one parser, one answer.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
pub use soil_spore::{LaunchError, LaunchTarget, Manifest, Surface, SurfaceKind};

/// One installed Spore, with the manifest that was parsed to find it.
#[derive(Debug, Clone)]
pub struct SporeEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub summary: String,
    /// The Canvas workspace this Spore is opened in.
    ///
    /// Derived, not configured: a development Spore draws in `<name>-dev`, and
    /// a manifest has no say in it.
    pub workspace: Option<String>,
    /// Where the package is installed.
    pub root: PathBuf,
    pub manifest: Manifest,
}

/// A declared surface, flattened for a host's own generated binding.
///
/// The hosts bind `soil:spore` themselves, and those types are local to each
/// host crate. This is the plain-data seam between the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredSurface {
    pub name: String,
    pub kind: SurfaceKind,
    /// The artifact it opens, relative to the package root. Resolved by soil
    /// (kind convention, then the package's `[ui] entry`) so no host invents
    /// its own default.
    pub entry: String,
    pub requires: Option<String>,
}

impl SporeEntry {
    /// Resolve one requested surface against the kinds this host can present.
    ///
    /// The decision is soil's: a host that re-decided "does this Spore have
    /// such a surface, and can I draw it" would be a second answer to a
    /// question the manifest already answers.
    pub fn resolve<'a>(
        &'a self,
        surface: &'a str,
        host: &[SurfaceKind],
    ) -> std::result::Result<LaunchTarget<'a>, LaunchError> {
        self.manifest.resolve_launch(surface, host)
    }

    /// Every declared surface, resolved for presentation to a guest.
    #[must_use]
    pub fn surfaces(&self) -> Vec<DeclaredSurface> {
        self.manifest
            .surfaces
            .iter()
            .map(|surface| DeclaredSurface {
                name: surface.name.clone(),
                kind: surface.kind,
                entry: surface
                    .entry(self.manifest.ui.as_ref())
                    .unwrap_or_default()
                    .to_owned(),
                requires: surface.requires.clone(),
            })
            .collect()
    }
}

/// What a host does when a guest asks it to open one surface.
///
pub trait Launcher {
    /// Open one resolved surface and return the host target name, which is what
    /// `launched.target` echoes back to the guest.
    fn launch(&mut self, entry: &SporeEntry, target: &LaunchTarget<'_>) -> Result<String, String>;
}

/// Every Spore installed under one root.
#[derive(Debug, Clone)]
pub struct SporeCatalog {
    root: PathBuf,
    entries: Vec<SporeEntry>,
}

impl SporeCatalog {
    /// Read every `<root>/<name>/spore.toml`.
    ///
    /// A package that does not parse is an error rather than a skipped
    /// directory: a host that silently drops it shows a shorter catalogue and
    /// nothing says why.
    pub fn scan(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let mut entries = Vec::new();
        for directory in std::fs::read_dir(&root)
            .with_context(|| format!("read Spore root {}", root.display()))?
        {
            let directory = directory?;
            if !directory.file_type()?.is_dir() {
                continue;
            }
            let package = directory.path();
            if !package.join("spore.toml").is_file() {
                continue;
            }
            let manifest = Manifest::load(&package)
                .with_context(|| format!("load Spore {}", package.display()))?;
            entries.push(SporeEntry {
                id: manifest.spore.id.clone(),
                name: manifest.spore.name.clone(),
                version: manifest.spore.version.clone(),
                summary: manifest.spore.summary.clone(),
                workspace: Some(format!("{}-dev", manifest.spore.name)),
                root: package,
                manifest,
            });
        }
        entries.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        Ok(Self { root, entries })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn entries(&self) -> &[SporeEntry] {
        &self.entries
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SporeEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(root: &Path, name: &str, launch: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        std::fs::create_dir_all(dir.join("web")).unwrap();
        std::fs::write(dir.join("ui/app.lua"), "").unwrap();
        std::fs::write(dir.join("web/index.html"), "").unwrap();
        std::fs::write(
            dir.join("spore.toml"),
            format!(
                r#"schema = 1
[spore]
id = "dev.fungos.{name}"
name = "{name}"
version = "0.1.0"
license = "MIT"
summary = "test"
[engine]
soil = ">=0.1"
target = "wasm32-wasip2"
[ui]
entry = "ui/app.lua"
{launch}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn scan_finds_packages_and_leaves_others_alone() {
        let root = tempfile::tempdir().unwrap();
        package(
            root.path(),
            "alpha",
            "[launch]\ndefault = \"canvas\"\nsurfaces = [\"canvas\"]\n",
        );
        std::fs::create_dir_all(root.path().join("not-a-package")).unwrap();
        let catalog = SporeCatalog::scan(root.path()).unwrap();
        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].workspace.as_deref(), Some("alpha-dev"));
    }

    #[test]
    fn declared_surfaces_resolve_their_entry() {
        let root = tempfile::tempdir().unwrap();
        package(
            root.path(),
            "beta",
            "[launch]\ndefault = \"workbook\"\nsurfaces = [\"canvas\", \"web\"]\n\n[[surfaces]]\nname = \"workbook\"\nkind = \"web\"\n\n[[surfaces]]\nname = \"preview\"\nkind = \"canvas\"\nentry = \"ui/app.lua\"\n",
        );
        let catalog = SporeCatalog::scan(root.path()).unwrap();
        let entry = &catalog.entries()[0];
        let surfaces = entry.surfaces();
        assert_eq!(surfaces[0].name, "workbook");
        assert_eq!(surfaces[0].kind, SurfaceKind::Web);
        assert_eq!(surfaces[0].entry, "web/index.html");
        assert_eq!(surfaces[1].entry, "ui/app.lua");
        assert_eq!(
            entry.resolve("workbook", &SurfaceKind::ALL).unwrap().entry,
            Some("web/index.html")
        );
    }

    #[test]
    fn a_host_without_a_browser_refuses_a_web_surface() {
        let root = tempfile::tempdir().unwrap();
        package(
            root.path(),
            "gamma",
            "[launch]\ndefault = \"workbook\"\nsurfaces = [\"web\"]\n\n[[surfaces]]\nname = \"workbook\"\nkind = \"web\"\n",
        );
        let catalog = SporeCatalog::scan(root.path()).unwrap();
        let error = catalog.entries()[0]
            .resolve("workbook", &[SurfaceKind::Canvas])
            .unwrap_err();
        assert_eq!(error.code(), "unsupported");
        let error = catalog.entries()[0]
            .resolve("nope", &SurfaceKind::ALL)
            .unwrap_err();
        assert_eq!(error.code(), "not-found");
    }
}
