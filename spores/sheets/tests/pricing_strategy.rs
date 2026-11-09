//! Three products, not three prices for one product.
//!
//! - SaaS small shop: monthly account price with a parts allowance and overage.
//! - Direct/RFQ: per accepted package, a paid pilot, or a team plan.
//! - OEM/SDK: a flat platform bundle or on-prem, with support.
//!
//! Comparative anchors were checked 2026-11-07 and live in
//! `research/pricing-comparables-2026-11-07.md` in fpl-templates. The segment
//! mix is illustrative until FPL's pipeline replaces it; the cell says so.
use sheets_spore::model::{evaluate, Tab, Value};

const CAP_PARTS: f64 = 2_000.0; // included in the SaaS price
const OVERAGE: f64 = 0.10; // per part above the cap
const SDK_LIST: f64 = 60_000.0; // $40k SDK + $20k API at list
const SDK_BUNDLE: f64 = 50_000.0; // 16.7% bundle discount
const ON_PREM: f64 = 70_000.0; // bundle + $20k on-prem
const SUPPORT_RATE: f64 = 0.18; // annual support on licence
const DIRECT_PART: f64 = 199.0; // per accepted package
const PILOT: f64 = 1_500.0; // paid pilot
const TEAM_MONTHLY: f64 = 1_500.0; // team plan
const TEAM_PACKAGES: f64 = 10.0; // included packages

/// Three public options for the small-shop SaaS price.
const SAAS_PRICES: [f64; 3] = [99.0, 125.0, 149.0];
const SELECTED_SAAS: usize = 1;

fn column(index: usize) -> char {
    char::from(b'B' + index as u8)
}

/// (label, product, parts per month, customers out of 100)
#[derive(Clone, Copy, PartialEq)]
enum Product {
    Saas,
    Sdk,
    OnPrem,
}

const SEGMENTS: [(&str, Product, f64, f64); 5] = [
    ("Small shop", Product::Saas, 500.0, 60.0),
    ("Growing shop", Product::Saas, 2_000.0, 25.0),
    ("Bureau", Product::Saas, 10_000.0, 10.0),
    ("Platform ISV", Product::Sdk, 40_000.0, 4.0),
    ("Giant platform", Product::OnPrem, 400_000.0, 1.0),
];

fn strategy_tab(saas_index: usize) -> Tab {
    let mut tab = Tab::new("Pricing");
    tab.set("P1", CAP_PARTS.to_string());
    tab.set("P2", OVERAGE.to_string());
    tab.set("P3", SDK_BUNDLE.to_string());
    tab.set("P4", ON_PREM.to_string());
    tab.set("P5", "Licence wins only above the derived break-even");
    tab.set("P6", "Illustrative segment mix; replace with FPL pipeline");
    tab.set("P7", SAAS_PRICES[saas_index].to_string());
    tab.set("P8", SDK_LIST.to_string());
    tab.set("P9", "=(P8-P3)/P8"); // bundle discount
    tab.set("P10", "=(P3-P7*12)/(P2*12)+P1"); // break-even parts at the selected price
    tab.set("P11", TEAM_MONTHLY.to_string());
    tab.set("P12", TEAM_PACKAGES.to_string());
    tab.set("P13", "=P11/P12"); // effective team price per package
    tab.set("P14", DIRECT_PART.to_string());
    tab.set("P15", PILOT.to_string());
    tab.set("P16", SUPPORT_RATE.to_string());

    for (index, (_, product, parts, _)) in SEGMENTS.iter().enumerate() {
        let c = column(index);
        let saas = if *product == Product::Saas {
            SAAS_PRICES[saas_index]
        } else {
            0.0
        };
        tab.set(&format!("{c}1"), saas.to_string());
        tab.set(&format!("{c}2"), parts.to_string());
        // SaaS customers pay the lower of the metered bill and the licence;
        // a SaaS account is never forced to buy a more expensive licence.
        tab.set(
            &format!("{c}3"),
            match product {
                Product::Saas => format!("=MIN({c}1*12+MAX(0,{c}2-$P$1)*12*$P$2,$P$3)"),
                Product::Sdk => "=$P$3".to_string(),
                Product::OnPrem => "=$P$4".to_string(),
            },
        );
        tab.set(&format!("{c}4"), format!("={c}3/12"));
    }

    // SaaS price positioning against public CAD list prices.
    tab.set("H1", "Comparable");
    tab.set("I1", "Annual price");
    tab.set("J1", "Transmog SaaS");
    tab.set("K1", "Multiple");
    tab.set("J2", "=$P$7*12");
    for (row, (label, price)) in [
        ("Fusion", 680.0),
        ("Onshape Standard", 1_500.0),
        ("SOLIDWORKS Design Standard", 2_820.0),
        ("Creo Design Essentials", 3_350.0),
    ]
    .iter()
    .enumerate()
    {
        let r = row + 2;
        tab.set(&format!("H{r}"), *label);
        tab.set(&format!("I{r}"), price.to_string());
        tab.set(&format!("J{r}"), format!("=$P$7*12"));
        tab.set(&format!("K{r}"), format!("=J{r}/I{r}"));
    }

    // Direct conversion positioning.
    tab.set("H7", "PartWork Simple");
    tab.set("H8", "PartWork Complex");
    tab.set("H9", "MiniCAD low");
    tab.set("H10", "MiniCAD high");
    tab.set("I7", "29.99");
    tab.set("I8", "99.99");
    tab.set("I9", "79");
    tab.set("I10", "149");
    for r in 7..=10 {
        tab.set(&format!("J{r}"), "=$P$14");
        tab.set(&format!("K{r}"), format!("=J{r}/I{r}"));
    }
    tab
}

