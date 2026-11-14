use super::*;
use serde_json::json;

/// One real `/v1/forecast` response, trimmed to the fields this Spore asks for.
/// Seattle, 2026-11-13, local time.
const SEATTLE: &[u8] = br#"{
  "latitude": 47.55,
  "longitude": -122.25,
  "generationtime_ms": 0.19,
  "utc_offset_seconds": -28800,
  "timezone": "America/Los_Angeles",
  "timezone_abbreviation": "PST",
  "elevation": 56.0,
  "current_units": {"time": "iso8601", "interval": "seconds", "temperature_2m": "\u00b0C", "weather_code": "wmo code"},
  "current": {"time": "2026-11-13T20:45", "interval": 900, "temperature_2m": 12.8, "weather_code": 61},
  "daily_units": {"time": "iso8601", "weather_code": "wmo code", "temperature_2m_max": "\u00b0C", "temperature_2m_min": "\u00b0C", "sunrise": "iso8601", "sunset": "iso8601"},
  "daily": {
    "time": ["2026-11-13", "2026-11-14", "2026-11-15", "2026-11-16", "2026-11-17"],
    "weather_code": [61, 3, 80, 71, 0],
    "temperature_2m_max": [14.2, 13.1, 11.0, 4.5, 6.0],
    "temperature_2m_min": [8.1, 7.4, 5.2, -1.0, 0.5],
    "sunrise": ["2026-11-13T07:12", "2026-11-14T07:13", "2026-11-15T07:15", "2026-11-16T07:16", "2026-11-17T07:18"],
    "sunset": ["2026-11-13T16:38", "2026-11-14T16:37", "2026-11-15T16:36", "2026-11-16T16:35", "2026-11-17T16:34"]
  }
}"#;

/// A polar winter: the sun does not rise, and the provider says so with nulls.
const TROMSO: &[u8] = br#"{
  "utc_offset_seconds": 3600,
  "timezone": "Europe/Oslo",
  "current": {"time": "2026-12-21T12:00", "interval": 900, "temperature_2m": -9.5, "weather_code": 71},
  "daily": {
    "time": ["2026-12-21"],
    "weather_code": [71],
    "temperature_2m_max": [-7.0],
    "temperature_2m_min": [-12.0],
    "sunrise": [null],
    "sunset": [null]
  }
}"#;

fn request<'a>(location: &'a str, latitude: f64, longitude: f64) -> SampleRequest<'a> {
    SampleRequest {
        location,
        latitude,
        longitude,
        fetched_at_monotonic_ms: 5_000,
    }
}

fn seattle() -> CurrentWeather {
    parse_forecast(SEATTLE, &request("Seattle", 47.6062, -122.3321)).expect("seattle")
}

#[test]
fn the_forecast_url_is_the_documented_request() {
    assert_eq!(
        forecast_url(FORECAST_ORIGIN, 47.6062, -122.3321),
        "https://api.open-meteo.com/v1/forecast?latitude=47.6062&longitude=-122.3321\
&current=temperature_2m,weather_code\
&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset\
&timezone=auto&forecast_days=5"
    );
}

#[test]
fn the_geocoding_url_escapes_the_place_name() {
    assert_eq!(
        geocode_url(GEOCODING_ORIGIN, "New York, NY"),
        "https://geocoding-api.open-meteo.com/v1/search?name=New%20York%2C%20NY&count=1&language=en&format=json"
    );
    assert_eq!(percent_encode("Z\u{fc}rich"), "Z%C3%BCrich");
}

#[test]
fn a_geocoded_place_keeps_its_coordinates() {
    let body = br#"{
      "results": [{
        "id": 5809844,
        "name": "Seattle",
        "latitude": 47.60621,
        "longitude": -122.33207,
        "elevation": 56.0,
        "feature_code": "PPLA2",
        "country_code": "US",
        "timezone": "America/Los_Angeles",
        "population": 780995,
        "country_id": 6252001,
        "country": "United States",
        "admin1": "Washington"
      }],
      "generationtime_ms": 0.15
    }"#;
    let place = parse_geocode(body).expect("place").expect("some place");
    assert_eq!(place.name, "Seattle");
    assert_eq!(place.latitude, 47.60621);
    assert_eq!(place.longitude, -122.33207);
    assert_eq!(place.describe(), "Seattle, Washington, United States");
}

