//! The FPL cash model as a tab of formulas, checked against an independent
//! month loop. This is the parity test: if the spreadsheet formulas and the
//! reference disagree, the workbook is wrong no matter how it looks.
use sheets_spore::model::{Tab, Value, evaluate};

const MONTHS: usize = 12;
const PRICE: f64 = 100.0;
const CHURN: f64 = 0.01;
const PAYROLL: f64 = 41566.67;
const OPEX: f64 = 12350.0;
const FUNDING: f64 = 2_000_000.0;
const ADDS_PER_MONTH: f64 = 8.0;

fn column(month: usize) -> char {
    // B is month 1, M is month 12.
    char::from(b'A' + month as u8)
}

fn projection_tab() -> Tab {
    let mut tab = Tab::new("Projection");
    tab.set("P1", PRICE.to_string());
    tab.set("P2", CHURN.to_string());
    tab.set("P3", PAYROLL.to_string());
    tab.set("P4", OPEX.to_string());
    tab.set("P5", FUNDING.to_string());
    for month in 1..=MONTHS {
        let c = column(month);
        let previous = if month == 1 { None } else { Some(column(month - 1)) };
        tab.set(&format!("{c}1"), month.to_string());
        let adds = if month == 1 { "0".to_string() } else { ADDS_PER_MONTH.to_string() };
        tab.set(&format!("{c}2"), adds);
        // Opening accounts: zero in month one, otherwise the previous close.
        match previous {
            None => tab.set(&format!("{c}3"), "0"),
            Some(p) => tab.set(&format!("{c}3"), format!("={p}5")),
        }
        tab.set(&format!("{c}4"), format!("={c}3*$P$2"));
        tab.set(&format!("{c}5"), format!("={c}3-{c}4+{c}2"));
        tab.set(&format!("{c}6"), format!("={c}3-{c}4+{c}2/2"));
        tab.set(&format!("{c}7"), format!("={c}6*$P$1"));
        // Collections lag recognition by one month.
        match previous {
            None => tab.set(&format!("{c}8"), "0"),
            Some(p) => tab.set(&format!("{c}8"), format!("={p}7")),
        }
        tab.set(&format!("{c}9"), "=$P$3");
        tab.set(&format!("{c}10"), "=$P$4");
        tab.set(&format!("{c}11"), format!("={c}8-{c}9-{c}10"));
        match previous {
            None => tab.set(&format!("{c}12"), format!("=$P$5+{c}11")),
            Some(p) => tab.set(&format!("{c}12"), format!("={p}12+{c}11")),
        }
    }
    tab
}

/// Independent month loop. Deliberately written as plain arithmetic so it
/// cannot share a bug with the spreadsheet formulas.
fn reference() -> Vec<(f64, f64, f64, f64, f64)> {
    let mut accounts = 0.0f64;
    let mut previous_revenue = 0.0f64;
    let mut cash = FUNDING;
    let mut rows = Vec::new();
    for month in 1..=MONTHS {
        let adds = if month == 1 { 0.0 } else { ADDS_PER_MONTH };
        let lost = accounts * CHURN;
        let billable = accounts - lost + adds / 2.0;
        accounts = accounts - lost + adds;
        let revenue = billable * PRICE;
        let collected = previous_revenue;
        previous_revenue = revenue;
        let net = collected - PAYROLL - OPEX;
        cash += net;
        rows.push((accounts, revenue, collected, net, cash));
    }
    rows
}

fn number(values: &std::collections::BTreeMap<String, Value>, address: &str) -> f64 {
    values
        .get(address)
        .unwrap_or_else(|| panic!("no value at {address}"))
        .as_number()
        .unwrap_or_else(|| panic!("{address} is not numeric"))
}

#[test]
fn formulas_match_the_reference_loop() {
    let tab = projection_tab();
    let values = evaluate(&tab).expect("tab evaluates");
    for (index, (accounts, revenue, collected, net, cash)) in reference().into_iter().enumerate() {
        let month = index + 1;
        let c = column(month);
        let close = number(&values, &format!("{c}5"));
        let revenue_cell = number(&values, &format!("{c}7"));
        let collected_cell = number(&values, &format!("{c}8"));
        let net_cell = number(&values, &format!("{c}11"));
        let cash_cell = number(&values, &format!("{c}12"));
        assert!((close - accounts).abs() < 1e-6, "month {month} accounts {close} != {accounts}");
        assert!((revenue_cell - revenue).abs() < 1e-6, "month {month} revenue");
        assert!((collected_cell - collected).abs() < 1e-6, "month {month} collected");
        assert!((net_cell - net).abs() < 1e-6, "month {month} net");
        assert!((cash_cell - cash).abs() < 1e-6, "month {month} cash");
    }
}

#[test]
fn month_twelve_revenue_is_published() {
    let tab = projection_tab();
    let values = evaluate(&tab).expect("tab evaluates");
    let revenue = number(&values, "M7");
    let expected = reference()[11].1;
    assert!((revenue - expected).abs() < 1e-6, "{revenue} != {expected}");
}
