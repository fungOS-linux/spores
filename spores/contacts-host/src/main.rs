//! Wasmtime host for the contacts Spore.
//!
//! Opens the local `fungos-records` contacts book as one `soil:records`
//! collection, calls the Spore's `snapshot(query)`, and prints JSON.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use soil_records_host::{RecordsBookHost, records};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({ path: "wit", world: "contacts-spore" });

struct State {
    records: RecordsBookHost,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl records::Host for State {
    fn get(&mut self, key: records::Key) -> Result<records::Entry, records::Error> {
        self.records.get(key)
    }
    fn scan(
        &mut self,
        collection: String,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<records::Page, records::Error> {
        self.records.scan(collection, cursor, limit)
    }
    fn upsert(&mut self, entry: records::Entry) -> Result<records::Entry, records::Error> {
        self.records.upsert(entry)
    }
    fn delete(&mut self, key: records::Key) -> Result<(), records::Error> {
        self.records.delete(key)
    }
    fn changes(
        &mut self,
        since: Option<String>,
        limit: u32,
    ) -> Result<records::Page, records::Error> {
        self.records.changes(since, limit)
    }
    fn apply(
        &mut self,
        entries: Vec<records::Entry>,
    ) -> Result<records::ApplyResult, records::Error> {
        self.records.apply(entries)
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let module = args
        .next()
        .or_else(|| std::env::var("CONTACTS_SPORE").ok())
        .unwrap_or_else(|| "spores/contacts/module.wasm".into());
    let query = args.next().or_else(|| std::env::var("CONTACTS_QUERY").ok()).unwrap_or_default();
    let root = std::env::var("RECORDS_ROOT")
        .unwrap_or_else(|_| "~/.local/share/fungos/records".into());
    let root = expand_home(&root);
    let book = std::env::var("RECORDS_CONTACTS_BOOK")
        .unwrap_or_else(|_| "personal/avery/contacts".into());
    let collection = std::env::var("RECORDS_COLLECTION").unwrap_or_else(|_| "contact".into());

    let engine = Engine::default();
    let component = wasmtime::component::Component::from_file(&engine, &module)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("load component {module}"))?;
    let mut linker = wasmtime::component::Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync::<State>(&mut linker)
        .map_err(anyhow::Error::from)
        .context("link wasi")?;
    soil_records_host::add_to_linker::<State>(&mut linker)
        .map_err(anyhow::Error::from)
        .context("link soil:records")?;

    let records = RecordsBookHost::open_contacts(&root, &book, &collection)?;
    let mut store = Store::new(
        &engine,
        State {
            records,
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            table: ResourceTable::new(),
        },
    );
    let bindings = ContactsSpore::instantiate(&mut store, &component, &linker)
        .map_err(anyhow::Error::from)
        .context("instantiate contacts Spore")?;
    let snapshot = bindings
        .call_snapshot(&mut store, &query)
        .map_err(anyhow::Error::from)?
        .map_err(|message| anyhow!("snapshot: {message}"))?;
    println!("{}", String::from_utf8_lossy(&snapshot));
    Ok(())
}

fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        std::env::var_os("HOME")
            .map(|home| Path::new(&home).join(rest))
            .unwrap_or_else(|| Path::new(path).to_path_buf())
    } else {
        Path::new(path).to_path_buf()
    }
}
