//! Installed-Spore browser.

use serde_json::json;

wit_bindgen::generate!({ path: "wit", world: "spore-browser", generate_all });

use soil::spore::catalog;

struct Component;
export!(Component);

impl Guest for Component {
    fn snapshot() -> Result<Vec<u8>, String> {
        let entries = catalog::entries().map_err(|error| format!("catalog: {error:?}"))?;
        let items = entries
            .into_iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "name": entry.name,
                    "version": entry.version,
                    "summary": entry.summary,
                    "workspace": entry.workspace,
                    "default_surface": entry.default_surface,
                    "surfaces": entry.surfaces,
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_vec(&json!({"revision": 1, "entries": items}))
            .map_err(|error| error.to_string())
    }

    fn launch(id: String, surface: String) -> Result<String, String> {
        catalog::launch(&id, &surface)
            .map(|launched| launched.target)
            .map_err(|error| format!("launch: {error:?}"))
    }
}
