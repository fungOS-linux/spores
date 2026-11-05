//! Wasmtime host for the search Spore.
//!
//! Opens a local `fungos-vfs` workspace as `soil:vfs` plus its derived
//! `soil:index`, then calls the Spore's `snapshot(query)` and prints the JSON
//! Canvas snapshot. The workspace root comes from `VFS_ROOT` (defaulting to a
//! sensible local workspace); the Spore never sees the host path itself.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use soil_vfs_host::{VfsHost, index, vfs};
use soil_vfs_host::index::Host as _;
use wasmtime::{Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({ path: "wit", world: "search-spore" });

struct State {
    host: VfsHost,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

// The Spore imports `soil:vfs` and `soil:index`; both are served by the wrapped
// `VfsHost`, which already implements the generated host traits.
impl vfs::Host for State {
    fn ls(&mut self, parent: String, offset: u64, limit: u32) -> Result<vfs::Page, vfs::Error> {
        self.host.ls(parent, offset, limit)
    }
    fn stat(&mut self, path: String) -> Result<vfs::Entry, vfs::Error> {
        self.host.stat(path)
    }
    fn read(&mut self, path: String, offset: u64, length: u32) -> Result<vfs::Chunk, vfs::Error> {
        self.host.read(path, offset, length)
    }
}

impl index::Host for State {
    fn browse(
        &mut self,
        parent: String,
        order: index::Order,
        offset: u64,
        limit: u32,
    ) -> Result<index::Page, index::Error> {
        self.host.browse(parent, order, offset, limit)
    }
    fn search(&mut self, query: index::Query) -> Result<index::Page, index::Error> {
        self.host.search(query)
    }
    fn state(&mut self) -> Result<index::IndexState, index::Error> {
        self.host.state()
    }
    fn refresh(&mut self) -> Result<index::IndexState, index::Error> {
        self.host.refresh()
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let module = args
        .next()
        .or_else(|| std::env::var("SEARCH_SPORE").ok())
        .unwrap_or_else(|| "spores/search/module.wasm".into());
    let query = args
        .next()
        .or_else(|| std::env::var("SEARCH_QUERY").ok())
        .unwrap_or_default();
    let root = std::env::var("VFS_ROOT").unwrap_or_else(|_| "~/.local/share/fungos/vfs".into());
    let root = expand_home(&root);

    let engine = Engine::default();
    let component = wasmtime::component::Component::from_file(&engine, &module)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("load component {module}"))?;
    let mut linker = wasmtime::component::Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync::<State>(&mut linker)
        .map_err(anyhow::Error::from)
        .context("link wasi")?;
    soil_vfs_host::add_to_linker(&mut linker)
        .map_err(anyhow::Error::from)
        .context("link soil:vfs + soil:index")?;

    let mut host =
        VfsHost::open(&root).with_context(|| format!("open vfs root {}", root.display()))?;
    // A bare workspace has never been indexed. Refresh once on startup when the
    // host reports an empty index, then let the Spore query the derived view.
    if host.state().map(|state| state.entries).unwrap_or(0) <= 1 {
        host.refresh().context("refresh VFS index")?;
    }
    let mut store = Store::new(
        &engine,
        State {
            host,
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            table: ResourceTable::new(),
        },
    );
    let bindings = SearchSpore::instantiate(&mut store, &component, &linker)
        .map_err(anyhow::Error::from)
        .context("instantiate search Spore")?;
    let snapshot = bindings
        .call_snapshot(&mut store, &query)
        .map_err(anyhow::Error::from)?
        .map_err(|message| anyhow!("snapshot: {message}"))?;
    println!("{}", String::from_utf8_lossy(&snapshot));
    Ok(())
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        std::env::var_os("HOME")
            .map(|home| Path::new(&home).join(rest))
            .unwrap_or_else(|| Path::new(path).to_path_buf())
    } else {
        Path::new(path).to_path_buf()
    }
}
