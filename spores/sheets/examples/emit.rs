//! Writes the 12-month projection tab to an .xlsx for inspection.
use sheets_spore::model::Tab;

fn main() -> Result<(), String> {
    let mut tab = Tab::new("Projection");
    tab.set("A1", "Month");
    tab.set("B1", "1");
    tab.set("C1", "2");
    tab.set("B2", "=B1*100");
    tab.set("C2", "=C1*100");
    let bytes = sheets_spore::model::to_xlsx(&tab, "Projection")?;
    std::fs::write("projection.xlsx", &bytes).map_err(|e| e.to_string())?;
    println!("wrote projection.xlsx ({} bytes)", bytes.len());
    Ok(())
}
