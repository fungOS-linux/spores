//! Print one ANSI frame. Useful for piping a TUI into a VT host.
//!
//! ```sh
//! cargo run -p spore-tui --example frame -- 80 24 0 | soil-vt-parse 80 24
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cols = args.first().and_then(|value| value.parse().ok()).unwrap_or(80);
    let rows = args.get(1).and_then(|value| value.parse().ok()).unwrap_or(24);
    let tick = args.get(2).and_then(|value| value.parse().ok()).unwrap_or(0);
    match spore_tui::App::preview(cols, rows, tick).render() {
        Ok(frame) => print!("{frame}"),
        Err(error) => {
            eprintln!("frame: {error}");
            std::process::exit(1);
        }
    }
}
