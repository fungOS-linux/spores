use super::*;
use std::collections::VecDeque;

/// One trimmed `/v1/search` response for Seattle.
const SEATTLE_PLACE: &str = r#"{
  "results": [{
    "id": 5809844, "name": "Seattle", "latitude": 47.60621, "longitude": -122.33207,
    "elevation": 56.0, "feature_code": "PPLA2", "country_code": "US",
    "timezone": "America/Los_Angeles", "population": 780995,
    "country_id": 6252001, "country": "United States", "admin1": "Washington"
  }],
  "generationtime_ms": 0.15
}"#;

/// One trimmed `/v1/forecast` response for Seattle, 2026-11-13.
const SEATTLE_FORECAST: &str = r#"{
  "utc_offset_seconds": -28800,
  "timezone": "America/Los_Angeles",
  "current": {"time": "2026-11-13T20:45", "interval": 900, "temperature_2m": 12.8, "weather_code": 61},
  "daily": {
    "time": ["2026-11-13", "2026-11-14"],
    "weather_code": [61, 3],
    "temperature_2m_max": [14.2, 13.1],
    "temperature_2m_min": [8.1, 7.4],
    "sunrise": ["2026-11-13T07:12", "2026-11-14T07:13"],
    "sunset": ["2026-11-13T16:38", "2026-11-14T16:37"]
  }
}"#;

struct FakeHost {
    responses: VecDeque<Result<Vec<u8>, String>>,
    requests: Vec<String>,
    records: Vec<(String, Value)>,
    write_error: Option<String>,
    now_ms: u64,
}

impl FakeHost {
    fn new(responses: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            responses: responses
                .into_iter()
                .map(|body| Ok(body.as_bytes().to_vec()))
                .collect(),
            requests: Vec::new(),
            records: Vec::new(),
            write_error: None,
            now_ms: 1_000,
        }
    }

    fn failing(error: &str) -> Self {
        Self {
            responses: VecDeque::from([Err(error.to_string())]),
            requests: Vec::new(),
            records: Vec::new(),
            write_error: None,
            now_ms: 1_000,
        }
    }
}

impl Host for FakeHost {
    fn get(&mut self, url: &str) -> Result<Vec<u8>, String> {
        self.requests.push(url.to_string());
        self.responses
            .pop_front()
            .unwrap_or_else(|| Err("no response left".to_string()))
    }

    fn upsert(&mut self, id: &str, body: &[u8]) -> Result<(), String> {
        if let Some(error) = &self.write_error {
            return Err(error.clone());
        }
        self.records.push((
            id.to_string(),
            serde_json::from_slice(body).expect("record body"),
        ));
        Ok(())
    }

    fn monotonic_ms(&mut self) -> Result<u64, String> {
        Ok(self.now_ms)
    }
}

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn a_place_is_geocoded_then_written_then_summarized() {
    let mut host = FakeHost::new([SEATTLE_PLACE, SEATTLE_FORECAST]);
    let output = run(&args("now --place Seattle"), &mut host).expect("run");

    assert_eq!(
        host.requests,
        vec![
            "https://geocoding-api.open-meteo.com/v1/search?name=Seattle&count=1&language=en&format=json",
            "https://api.open-meteo.com/v1/forecast?latitude=47.60621&longitude=-122.33207\
&current=temperature_2m,weather_code\
&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset\
&timezone=auto&forecast_days=5",
        ]
    );
    assert_eq!(host.records.len(), 1);
    let (id, body) = &host.records[0];
    assert_eq!(id, "seattle");
    assert_eq!(body["location"], json!("Seattle"));
    assert_eq!(body["latitude"], json!(47.60621));
    assert_eq!(body["timestamp"], json!(1_794_602_700i64));
    assert_eq!(body["fetched_at_monotonic_ms"], json!(1_000));
    assert_eq!(body["schema_version"], json!(1));

    assert_eq!(
        output,
        "Seattle: 12.8\u{b0}C rain, 8.1\u{b0}C to 14.2\u{b0}C, sunrise 07:12, sunset 16:38 [weather.current/seattle]"
    );
}

