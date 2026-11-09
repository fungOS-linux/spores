//! The wasm component: records in, records out. No host capability beyond
//! `soil:records` is imported, and calculation happens here.
wit_bindgen::generate!({ path: "wit", world: "sheets", generate_all });

use soil::records::records::{self, Entry, Key};

use crate::model::{self, Tab};

const TAB_COLLECTION: &str = "tab";
const CALC_COLLECTION: &str = "calc";
const XLSX_COLLECTION: &str = "xlsx";
const XLSX_SCHEMA: &str = "fungos.sheet/xlsx@1";

struct Component;

export!(Component);

impl Guest for Component {
    fn run(args: Vec<String>) -> Result<(), String> {
        match args.first().map(String::as_str).unwrap_or_default() {
            "calc" => {
                let id = args.get(1).ok_or("calc needs a tab id")?;
                let tab = read_tab(id)?;
                let values = model::evaluate(&tab)?;
                let body = serde_json::to_vec(&model::calc_body(id, &values))
                    .map_err(|error| error.to_string())?;
                write(id, CALC_COLLECTION, model::CALC_SCHEMA, body)?;
                Ok(())
            }
            // The workbook is stored as a record body so the host decides how a
            // user receives a file; the Spore never assumes a filesystem.
            "export" => {
                let id = args.get(1).ok_or("export needs a tab id")?;
                let tab = read_tab(id)?;
                let title = if tab.title.is_empty() { id.clone() } else { tab.title.clone() };
                let bytes = model::to_xlsx(&tab, &title)?;
                write(id, XLSX_COLLECTION, XLSX_SCHEMA, bytes)?;
                Ok(())
            }
            other => Err(format!(
                "unknown command {other:?}; use calc <tab-id> or export <tab-id>"
            )),
        }
    }
}

fn read_tab(id: &str) -> Result<Tab, String> {
    let entry = records::get(&Key {
        collection: TAB_COLLECTION.into(),
        id: id.into(),
    })
    .map_err(|error| format!("records {error:?}"))?;
    Tab::from_body(&entry.body)
}

fn write(id: &str, collection: &str, schema: &str, body: Vec<u8>) -> Result<(), String> {
    records::upsert(&Entry {
        key: Key { collection: collection.into(), id: id.into() },
        schema: schema.into(),
        revision: 0,
        updated_at_ms: 0,
        origin: String::new(),
        deleted: false,
        body,
    })
    .map_err(|error| format!("records {error:?}"))?;
    Ok(())
}
