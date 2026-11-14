//! NOAA / National Weather Service provider.
//!
//! Source: <https://www.weather.gov/documentation/services-web-api>
//!
//! Two calls: `/points/{lat},{lon}` resolves a grid and the zone's name, then
//! its `forecast` URL returns alternating day/night periods. No key, but the
//! service asks for a User-Agent that identifies the caller, and coverage is
//! the United States and its territories only.
//!
//! NWS publishes no sunrise or sunset, so those come back as
//! [`SUN_UNKNOWN`]. That is an honest gap, not a zero: the watch renders
//! "unknown" rather than claiming the sun rises at midnight.

use serde_json::Value;

use crate::{CurrentWeather, Error, ForecastDay, Icon, SampleRequest, SUN_UNKNOWN};

pub const ORIGIN: &str = "https://api.weather.gov";
/// The service asks callers to identify themselves.
pub const USER_AGENT: &str = "soil-weather/0.1 (dev.fungos.weather)";

/// `Ok(None)` when the point is outside NWS coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub forecast_url: String,
    pub timezone: String,
}

#[must_use]
pub fn points_url(origin: &str, latitude: f64, longitude: f64) -> String {
    format!("{origin}/points/{latitude:.4},{longitude:.4}")
}

pub fn parse_points(body: &[u8]) -> Result<Option<Grid>, Error> {
    let value: Value = serde_json::from_slice(body).map_err(|e| Error::Json(e.to_string()))?;
    if let Some(error) = crate::provider_error(&value) {
        return Err(error);
    }
    if value.get("status").is_some() && value.get("properties").is_none() {
        // NWS answers 404 with a JSON problem body for points it does not
        // cover; the caller decides whether that is a fallback or an error.
        return Ok(None);
    }
    let properties = value
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Missing("properties".into()))?;
    let forecast_url = properties
        .get("forecast")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Missing("properties.forecast".into()))?
        .to_string();
    let timezone = properties
        .get("timeZone")
        .and_then(Value::as_str)
        .unwrap_or("UTC")
        .to_string();
    Ok(Some(Grid {
        forecast_url,
        timezone,
    }))
}

pub fn parse_forecast(body: &[u8], request: &SampleRequest<'_>, grid: &Grid) -> Result<CurrentWeather, Error> {
    let value: Value = serde_json::from_slice(body).map_err(|e| Error::Json(e.to_string()))?;
    if let Some(error) = crate::provider_error(&value) {
        return Err(error);
    }
    let periods = value
        .get("properties")
        .and_then(|p| p.get("periods"))
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Missing("properties.periods".into()))?;
    let first = periods.first().ok_or_else(|| Error::Missing("periods[0]".into()))?;
    let start = first
        .get("startTime")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Missing("periods[0].startTime".into()))?;
    let (timestamp, utc_offset_seconds) = parse_iso_local(start)?;
    let temp_c = temperature(first)?;
    let icon = icon_for(first);
    crate::check_location(request.location)?;

    // Periods alternate day and night. The first two periods are "today", but
    // a request after dark starts with the night period, so the pair is chosen
    // by the day flag rather than by position.
    let (today_max, today_min) = day_range(periods);
    let forecast = daily(periods);

    Ok(CurrentWeather {
        location: request.location.to_string(),
        latitude: request.latitude,
        longitude: request.longitude,
        timestamp,
        utc_offset_seconds,
        timezone: grid.timezone.clone(),
        temp_c,
        min_c: today_min.unwrap_or(temp_c),
        max_c: today_max.unwrap_or(temp_c),
        icon,
        sunrise_min: SUN_UNKNOWN,
        sunset_min: SUN_UNKNOWN,
        fetched_at_monotonic_ms: request.fetched_at_monotonic_ms,
        forecast,
    })
}

/// Fahrenheit to Celsius. NWS returns `temperatureUnit` per period.
fn temperature(period: &Value) -> Result<f64, Error> {
    let value = period
        .get("temperature")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::Missing("periods[].temperature".into()))?;
    let unit = period
        .get("temperatureUnit")
        .and_then(Value::as_str)
        .unwrap_or("F");
    Ok(match unit {
        "C" => value,
        "F" => (value - 32.0) * 5.0 / 9.0,
        other => {
            return Err(Error::WrongType {
                field: "periods[].temperatureUnit".into(),
                expected: match other {
                    _ => "F or C",
                },
            });
        }
    })
}