#[test]
fn coordinates_skip_geocoding_and_name_the_record_after_the_point() {
    let mut host = FakeHost::new([SEATTLE_FORECAST]);
    let output = run(&args("now --lat 47.6 --lon -122.3"), &mut host).expect("run");
    assert_eq!(host.requests.len(), 1, "no geocoding request");
    assert!(
        host.requests[0].starts_with("https://api.open-meteo.com/v1/forecast?latitude=47.6"),
        "{}",
        host.requests[0]
    );
    assert_eq!(host.records[0].0, "47.6000,-122.3000");
    assert_eq!(host.records[0].1["location"], json!("47.6000,-122.3000"));
    assert_eq!(
        output,
        "47.6000,-122.3000: 12.8\u{b0}C rain, 8.1\u{b0}C to 14.2\u{b0}C, sunrise 07:12, sunset 16:38 [weather.current/47.6000,-122.3000]"
    );
}

#[test]
fn a_label_names_both_the_watch_and_the_record() {
    let mut host = FakeHost::new([SEATTLE_FORECAST]);
    let output = run(
        &args("now --lat 47.6 --lon -122.3 --label Living-room"),
        &mut host,
    )
    .expect("run");
    assert_eq!(host.records[0].0, "living-room");
    assert_eq!(host.records[0].1["location"], json!("Living-room"));
    assert!(
        output.starts_with("Living-room: 12.8\u{b0}C rain"),
        "{output}"
    );
}

#[test]
fn json_prints_the_body_instead_of_the_summary() {
    let mut host = FakeHost::new([SEATTLE_PLACE, SEATTLE_FORECAST]);
    let output = run(&args("now --json --place Seattle"), &mut host).expect("run");
    let parsed: Value = serde_json::from_str(&output).expect("json output");
    assert_eq!(parsed, host.records[0].1);
    assert!(output.starts_with("{\n"), "{output}");
}

#[test]
fn a_polar_sun_reads_as_no_sun_not_midnight() {
    let forecast = SEATTLE_FORECAST
        .replace("\"2026-11-13T07:12\"", "null")
        .replace("\"2026-11-14T07:13\"", "null")
        .replace("\"2026-11-13T16:38\"", "null")
        .replace("\"2026-11-14T16:37\"", "null");
    let mut host = FakeHost::new([]);
    host.responses.push_back(Ok(forecast.into_bytes()));
    let output = run(&args("now --lat 69.65 --lon 18.96"), &mut host).expect("run");
    assert_eq!(host.records[0].1["sunrise_min"], json!(-2));
    assert!(output.contains("no sunrise or sunset today"), "{output}");
}

#[test]
fn a_missing_place_names_the_flags_that_would_supply_one() {
    let error = run(&args("now"), &mut FakeHost::new([])).unwrap_err();
    assert!(error.contains("--place"), "{error}");
    assert!(error.contains("--lat"), "{error}");
}

#[test]
fn half_a_coordinate_names_what_is_missing() {
    let error = run(&args("now --lat 47.6"), &mut FakeHost::new([])).unwrap_err();
    assert_eq!(error, "--lat needs --lon as well");
    let error = run(&args("now --lon -122.3"), &mut FakeHost::new([])).unwrap_err();
    assert_eq!(error, "--lon needs --lat as well");
}

#[test]
fn a_place_and_coordinates_are_mutually_exclusive() {
    let error = run(
        &args("now --place Seattle --lat 47.6 --lon -122.3"),
        &mut FakeHost::new([]),
    )
    .unwrap_err();
    assert!(error.contains("not both"), "{error}");
}

#[test]
fn a_coordinate_out_of_range_is_refused_before_any_request() {
    let mut host = FakeHost::new([]);
    let error = run(&args("now --lat 91 --lon 0"), &mut host).unwrap_err();
    assert_eq!(error, "--lat: 91 is outside -90..90");
    assert!(host.requests.is_empty());
}

