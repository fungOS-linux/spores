//! The wasm guest: state in, ANSI frames out.
//!
//! The host calls `start` once, then `key`/`resize` as input arrives. Each call
//! returns a whole frame, so a dropped frame costs one redraw rather than a
//! desynchronised screen.

wit_bindgen::generate!({ path: "wit", world: "tui", generate_all });

use std::sync::Mutex;

use crate::App;

static APP: Mutex<Option<App>> = Mutex::new(None);

struct Component;

export!(Component);

impl Guest for Component {
    fn start(cols: u16, rows: u16) -> Result<String, String> {
        let app = App::new(cols, rows);
        let frame = app.render()?;
        *APP.lock().map_err(|_| "app poisoned")? = Some(app);
        Ok(frame)
    }

    fn key(key: String) -> Result<String, String> {
        let mut guard = APP.lock().map_err(|_| "app poisoned")?;
        let app = guard.as_mut().ok_or("start first")?;
        app.key(&key);
        app.render()
    }

    fn resize(cols: u16, rows: u16) -> Result<String, String> {
        let mut guard = APP.lock().map_err(|_| "app poisoned")?;
        let app = guard.as_mut().ok_or("start first")?;
        app.resize(cols, rows);
        app.render()
    }

    fn frame() -> Result<String, String> {
        let guard = APP.lock().map_err(|_| "app poisoned")?;
        guard.as_ref().ok_or("start first")?.render()
    }

    fn render(cols: u16, rows: u16, tick: u64) -> Result<String, String> {
        App::preview(cols, rows, tick).render()
    }
}