fn number(values: &std::collections::BTreeMap<String, Value>, address: &str) -> f64 {
    values
        .get(address)
        .unwrap_or_else(|| panic!("no value at {address}"))
        .as_number()
        .unwrap_or_else(|| panic!("{address} is not numeric"))
}

fn text<'a>(values: &'a std::collections::BTreeMap<String, Value>, address: &str) -> &'a str {
    match values
        .get(address)
        .unwrap_or_else(|| panic!("no value at {address}"))
    {
        Value::Text(value) => value,
        Value::Number(_) => panic!("{address} is not text"),
    }
}

fn expected(product: Product, saas: f64, parts: f64) -> f64 {
    match product {
        Product::Saas => {
            (saas * 12.0 + (parts - CAP_PARTS).max(0.0) * 12.0 * OVERAGE).min(SDK_BUNDLE)
        }
        Product::Sdk => SDK_BUNDLE,
        Product::OnPrem => ON_PREM,
    }
}

#[test]
fn each_segment_buys_one_product() {
    for (option, saas_price) in SAAS_PRICES.iter().enumerate() {
        let values = evaluate(&strategy_tab(option)).expect("strategy evaluates");
        for (index, (label, product, parts, _)) in SEGMENTS.iter().enumerate() {
            let c = column(index);
            let saas = if *product == Product::Saas {
                *saas_price
            } else {
                0.0
            };
            let want = expected(*product, saas, *parts);
            assert!(
                (number(&values, &format!("{c}3")) - want).abs() < 1e-6,
                "{label} annual at option {}: {want}",
                option + 1
            );
            assert!(
                (number(&values, &format!("{c}4")) - want / 12.0).abs() < 1e-6,
                "{label} monthly at option {}",
                option + 1
            );
        }
    }
}

#[test]
fn bureau_overage_is_the_awkward_segment() {
    // SaaS alone is $1,500; overage at 10,000 parts is $9,600, so the bill is
    // $11,100. That is the number to test with real bureaus.
    let values = evaluate(&strategy_tab(SELECTED_SAAS)).expect("strategy evaluates");
    assert!((number(&values, "D3") - 11_100.0).abs() < 1e-6);
    assert!((number(&values, "D3") - 1_500.0).abs() > 1_000.0);
}