#[test]
fn a_place_search_without_matches_is_none_not_a_guess() {
    assert_eq!(parse_geocode(br#"{"generationtime_ms": 0.1}"#), Ok(None));
    assert_eq!(parse_geocode(b"{}"), Ok(None));
    assert_eq!(parse_geocode(br#"{"results": []}"#), Ok(None));
}

#[test]
fn a_provider_error_carries_its_reason() {
    let body = br#"{"error": true, "reason": "Cannot initialize WeatherVariable from invalid String value tempeture_2m"}"#;
    let error = parse_forecast(body, &request("Seattle", 0.0, 0.0)).unwrap_err();
    assert_eq!(
        error.to_string(),
        "open-meteo: Cannot initialize WeatherVariable from invalid String value tempeture_2m"
    );
    assert!(matches!(parse_geocode(body), Err(Error::Provider(_))));
}

#[test]
fn the_wmo_table_maps_every_documented_code() {
    let expected = |code: i64| match code {
        0 => Icon::Clear,
        1 | 2 => Icon::FewClouds,
        3 => Icon::Clouds,
        45 | 48 => Icon::Mist,
        51..=67 => Icon::Rain,
        71..=77 => Icon::Snow,
        80..=82 => Icon::CloudsRain,
        85 | 86 => Icon::Snow,
        95..=99 => Icon::Thunderstorm,
        other => panic!("code {other} is not in the table"),
    };
    // Every code with a WMO meaning: the ranges this Spore documents, plus the
    // snow showers 85/86 that the same table carries.
    let mut checked = 0;
    for code in 0..=99 {
        let documented = matches!(code, 0..=3 | 45 | 48 | 51..=67 | 71..=77 | 80..=82 | 85 | 86 | 95..=99);
        if documented {
            assert_eq!(icon_for_code(code), Ok(expected(code)), "code {code}");
            checked += 1;
        } else {
            assert_eq!(
                icon_for_code(code),
                Err(Error::UnknownWeatherCode(code)),
                "code {code} has no mapping"
            );
        }
    }
    assert_eq!(checked, 4 + 2 + 17 + 7 + 3 + 2 + 5, "every documented code is covered");
}

#[test]
fn an_unknown_code_names_itself() {
    let error = icon_for_code(46).unwrap_err();
    assert_eq!(
        error.to_string(),
        "weather code 46 is not in the WMO table this Spore maps"
    );
}

#[test]
fn the_local_timestamp_is_the_utc_epoch_plus_the_offset() {
    // 2026-11-13T20:45 in a -08:00 zone. The wall clock, read as seconds, is
    // the local timestamp the watch shows; the same instant written in UTC is
    // eight hours later and must produce the same number once the offset is
    // applied.
    let local = local_seconds("2026-11-13T20:45", "current.time").unwrap();
    assert_eq!(local, 1_794_602_700);
    let utc = local_seconds("2026-11-14T04:45", "current.time").unwrap();
    assert_eq!(utc + i64::from(-28_800), local);
    assert_eq!(local_seconds("1970-01-01T00:00", "x").unwrap(), 0);
}

#[test]
fn a_timestamp_with_an_offset_suffix_is_refused() {
    for value in [
        "2026-11-13T20:45Z",
        "2026-11-13T20:45:00+00:00",
        "2026-13-13T20:45",
        "2026-11-13 20:45",
    ] {
        assert!(
            matches!(local_seconds(value, "current.time"), Err(Error::BadTime { .. })),
            "{value} must be refused"
        );
    }
    assert_eq!(
        local_seconds("2026-11-13T20:45:30", "t").unwrap(),
        1_794_602_730
    );
}

#[test]
fn the_forecast_becomes_one_record() {
    let record = seattle();
    assert_eq!(record.location, "Seattle");
    assert_eq!(record.latitude, 47.6062);
    assert_eq!(record.longitude, -122.3321);
    assert_eq!(record.timestamp, 1_794_602_700);
    assert_eq!(record.utc_offset_seconds, -28_800);
    assert_eq!(record.timezone, "America/Los_Angeles");
    assert_eq!(record.temp_c, 12.8);
    assert_eq!(record.min_c, 8.1);
    assert_eq!(record.max_c, 14.2);
    assert_eq!(record.icon, Icon::Rain);
    assert_eq!(record.sunrise_min, 7 * 60 + 12);
    assert_eq!(record.sunset_min, 16 * 60 + 38);
    assert_eq!(record.fetched_at_monotonic_ms, 5_000);
    assert_eq!(record.forecast.len(), 5);
    assert_eq!(
        record.forecast[1],
        ForecastDay {
            date: "2026-11-14".into(),
            min_c: 7.4,
            max_c: 13.1,
            icon: Icon::Clouds,
        }
    );
    assert_eq!(
        record
            .forecast
            .iter()
            .map(|day| day.icon)
            .collect::<Vec<_>>(),
        vec![
            Icon::Rain,
            Icon::Clouds,
            Icon::CloudsRain,
            Icon::Snow,
            Icon::Clear
        ]
    );
}

#[test]
fn a_polar_null_sun_is_absent_not_zero() {
    let record = parse_forecast(TROMSO, &request("Tromso", 69.65, 18.96)).expect("tromso");
    assert_eq!(record.sunrise_min, SUN_ABSENT);
    assert_eq!(record.sunset_min, SUN_ABSENT);
    assert_ne!(record.sunrise_min, 0);
    assert_eq!(record.icon, Icon::Snow);
}

#[test]
fn a_missing_sun_array_names_the_field() {
    let body = br#"{
      "utc_offset_seconds": 0, "timezone": "UTC",
      "current": {"time": "2026-11-13T20:45", "temperature_2m": 1.0, "weather_code": 0},
      "daily": {"time": ["2026-11-13"], "weather_code": [0],
        "temperature_2m_max": [2.0], "temperature_2m_min": [1.0],
        "sunset": ["2026-11-13T16:38"]}
    }"#;
    let error = parse_forecast(body, &request("Nowhere", 0.0, 0.0)).unwrap_err();
    assert_eq!(error.to_string(), "missing daily.sunrise");
}

#[test]
fn a_short_daily_array_names_the_missing_day() {
    let body = br#"{
      "utc_offset_seconds": 0, "timezone": "UTC",
      "current": {"time": "2026-11-13T20:45", "temperature_2m": 1.0, "weather_code": 0},
      "daily": {"time": ["2026-11-13", "2026-11-14"], "weather_code": [0],
        "temperature_2m_max": [2.0, 3.0], "temperature_2m_min": [1.0, 2.0],
        "sunrise": [null, null], "sunset": [null, null]}
    }"#;
    let error = parse_forecast(body, &request("Nowhere", 0.0, 0.0)).unwrap_err();
    assert_eq!(error.to_string(), "missing daily.weather_code[1]");
}

#[test]
fn the_record_body_carries_the_whole_schema() {
    let body = seattle().body();
    assert_eq!(
        body,
        json!({
            "schema_version": 1,
            "location": "Seattle",
            "latitude": 47.6062,
            "longitude": -122.3321,
            "timestamp": 1_794_602_700i64,
            "utc_offset_seconds": -28_800,
            "timezone": "America/Los_Angeles",
            "temp_c": 12.8,
            "min_c": 8.1,
            "max_c": 14.2,
            "icon": "rain",
            "sunrise_min": 432,
            "sunset_min": 998,
            "source": "open-meteo",
            "model_version": "weather-spore@0.1.0",
            "fetched_at_monotonic_ms": 5_000,
            "forecast": [
                {"date": "2026-11-13", "min_c": 8.1, "max_c": 14.2, "icon": "rain"},
                {"date": "2026-11-14", "min_c": 7.4, "max_c": 13.1, "icon": "clouds"},
                {"date": "2026-11-15", "min_c": 5.2, "max_c": 11.0, "icon": "clouds-rain"},
                {"date": "2026-11-16", "min_c": -1.0, "max_c": 4.5, "icon": "snow"},
                {"date": "2026-11-17", "min_c": 0.5, "max_c": 6.0, "icon": "clear"},
            ],
        })
    );
}

#[test]
fn a_body_round_trips_through_from_body() {
    let record = seattle();
    let body = serde_json::to_vec(&record.body()).expect("json");
    assert_eq!(CurrentWeather::from_body(&body), Ok(record));
}

#[test]
fn a_reader_refuses_a_schema_it_does_not_know() {
    let mut body = seattle().body();
    body["schema_version"] = json!(2);
    let error = CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "weather.current.v1 version 2 is not a schema this Spore reads"
    );
}

#[test]
fn a_reader_names_a_missing_or_unknown_field() {
    let mut body = seattle().body();
    body.as_object_mut().unwrap().remove("temp_c");
    let error = CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap()).unwrap_err();
    assert_eq!(error.to_string(), "missing temp_c");

    let mut body = seattle().body();
    body["icon"] = json!("hail");
    let error = CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap()).unwrap_err();
    assert_eq!(error.to_string(), "unknown weather icon \"hail\"");

    let mut body = seattle().body();
    body["sunrise_min"] = json!("07:12");
    let error = CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap()).unwrap_err();
    assert_eq!(error.to_string(), "sunrise_min is not a number");
}