fn day_range(periods: &[Value]) -> (Option<f64>, Option<f64>) {
    let mut max = None;
    let mut min = None;
    for period in periods.iter().take(2) {
        let Ok(c) = temperature(period) else { continue };
        if period.get("isDaytime").and_then(Value::as_bool).unwrap_or(false) {
            max = Some(max.map_or(c, |m: f64| m.max(c)));
        } else {
            min = Some(min.map_or(c, |m: f64| m.min(c)));
        }
    }
    (max, min)
}

fn daily(periods: &[Value]) -> Vec<ForecastDay> {
    let mut days: Vec<ForecastDay> = Vec::new();
    for period in periods {
        let Some(start) = period.get("startTime").and_then(Value::as_str) else {
            continue;
        };
        let Ok((local, _)) = parse_iso_local(start) else {
            continue;
        };
        let Ok(c) = temperature(period) else { continue };
        let date = date_of(local);
        let daytime = period.get("isDaytime").and_then(Value::as_bool).unwrap_or(false);
        let icon = icon_for(period);
        match days.iter_mut().find(|day| day.date == date) {
            Some(day) => {
                day.min_c = day.min_c.min(c);
                day.max_c = day.max_c.max(c);
                if daytime {
                    day.icon = icon;
                }
            }
            None => days.push(ForecastDay {
                date,
                min_c: c,
                max_c: c,
                icon,
            }),
        }
    }
    days.truncate(crate::FORECAST_DAYS as usize);
    days
}

/// The nine-icon vocabulary has no "unknown", so the least-wrong default is
/// used for a phrase the table does not know. Every documented NWS phrase that
/// appears in the fixture is mapped explicitly.
#[must_use]
pub fn icon_for(period: &Value) -> Icon {
    let text = period
        .get("shortForecast")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if text.contains("thunder") {
        Icon::Thunderstorm
    } else if text.contains("snow") || text.contains("flurries") || text.contains("sleet") || text.contains("ice") {
        Icon::Snow
    } else if text.contains("rain") || text.contains("showers") || text.contains("drizzle") {
        if text.contains("cloud") || text.contains("partly") {
            Icon::CloudsRain
        } else {
            Icon::Rain
        }
    } else if text.contains("fog") || text.contains("mist") || text.contains("haze") || text.contains("smoke") {
        Icon::Mist
    } else if text.contains("partly") || text.contains("few") || text.contains("mostly sunny") || text.contains("mostly clear") {
        Icon::FewClouds
    } else if text.contains("mostly cloudy") || text.contains("cloudy") || text.contains("overcast") {
        Icon::HeavyClouds
    } else if text.contains("sunny") || text.contains("clear") {
        Icon::Clear
    } else {
        Icon::Clouds
    }
}