#[test]
fn platform_revenue_concentrates_in_five_percent_of_customers() {
    let values = evaluate(&strategy_tab(SELECTED_SAAS)).expect("strategy evaluates");
    let mut total = 0.0;
    for (index, (_, _, _, count)) in SEGMENTS.iter().enumerate() {
        total += count * number(&values, &format!("{}3", column(index)));
    }
    assert!((total - 508_500.0).abs() < 1e-6, "total {total}");
    let platforms = 4.0 * number(&values, "E3") + 1.0 * number(&values, "F3");
    assert!(
        (platforms / total - 0.5310).abs() < 0.001,
        "share {}",
        platforms / total
    );
}

#[test]
fn bundle_discount_and_break_even_are_derived() {
    let values = evaluate(&strategy_tab(SELECTED_SAAS)).expect("strategy evaluates");
    assert!((number(&values, "P8") - 60_000.0).abs() < 1e-6);
    assert!((number(&values, "P16") - 0.18).abs() < 1e-6, "support rate");
    assert!(
        (number(&values, "P9") - 1.0 / 6.0).abs() < 1e-6,
        "bundle discount"
    );
    assert!(
        (number(&values, "P10") - 42_416.666_666).abs() < 1e-3,
        "break-even parts"
    );
    // Just below the break-even the metered bill is cheaper; just above it the
    // licence wins. The rule must not force the licence at 20,000 parts.
    let saas = SAAS_PRICES[SELECTED_SAAS];
    let below = saas * 12.0 + (42_000.0 - CAP_PARTS) * 12.0 * OVERAGE;
    let above = saas * 12.0 + (43_000.0 - CAP_PARTS) * 12.0 * OVERAGE;
    assert!(below < SDK_BUNDLE);
    assert!(above > SDK_BUNDLE);
    assert!(
        expected(Product::Saas, saas, 20_000.0) < SDK_BUNDLE,
        "20,000 parts must not be forced onto the licence"
    );
}

#[test]
fn direct_team_plan_is_a_discount_to_the_per_part_price() {
    let values = evaluate(&strategy_tab(SELECTED_SAAS)).expect("strategy evaluates");
    assert!(
        (number(&values, "P13") - 150.0).abs() < 1e-6,
        "effective team price"
    );
    assert!(number(&values, "P14") > number(&values, "P13"));
    assert!((number(&values, "P15") - PILOT).abs() < 1e-6);
}

#[test]
fn comparative_multiples_are_explicit() {
    let values = evaluate(&strategy_tab(SELECTED_SAAS)).expect("strategy evaluates");
    // $125/month = $1,500/year.
    assert!((number(&values, "J2") - 1_500.0).abs() < 1e-6);
    assert!(
        (number(&values, "K2") - 1_500.0 / 680.0).abs() < 1e-6,
        "Fusion multiple"
    );
    assert!(
        (number(&values, "K3") - 1.0).abs() < 1e-6,
        "Onshape multiple"
    );
    assert!(
        (number(&values, "K4") - 1_500.0 / 2_820.0).abs() < 1e-6,
        "SOLIDWORKS multiple"
    );
    assert!(
        (number(&values, "K5") - 1_500.0 / 3_350.0).abs() < 1e-6,
        "Creo multiple"
    );
    // $199 per accepted package.
    assert!(
        (number(&values, "K7") - 199.0 / 29.99).abs() < 1e-6,
        "PartWork Simple multiple"
    );
    assert!(
        (number(&values, "K8") - 199.0 / 99.99).abs() < 1e-6,
        "PartWork Complex multiple"
    );
    assert!(
        (number(&values, "K9") - 199.0 / 79.0).abs() < 1e-6,
        "MiniCAD low multiple"
    );
    assert!(
        (number(&values, "K10") - 199.0 / 149.0).abs() < 1e-6,
        "MiniCAD high multiple"
    );
}

#[test]
fn mix_is_labelled_illustrative_until_a_pipeline_replaces_it() {
    let values = evaluate(&strategy_tab(SELECTED_SAAS)).expect("strategy evaluates");
    assert!(text(&values, "P6").contains("Illustrative"));
    assert!(text(&values, "P6").contains("pipeline"));
}