#[test]
fn only_the_nine_icon_names_parse() {
    for icon in Icon::ALL {
        assert_eq!(Icon::parse(icon.as_str()), Ok(icon));
    }
    assert_eq!(Icon::ALL.len(), 9);
    assert!(Icon::parse("showers").is_err());
}

#[test]
fn the_record_id_is_a_slug_or_the_coordinates() {
    assert_eq!(record_id(Some("Seattle"), 47.6, -122.3).unwrap(), "seattle");
    assert_eq!(
        record_id(Some("Living Room"), 47.6, -122.3).unwrap(),
        "living-room"
    );
    assert_eq!(
        record_id(Some("  Z\u{fc}rich  "), 47.6, -122.3).unwrap(),
        "z-rich"
    );
    assert_eq!(record_id(None, 47.60621, -122.33207).unwrap(), "47.6062,-122.3321");
    assert!(matches!(
        record_id(Some("\u{6771}\u{4eac}"), 1.0, 2.0),
        Err(Error::EmptyRecordId(_))
    ));
}

#[test]
fn a_location_the_watch_would_truncate_is_refused() {
    assert!(check_location("Living room").is_ok());
    assert!(check_location(&"a".repeat(LOCATION_BYTES)).is_ok());
    let error = check_location(&"a".repeat(LOCATION_BYTES + 1)).unwrap_err();
    assert_eq!(error.to_string(), "location is 33 bytes; the watch field holds 32");

    let error = parse_forecast(SEATTLE, &request(&"b".repeat(40), 0.0, 0.0)).unwrap_err();
    assert!(matches!(error, Error::LocationTooLong(40)));
}

#[test]
fn a_date_with_a_clock_still_names_the_day() {
    let mut body: Value = serde_json::from_slice(SEATTLE).unwrap();
    body["daily"]["time"][0] = json!("2026-11-13T00:00");
    let record = parse_forecast(
        &serde_json::to_vec(&body).unwrap(),
        &request("Seattle", 47.6062, -122.3321),
    )
    .expect("record");
    assert_eq!(record.forecast[0].date, "2026-11-13");
}

#[test]
fn a_body_that_is_not_json_says_so() {
    let error = CurrentWeather::from_body(b"not json").unwrap_err();
    assert!(error.to_string().starts_with("response is not JSON:"), "{error}");
}