/// `YYYY-MM-DDTHH:MM:SS±HH:MM` to (local epoch seconds, offset seconds).
fn parse_iso_local(text: &str) -> Result<(i64, i32), Error> {
    let bad = || Error::WrongType {
        field: "startTime".into(),
        expected: "an ISO 8601 timestamp with an offset",
    };
    let (date, rest) = text.split_once('T').ok_or_else(bad)?;
    let bytes = rest.as_bytes();
    if bytes.len() < 8 {
        return Err(bad());
    }
    let offset_at = rest.rfind(['+', '-']).ok_or_else(bad)?;
    let (clock, offset) = rest.split_at(offset_at);
    let mut clock = clock.split(':');
    let hour: i64 = clock.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let minute: i64 = clock.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let second: i64 = clock
        .next()
        .unwrap_or("0")
        .trim_end_matches('Z')
        .parse()
        .map_err(|_| bad())?;
    let mut day = date.split('-');
    let year: i64 = day.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let month: i64 = day.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let dom: i64 = day.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let mut parts = offset[1..].split(':');
    let oh: i32 = parts.next().unwrap_or("0").parse().map_err(|_| bad())?;
    let om: i32 = parts.next().unwrap_or("0").parse().map_err(|_| bad())?;
    let utc_offset_seconds = sign * (oh * 3600 + om * 60);
    let days = days_from_civil(year, month, dom);
    let utc = days * 86_400 + hour * 3600 + minute * 60 + second - i64::from(utc_offset_seconds);
    // The record carries local seconds, which is UTC plus the offset.
    Ok((utc + i64::from(utc_offset_seconds), utc_offset_seconds))
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn date_of(local_seconds: i64) -> String {
    let days = local_seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SampleRequest<'static> {
        SampleRequest {
            location: "Seattle",
            latitude: 47.6062,
            longitude: -122.3321,
            fetched_at_monotonic_ms: 1,
        }
    }

    const POINTS: &str = r#"{"properties":{"forecast":"https://api.weather.gov/gridpoints/SEW/125,68/forecast","timeZone":"America/Los_Angeles"}}"#;

    const FORECAST: &str = r#"{"properties":{"periods":[
        {"number":1,"startTime":"2026-11-13T10:00:00-08:00","isDaytime":true,"temperature":52,"temperatureUnit":"F","shortForecast":"Rain"},
        {"number":2,"startTime":"2026-11-13T22:00:00-08:00","isDaytime":false,"temperature":44,"temperatureUnit":"F","shortForecast":"Rain Likely"},
        {"number":3,"startTime":"2026-11-14T10:00:00-08:00","isDaytime":true,"temperature":48,"temperatureUnit":"F","shortForecast":"Partly Sunny"},
        {"number":4,"startTime":"2026-11-14T22:00:00-08:00","isDaytime":false,"temperature":40,"temperatureUnit":"F","shortForecast":"Mostly Cloudy"}
    ]}}"#;

    #[test]
    fn points_resolve_a_forecast_url_and_zone() {
        let grid = parse_points(POINTS.as_bytes()).expect("points").expect("covered");
        assert_eq!(grid.timezone, "America/Los_Angeles");
        assert!(grid.forecast_url.ends_with("/gridpoints/SEW/125,68/forecast"));
        let uncovered = br#"{"status":404,"title":"No point found"}"#;
        assert_eq!(parse_points(uncovered).expect("points"), None);
        assert!(parse_points(b"{}").is_err());
        assert_eq!(
            points_url(ORIGIN, 47.60621, -122.33207),
            "https://api.weather.gov/points/47.6062,-122.3321"
        );
    }

    #[test]
    fn a_forecast_becomes_a_record() {
        let grid = parse_points(POINTS.as_bytes()).expect("points").expect("covered");
        let weather = parse_forecast(FORECAST.as_bytes(), &request(), &grid).expect("forecast");
        // 52F is 11.1C, and the first period is the current condition.
        assert!((weather.temp_c - 11.111).abs() < 0.01, "{}", weather.temp_c);
        assert!((weather.max_c - 11.111).abs() < 0.01);
        assert!((weather.min_c - 6.666).abs() < 0.01);
        assert_eq!(weather.icon, Icon::Rain);
        assert_eq!(weather.timezone, "America/Los_Angeles");
        assert_eq!(weather.utc_offset_seconds, -8 * 3600);
        assert_eq!(weather.sunrise_min, SUN_UNKNOWN);
        assert_eq!(weather.forecast.len(), 2);
        assert_eq!(weather.forecast[0].date, "2026-11-13");
        assert_eq!(weather.forecast[1].date, "2026-11-14");
        assert_eq!(weather.forecast[1].icon, Icon::FewClouds);
    }

    #[test]
    fn local_time_and_offset_survive_the_iso_parse() {
        let (local, offset) = parse_iso_local("2026-11-13T10:00:00-08:00").expect("iso");
        assert_eq!(offset, -28_800);
        assert_eq!(date_of(local), "2026-11-13");
        // 10:00 at -08:00 is the same instant as 18:00 at +00:00.
        let (utc_local, utc_offset) = parse_iso_local("2026-11-13T18:00:00+00:00").expect("iso");
        assert_eq!(utc_offset, 0);
        assert_eq!(local - i64::from(offset), utc_local);
        assert_eq!((local - i64::from(offset)).rem_euclid(86_400), 64_800);
    }

    #[test]
    fn phrases_map_to_the_nine_icons() {
        let cases = [
            ("Thunderstorms", Icon::Thunderstorm),
            ("Snow Showers", Icon::Snow),
            ("Rain", Icon::Rain),
            ("Rain And Snow", Icon::Snow),
            ("Cloudy", Icon::HeavyClouds),
            ("Partly Cloudy", Icon::FewClouds),
            ("Sunny", Icon::Clear),
            ("Fog", Icon::Mist),
            ("Showers And Thunderstorms", Icon::Thunderstorm),
        ];
        for (text, want) in cases {
            let period = serde_json::json!({ "shortForecast": text });
            assert_eq!(icon_for(&period), want, "{text}");
        }
    }
}
