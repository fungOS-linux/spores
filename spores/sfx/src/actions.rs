//! Action envelope parsing.
//!
//! The Lua view emits `ui.action(name, payload)` tables; a host may serialize
//! that table as-is, flatten it, or send a plain JSON object. The Spore accepts
//! all three so the view's intent survives the boundary without a
//! host-specific rewrite. This lives outside the wasm-only glue so it can be
//! tested on the host.

use serde_json::Value;

pub fn action_parts(value: &Value) -> Result<(&str, Value), String> {
    match value.get("action") {
        Some(Value::String(name)) => {
            let params = value
                .get("payload")
                .cloned()
                .unwrap_or_else(|| value.clone());
            Ok((name.as_str(), params))
        }
        Some(Value::Object(table)) => {
            let name = table
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| "action table has no action name".to_string())?;
            let params = table
                .get("payload")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default()));
            Ok((name, params))
        }
        _ => Err("action is missing".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_flat_action_keeps_its_fields() {
        let value = json!({"action": "sfx.trigger", "pad": "pop"});
        let (name, params) = action_parts(&value).unwrap();
        assert_eq!(name, "sfx.trigger");
        assert_eq!(params["pad"], "pop");
    }

    #[test]
    fn an_action_with_a_payload_uses_the_payload() {
        let value = json!({"action": "sfx.event", "payload": {"kind": "chat.message.v1"}});
        let (name, params) = action_parts(&value).unwrap();
        assert_eq!(name, "sfx.event");
        assert_eq!(params["kind"], "chat.message.v1");
    }

    #[test]
    fn the_ui_action_table_survives_the_boundary() {
        let value = json!({
            "action": {"action": "sfx.enabled", "payload": {"kind": "ci.failed.v1", "enabled": false}}
        });
        let (name, params) = action_parts(&value).unwrap();
        assert_eq!(name, "sfx.enabled");
        assert_eq!(params["kind"], "ci.failed.v1");
        assert_eq!(params["enabled"], false);
    }

    #[test]
    fn a_missing_action_is_a_named_error() {
        assert!(action_parts(&json!({"pad": "pop"})).is_err());
        assert!(action_parts(&json!({"action": {"payload": {}}})).is_err());
    }
}
