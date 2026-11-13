//! Build the demo vault the screenshots and the tests use.
//!
//!     cargo run --example demo_vault -- /tmp/doc-shell-demo
//!     cargo run -- --root /tmp/doc-shell-demo/vault

use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("demo-vault"));
    let summary = doc_shell::demo::build(&out)?;
    println!("vault:     {}", summary.vault.display());
    println!("records:   {}", summary.records.display());
    println!("documents: 5 markdown files, 1 the layer does not model");
    println!("sessions:  {}", summary.sessions);
    println!();
    println!("  cargo run -- --root {}", summary.vault.display());
    Ok(())
}
