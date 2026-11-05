//! Wasmtime host for the calendar Spore.
//!
//! Opens one `fungos-records` calendar book as the `event` collection, calls
//! `snapshot(query)`, and prints the JSON Canvas snapshot.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use fungos_records::{RecordEntry, Store};
use wasmtime::{Engine, Store as WasmStore};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({ path: "wit", world: "calendar-spore" });

use soil::records::records::{self, Entry};

const SCHEMA: &str = "fungos.calendar/event";
const COLLECTION: &str = "event";

struct State {
    store: Store,
    book: String,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl State {
    fn entry(&self, record: fungos_records::Record) -> Entry {
        Entry {
            key: records::Key { collection: COLLECTION.into(), id: record.entity_id },
            schema: record.schema_id,
            revision: record.updated_hlc,
            updated_at_ms: record.updated_hlc,
            origin: record.origin,
            deleted: record.deleted,
            body: serde_json::to_vec(&record.fields).unwrap_or_default(),
        }
    }

    fn record(&self, entry: &Entry) -> Result<RecordEntry<'static>, records::Error> {
        if entry.key.collection != COLLECTION || entry.key.id.is_empty() {
            return Err(records::Error::Invalid);
        }
        if entry.schema != SCHEMA || entry.body.is_empty() {
            return Err(records::Error::Invalid);
        }
        let fields: serde_json::Value =
            serde_json::from_slice(&entry.body).map_err(|_| records::Error::Invalid)?;
        let fields: &'static serde_json::Value = Box::leak(Box::new(fields));
        let schema: &'static str = Box::leak(entry.schema.clone().into_boxed_str());
        let origin: &'static str = Box::leak(entry.origin.clone().into_boxed_str());
        Ok(RecordEntry {
            entity_id: Box::leak(entry.key.id.clone().into_boxed_str()),
            schema_id: schema,
            schema_version: 1,
            fields: fields.clone(),
            deleted: entry.deleted,
            revision: entry.revision,
            origin,
        })
    }
}

impl records::Host for State {
    fn get(&mut self, key: records::Key) -> Result<Entry, records::Error> {
        if key.collection != COLLECTION {
            return Err(records::Error::Invalid);
        }
        let record = self.store.get(&self.book, &key.id).map_err(|_| records::Error::NotFound)?;
        Ok(self.entry(record))
    }

    fn scan(
        &mut self,
        collection: String,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<records::Page, records::Error> {
        if collection != COLLECTION || limit == 0 || limit > 512 {
            return Err(records::Error::Invalid);
        }
        let records = self
            .store
            .scan_records(&self.book, cursor.as_deref(), limit as usize)
            .map_err(|_| records::Error::Io)?;
        let next = if records.len() == limit as usize {
            records.last().map(|record| record.entity_id.clone())
        } else {
            None
        };
        Ok(records::Page { entries: records.into_iter().map(|record| self.entry(record)).collect(), next })
    }

    fn upsert(&mut self, entry: Entry) -> Result<Entry, records::Error> {
        let id = entry.key.id.clone();
        if entry.key.collection != COLLECTION || entry.schema != SCHEMA || entry.body.is_empty() {
            return Err(records::Error::Invalid);
        }
        let body: serde_json::Value =
            serde_json::from_slice(&entry.body).map_err(|_| records::Error::Invalid)?;
        self.store
            .append(&self.book, &id, "calendar.put", body)
            .map_err(|_| records::Error::Io)?;
        Ok(self.entry(self.store.get(&self.book, &id).map_err(|_| records::Error::Io)?))
    }

    fn delete(&mut self, key: records::Key) -> Result<(), records::Error> {
        if key.collection != COLLECTION {
            return Err(records::Error::Invalid);
        }
        self.store
            .append(&self.book, &key.id, "calendar.delete", serde_json::Value::Null)
            .map(|_| ())
            .map_err(|_| records::Error::Io)
    }

    fn changes(
        &mut self,
        since: Option<String>,
        limit: u32,
    ) -> Result<records::Page, records::Error> {
        if limit == 0 || limit > 512 {
            return Err(records::Error::Invalid);
        }
        let records = self
            .store
            .scan_changes(&self.book, since.as_deref(), limit as usize)
            .map_err(|_| records::Error::Io)?;
        let next = if records.len() == limit as usize {
            records.last().map(|record| format!("{}:{}", record.updated_hlc, record.entity_id))
        } else {
            None
        };
        Ok(records::Page { entries: records.into_iter().map(|record| self.entry(record)).collect(), next })
    }

    fn apply(&mut self, entries: Vec<Entry>) -> Result<records::ApplyResult, records::Error> {
        let mut applied = 0u32;
        let mut skipped = 0u32;
        for entry in entries {
            let record = self.record(&entry)?;
            match self.store.apply_entry(&self.book, record) {
                Ok(true) => applied += 1,
                Ok(false) => skipped += 1,
                Err(_) => return Err(records::Error::Io),
            }
        }
        Ok(records::ApplyResult { applied, skipped })
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let module = args
        .next()
        .or_else(|| std::env::var("CALENDAR_SPORE").ok())
        .unwrap_or_else(|| "spores/calendar/module.wasm".into());
    let query = args.next().or_else(|| std::env::var("CALENDAR_QUERY").ok()).unwrap_or_default();
    let root = expand_home(
        &std::env::var("RECORDS_ROOT").unwrap_or_else(|_| "~/.local/share/fungos/records".into()),
    );
    let book = std::env::var("RECORDS_CALENDAR_BOOK")
        .unwrap_or_else(|_| "personal/avery/calendar".into());

    let engine = Engine::default();
    let component = wasmtime::component::Component::from_file(&engine, &module)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("load component {module}"))?;
    let mut linker = wasmtime::component::Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync::<State>(&mut linker)
        .map_err(anyhow::Error::from)
        .context("link wasi")?;
    records::add_to_linker::<State, wasmtime::component::HasSelf<State>>(&mut linker, |state| state)
        .map_err(anyhow::Error::from)
        .context("link soil:records")?;

    let store = Store::open(&root, &root.join("device_id")).context("open fungos-records")?;
    let mut store = WasmStore::new(
        &engine,
        State {
            store,
            book,
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            table: ResourceTable::new(),
        },
    );
    let bindings = CalendarSpore::instantiate(&mut store, &component, &linker)
        .map_err(anyhow::Error::from)
        .context("instantiate calendar Spore")?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let local_zone = std::env::var("CALENDAR_TZ")
        .ok()
        .filter(|zone| !zone.trim().is_empty())
        .or_else(|| iana_time_zone::get_timezone().ok())
        .unwrap_or_else(|| "UTC".into());
    let snapshot = bindings
        .call_snapshot(&mut store, &query, now_ms, &local_zone)
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
