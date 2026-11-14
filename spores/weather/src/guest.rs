//! The wasm guest: `soil:http`, `soil:records` and `soil:time` in, `run(args)` out.
//!
//! This module is compiled only for `wasm32`, so the command logic in `lib.rs`
//! stays testable on the host without a wasm runtime.

use std::cell::RefCell;

use serde_json::{json, Value};
use soil::http::http as http_abi;
use soil::records::records;
use soil::time::time;
use weather_protocol as weather;

wit_bindgen::generate!({ path: "wit", world: "weather", generate_all });

/// A five-day forecast is a couple of kilobytes; the cap is the host's budget
/// to enforce and this Spore's promise not to ask for more.
const MAX_BODY: u32 = 128 * 1024;
const TIMEOUT_MS: u32 = 15_000;

thread_local! {
    static SNAPSHOT: RefCell<Value> = const { RefCell::new(Value::Null) };
}

struct Component;

export!(Component);

impl Guest for Component {
    fn run(args: Vec<String>) -> Result<(), String> {
        match crate::execute(&args, &mut Capabilities) {
            Ok(outcome) => {
                SNAPSHOT.with(|slot| *slot.borrow_mut() = outcome.snapshot);
                println!("{}", outcome.output);
                Ok(())
            }
            Err(error) => {
                SNAPSHOT.with(|slot| *slot.borrow_mut() = json!({"ok": false, "error": error}));
                Err(error)
            }
        }
    }

    fn snapshot() -> Result<Vec<u8>, String> {
        SNAPSHOT.with(|slot| {
            serde_json::to_vec(&*slot.borrow()).map_err(|error| format!("snapshot JSON: {error}"))
        })
    }
}

struct Capabilities;

impl crate::Host for Capabilities {
    fn get(&mut self, url: &str) -> Result<Vec<u8>, String> {
        let response = http_abi::fetch(&http_abi::Request {
            url: url.to_string(),
            method: http_abi::Method::Get,
            headers: Vec::new(),
            body: None,
            credential: None,
            max_bytes: MAX_BODY,
            timeout_ms: TIMEOUT_MS,
        })
        .map_err(http_error)?;
        if !(200..300).contains(&response.status) {
            // Open-Meteo puts the reason for a 400 in the body; keep it.
            let body = String::from_utf8_lossy(&response.body);
            return Err(format!("http {}: {}", response.status, body.trim()));
        }
        Ok(response.body)
    }

    fn upsert(&mut self, id: &str, body: &[u8]) -> Result<(), String> {
        records::upsert(&records::Entry {
            key: records::Key {
                collection: weather::COLLECTION.to_string(),
                id: id.to_string(),
            },
            schema: weather::SCHEMA.to_string(),
            revision: 0,
            updated_at_ms: 0,
            origin: String::new(),
            deleted: false,
            body: body.to_vec(),
        })
        .map_err(|error| format!("{error:?}"))?;
        Ok(())
    }

    fn monotonic_ms(&mut self) -> Result<u64, String> {
        time::monotonic_ms().map_err(|error| format!("time: {error:?}"))
    }
}

fn http_error(error: http_abi::Error) -> String {
    match error {
        http_abi::Error::Denied => {
            "denied: the host's origin allowlist does not include this URL".to_string()
        }
        other => format!("{other:?}"),
    }
}
