//! Workbook model shared by the Spore component and its native tests.
//!
//! A tab is a sparse map of cell address to user input (a number, text, or a
//! formula starting with `=`). Calculation is IronCalc's: the Spore never
//! hand-rolls arithmetic, and the host never sees a formula engine.
use std::collections::BTreeMap;
use std::io::Cursor;

use ironcalc::base::Model;
use ironcalc::base::cell::CellValue;
use ironcalc::export::save_xlsx_to_writer;

/// Schema strings carry their version; the host stores schema_version as 1 and
/// never parses the suffix, so the string is the only real version.
pub const WORKBOOK_SCHEMA: &str = "fungos.sheet/workbook@1";
pub const TAB_SCHEMA: &str = "fungos.sheet/tab@1";
pub const CALC_SCHEMA: &str = "fungos.sheet/calc@1";

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
}

impl Value {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(number) => Some(*number),
            Self::Text(_) => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tab {
    pub title: String,
    pub cells: BTreeMap<String, String>,
}

impl Tab {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), cells: BTreeMap::new() }
    }

    pub fn set(&mut self, address: &str, input: impl Into<String>) {
        self.cells.insert(address.to_string(), input.into());
    }

    /// Record body for `fungos.sheet/tab@1`.
    pub fn to_body(&self) -> serde_json::Value {
        serde_json::json!({ "title": self.title, "cells": self.cells })
    }

    pub fn from_body(body: &[u8]) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_slice(body).map_err(|error| format!("tab body: {error}"))?;
        let title = value
            .get("title")
            .and_then(|title| title.as_str())
            .unwrap_or_default()
            .to_string();
        let cells = value
            .get("cells")
            .and_then(|cells| cells.as_object())
            .ok_or("tab body needs a cells object")?
            .iter()
            .map(|(address, input)| {
                let input = match input {
                    serde_json::Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                (address.clone(), input)
            })
            .collect();
        Ok(Self { title, cells })
    }
}

/// `A1` to `(1, 1)`. Absolute markers are rejected: a tab stores relative inputs.
pub fn parse_address(address: &str) -> Result<(i32, i32), String> {
    let mut letters = String::new();
    let mut digits = String::new();
    for character in address.chars() {
        if character.is_ascii_alphabetic() && digits.is_empty() {
            letters.push(character.to_ascii_uppercase());
        } else if character.is_ascii_digit() && !letters.is_empty() {
            digits.push(character);
        } else {
            return Err(format!("invalid cell address {address:?}"));
        }
    }
    if letters.is_empty() || digits.is_empty() {
        return Err(format!("invalid cell address {address:?}"));
    }
    let mut column = 0i32;
    for byte in letters.bytes() {
        column = column * 26 + (byte - b'A' + 1) as i32;
    }
    let row: i32 = digits.parse().map_err(|_| format!("invalid row in {address:?}"))?;
    Ok((row, column))
}

fn model_for(tab: &Tab, title: &str) -> Result<Model<'static>, String> {
    let mut model = Model::new_empty("workbook.xlsx", "en", "UTC", "en")
        .map_err(|error| error.to_string())?;
    model.rename_sheet("Sheet1", title).map_err(|error| error.to_string())?;
    for (address, input) in &tab.cells {
        let (row, column) = parse_address(address)?;
        model
            .set_user_input(0, row, column, input.clone())
            .map_err(|error| format!("{address}: {error}"))?;
    }
    model.evaluate();
    Ok(model)
}

/// Evaluate every cell in the tab and return the resulting values keyed by address.
pub fn evaluate(tab: &Tab) -> Result<BTreeMap<String, Value>, String> {
    let model = model_for(tab, if tab.title.is_empty() { "Sheet1" } else { &tab.title })?;
    let mut values = BTreeMap::new();
    for address in tab.cells.keys() {
        let (row, column) = parse_address(address)?;
        let value = model
            .get_cell_value_by_index(0, row, column)
            .map_err(|error| format!("{address}: {error}"))?;
        values.insert(
            address.clone(),
            match value {
                CellValue::Number(number) => Value::Number(number),
                CellValue::String(text) => Value::Text(text),
                CellValue::Boolean(flag) => Value::Text(flag.to_string()),
                CellValue::None => Value::Text(String::new()),
            },
        );
    }
    Ok(values)
}

/// Body for `fungos.sheet/calc@1`: the values other Spores read instead of
/// recomputing, plus the tab they came from.
pub fn calc_body(tab_id: &str, values: &BTreeMap<String, Value>) -> serde_json::Value {
    let mut numbers = serde_json::Map::new();
    let mut text = serde_json::Map::new();
    for (address, value) in values {
        match value {
            Value::Number(number) => {
                numbers.insert(address.clone(), serde_json::json!(number));
            }
            Value::Text(value) => {
                text.insert(address.clone(), serde_json::json!(value));
            }
        }
    }
    serde_json::json!({ "tab": tab_id, "numbers": numbers, "text": text })
}

/// A real .xlsx built in memory, ready for the host to store or hand to a user.
pub fn to_xlsx(tab: &Tab, title: &str) -> Result<Vec<u8>, String> {
    let model = model_for(tab, title)?;
    let cursor = save_xlsx_to_writer(&model, Cursor::new(Vec::new()))
        .map_err(|error| error.to_string())?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_addresses() {
        assert_eq!(parse_address("A1").unwrap(), (1, 1));
        assert_eq!(parse_address("B3").unwrap(), (3, 2));
        assert_eq!(parse_address("AA10").unwrap(), (10, 27));
        assert!(parse_address("1A").is_err());
        assert!(parse_address("A").is_err());
    }

    #[test]
    fn evaluates_a_formula() {
        let mut tab = Tab::new("Probe");
        tab.set("A1", "2000000");
        tab.set("B1", "250000");
        tab.set("A2", "=A1-B1/12");
        tab.set("A3", "=ROUND(A2*1.03,2)");
        let values = evaluate(&tab).unwrap();
        assert!((values["A2"].as_number().unwrap() - 1979166.6666666667).abs() < 1e-6);
        assert_eq!(values["A3"].as_number().unwrap(), 2038541.67);
    }

    #[test]
    fn body_round_trips() {
        let mut tab = Tab::new("Inputs");
        tab.set("A1", "100");
        tab.set("A2", "=A1*2");
        let body = serde_json::to_vec(&tab.to_body()).unwrap();
        assert_eq!(Tab::from_body(&body).unwrap(), tab);
    }

    #[test]
    fn writes_an_xlsx_archive() {
        let mut tab = Tab::new("Summary");
        tab.set("A1", "100");
        tab.set("A2", "=A1*2");
        let bytes = to_xlsx(&tab, "Summary").unwrap();
        assert!(bytes.starts_with(b"PK"), "xlsx must be a zip archive");
        let workbook = ironcalc::import::load_from_xlsx_bytes(&bytes, "Summary", "en", "UTC")
            .map_err(|error| error.to_string())
            .unwrap();
        let reopened = Model::from_workbook(workbook, "en").unwrap();
        assert_eq!(reopened.get_formatted_cell_value(0, 2, 1).unwrap(), "200");
    }
}