#[test]
fn an_unmatched_place_names_the_query() {
    let mut host = FakeHost::new(["{}"]);
    let error = run(&args("now --place Seattl"), &mut host).unwrap_err();
    assert_eq!(error, "geocoding: no place matched \"Seattl\"");
    assert!(host.records.is_empty());
}

#[test]
fn a_denied_origin_names_the_host_rule_and_the_url() {
    let mut host =
        FakeHost::failing("denied: the host's origin allowlist does not include this URL");
    let error = run(&args("now --place Seattle"), &mut host).unwrap_err();
    assert!(error.starts_with("geocoding: denied:"), "{error}");
    assert!(
        error.contains("https://geocoding-api.open-meteo.com/v1/search"),
        "{error}"
    );
}

#[test]
fn a_label_the_watch_cannot_show_is_refused_with_the_limit() {
    let mut host = FakeHost::new([SEATTLE_PLACE, SEATTLE_FORECAST]);
    let error = run(
        &[
            "now".to_string(),
            "--place".to_string(),
            "Seattle".to_string(),
            "--label".to_string(),
            "x".repeat(33),
        ],
        &mut host,
    )
    .unwrap_err();
    assert!(error.contains("the watch field holds 32"), "{error}");
    assert!(error.contains("--label"), "{error}");
    assert!(host.records.is_empty());
}

#[test]
fn an_unknown_weather_code_names_the_code_and_writes_nothing() {
    let forecast = SEATTLE_FORECAST.replace("\"weather_code\": 61}", "\"weather_code\": 46}");
    let mut host = FakeHost::new([]);
    host.responses.push_back(Ok(forecast.into_bytes()));
    let error = run(&args("now --lat 47.6 --lon -122.3"), &mut host).unwrap_err();
    assert_eq!(
        error,
        "weather code 46 is not in the WMO table this Spore maps"
    );
    assert!(host.records.is_empty());
}

#[test]
fn a_refused_write_names_the_record_and_the_reason() {
    let mut host = FakeHost::new([SEATTLE_FORECAST]);
    host.write_error = Some("denied".to_string());
    let error = run(&args("now --lat 47.6 --lon -122.3"), &mut host).unwrap_err();
    assert_eq!(error, "records: weather.current/47.6000,-122.3000: denied");
    assert!(host.records.is_empty());
}

#[test]
fn the_snapshot_names_what_was_written() {
    let mut host = FakeHost::new([SEATTLE_PLACE, SEATTLE_FORECAST]);
    let outcome = execute(&args("now --place Seattle"), &mut host).expect("run");
    assert_eq!(outcome.snapshot["ok"], json!(true));
    assert_eq!(outcome.snapshot["id"], json!("seattle"));
    assert_eq!(outcome.snapshot["schema"], json!("weather.current.v1"));
    assert_eq!(outcome.snapshot["collection"], json!("weather.current"));
    assert_eq!(outcome.snapshot["fetched_at_monotonic_ms"], json!(1_000));
}

#[test]
fn help_is_a_successful_run() {
    let outcome = execute(&args("--help"), &mut FakeHost::new([])).expect("help");
    assert!(
        outcome.output.contains("weather now --place NAME"),
        "{}",
        outcome.output
    );
}

#[test]
fn an_unknown_command_says_what_it_expected() {
    let error = run(&args("forecast"), &mut FakeHost::new([])).unwrap_err();
    assert_eq!(error, "unknown command \"forecast\"; expected now");
}

#[test]
fn the_summary_formats_minutes_as_a_clock() {
    assert_eq!(clock(0), "00:00");
    assert_eq!(clock(7 * 60 + 12), "07:12");
    assert_eq!(clock(16 * 60 + 38), "16:38");
    assert_eq!(clock(-1), "none");
    assert_eq!(clock(-2), "none");
}
