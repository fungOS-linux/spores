//! The `weather.current` record, and the Open-Meteo wire that fills it.
//!
//! Sources:
//! - <https://open-meteo.com/en/docs> — the `/v1/forecast` parameters
//!   (`current`, `daily`, `timezone=auto`, `utc_offset_seconds`), the response
//!   shape, and the WMO weather interpretation codes `weather_code` names.
//! - <https://open-meteo.com/en/docs/geocoding-api> — `/v1/search`, the place
//!   name to coordinates lookup.
//! - <https://github.com/HowardHinnant/date/wiki/Examples-and-Recipes> —
//!   `days_from_civil`, the calendar conversion the ISO timestamps need.
//!
//! Nothing here touches wasm or the network: request URLs, response parsing,
//! the WMO table, and the record body are host-testable with inline fixtures.

pub mod nws;

use serde_json::{json, Map, Value};

/// The records collection that holds one record per place.
pub const COLLECTION: &str = "weather.current";
/// The schema name every record in [`COLLECTION`] carries.
pub const SCHEMA: &str = "weather.current.v1";
/// The version inside the body, for a reader that has only the JSON.
pub const SCHEMA_VERSION: u64 = 1;
/// The provider this Spore reads. One provider, named rather than implied.
pub const SOURCE: &str = "open-meteo";
/// The model behind the numbers, in the same shape `pinetime-spore` stamps.
pub const MODEL_VERSION: &str = concat!("weather-spore@", env!("CARGO_PKG_VERSION"));

/// Forecast endpoint origin. The host allowlists the origin, not the path.
pub const FORECAST_ORIGIN: &str = "https://api.open-meteo.com";
/// Geocoding endpoint origin; a separate origin, so it is allowlisted separately.
pub const GEOCODING_ORIGIN: &str = "https://geocoding-api.open-meteo.com";

/// Days requested. InfiniTime's forecast frame holds five, today included.
pub const FORECAST_DAYS: u32 = 5;

/// InfiniTime's Simple Weather service has no sunrise or sunset value for a day
/// where the sun does not rise or set; Open-Meteo reports `null` there, and the
/// record carries the service's `-2` sentinel rather than a fabricated zero.
/// `infinitime::SUN_ABSENT` is the wire-side owner of this number, and a test
/// there pins the two together.
pub const SUN_ABSENT: i16 = -2;
/// The provider does not publish sunrise/sunset (NWS). The watch renders
/// "unknown", which is not the same as "does not occur".
pub const SUN_UNKNOWN: i16 = -1;

/// The longest location the watch's fixed 32-byte field can hold.
pub const LOCATION_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The provider sent a code outside the WMO table this Spore maps.
    UnknownWeatherCode(i64),
    /// A record body names an icon that is not one of the nine.
    UnknownIcon(String),
    /// A field the record or the response must carry was absent.
    Missing(String),
    /// A field was present with the wrong JSON type.
    WrongType {
        field: String,
        expected: &'static str,
    },
    /// A timestamp that is not the local ISO 8601 wall time this Spore expects.
    BadTime { field: String, value: String },
    /// A record body from a schema version this crate does not read.
    UnsupportedSchema(u64),
    /// The location will not fit the watch's field; shortening it would be a lie.
    LocationTooLong(usize),
    /// A label with no ASCII letter or digit cannot name a record.
    EmptyRecordId(String),
    /// The provider answered `{"error": true, "reason": ...}`.
    Provider(String),
    /// The provider's body was not JSON at all.
    Json(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownWeatherCode(code) => {
                write!(
                    f,
                    "weather code {code} is not in the WMO table this Spore maps"
                )
            }
            Self::UnknownIcon(name) => write!(f, "unknown weather icon {name:?}"),
            Self::Missing(field) => write!(f, "missing {field}"),
            Self::WrongType { field, expected } => write!(f, "{field} is not {expected}"),
            Self::BadTime { field, value } => {
                write!(f, "{field} {value:?} is not a local ISO 8601 timestamp")
            }
            Self::UnsupportedSchema(version) => write!(
                f,
                "{SCHEMA} version {version} is not a schema this Spore reads"
            ),
            Self::LocationTooLong(bytes) => write!(
                f,
                "location is {bytes} bytes; the watch field holds {LOCATION_BYTES}"
            ),
            Self::EmptyRecordId(label) => write!(
                f,
                "{label:?} has no ASCII letter or digit to name a record with; pass --label"
            ),
            Self::Provider(reason) => write!(f, "{SOURCE}: {reason}"),
            Self::Json(reason) => write!(f, "response is not JSON: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

/// The nine icons the InfiniTime Simple Weather service draws.
///
/// The record stores the name, not the id, so a reader can see what the watch
/// will draw without knowing the wire layout. `infinitime` owns the
/// name-to-id table; [`Icon::ALL`] is what that table must accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Clear,
    FewClouds,
    Clouds,
    HeavyClouds,
    CloudsRain,
    Rain,
    Thunderstorm,
    Snow,
    Mist,
}

impl Icon {
    pub const ALL: [Self; 9] = [
        Self::Clear,
        Self::FewClouds,
        Self::Clouds,
        Self::HeavyClouds,
        Self::CloudsRain,
        Self::Rain,
        Self::Thunderstorm,
        Self::Snow,
        Self::Mist,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::FewClouds => "few-clouds",
            Self::Clouds => "clouds",
            Self::HeavyClouds => "heavy-clouds",
            Self::CloudsRain => "clouds-rain",
            Self::Rain => "rain",
            Self::Thunderstorm => "thunderstorm",
            Self::Snow => "snow",
            Self::Mist => "mist",
        }
    }

    pub fn parse(name: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|icon| icon.as_str() == name)
            .ok_or_else(|| Error::UnknownIcon(name.to_string()))
    }
}

/// WMO weather interpretation code to the icon the watch draws.
///
/// The code table is the one Open-Meteo's `weather_code` documents: 0 clear,
/// 1-2 mainly clear and partly cloudy, 3 overcast, 45/48 fog, 51-67 drizzle
/// through freezing rain, 71-77 snowfall, 80-82 rain showers, 85-86 snow
/// showers, 95-99 thunderstorm. A code outside it is an error that names the
/// code: a default icon would be weather nobody read.
///
/// `heavy-clouds` is a valid watch icon this provider never selects; overcast
/// is `clouds`. Direct `pinetime weather --icon heavy-clouds` still works.
pub fn icon_for_code(code: i64) -> Result<Icon, Error> {
    Ok(match code {
        0 => Icon::Clear,
        1 | 2 => Icon::FewClouds,
        3 => Icon::Clouds,
        45 | 48 => Icon::Mist,
        51..=67 => Icon::Rain,
        71..=77 => Icon::Snow,
        80..=82 => Icon::CloudsRain,
        85 | 86 => Icon::Snow,
        95..=99 => Icon::Thunderstorm,
        other => return Err(Error::UnknownWeatherCode(other)),
    })
}

/// One match from the geocoding API, reduced to what this Spore uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// The place's own name, localized to `language=en`.
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub admin1: Option<String>,
    pub country: Option<String>,
}

impl Place {
    /// `Seattle, Washington, United States`, for a person to read.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts = vec![self.name.clone()];
        parts.extend(self.admin1.clone());
        parts.extend(self.country.clone());
        parts.join(", ")
    }
}

/// `/v1/search?name=…`: one best match, English names, JSON.
#[must_use]
pub fn geocode_url(origin: &str, place: &str) -> String {
    format!(
        "{origin}/v1/search?name={}&count=1&language=en&format=json",
        percent_encode(place)
    )
}

/// `/v1/forecast?…`: current conditions and five local days for one point.
///
/// `timezone=auto` resolves the point's zone and fixes `utc_offset_seconds`
/// for the whole response, which is what makes the daily arrays local days.
#[must_use]
pub fn forecast_url(origin: &str, latitude: f64, longitude: f64) -> String {
    format!(
        "{origin}/v1/forecast?latitude={latitude}&longitude={longitude}\
&current=temperature_2m,weather_code\
&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset\
&timezone=auto&forecast_days={FORECAST_DAYS}"
    )
}

/// What the caller knows before the provider answers: the place this record is
/// about, and when it was fetched.
#[derive(Debug, Clone, Copy)]
pub struct SampleRequest<'a> {
    /// The string the watch will show, already checked against [`LOCATION_BYTES`].
    pub location: &'a str,
    /// The coordinates the caller asked about, not the provider's grid cell.
    pub latitude: f64,
    pub longitude: f64,
    pub fetched_at_monotonic_ms: u64,
}

/// One day of the forecast, as the record carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastDay {
    /// The provider's local date, `YYYY-MM-DD`.
    pub date: String,
    pub min_c: f64,
    pub max_c: f64,
    pub icon: Icon,
}

/// One `weather.current.v1` body.
///
/// `source` and `model_version` are not fields here: they are constants this
/// Spore writes ([`SOURCE`], [`MODEL_VERSION`]) and never varies per sample.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentWeather {
    /// What the watch shows; at most [`LOCATION_BYTES`] bytes.
    pub location: String,
    pub latitude: f64,
    pub longitude: f64,
    /// Local seconds: the instant the numbers describe, shifted into the
    /// place's zone. This is the timestamp the watch displays, and it is the
    /// provider's local wall clock — equal to the UTC epoch plus
    /// [`Self::utc_offset_seconds`].
    pub timestamp: i64,
    pub utc_offset_seconds: i32,
    /// The provider's zone name for that offset, e.g. `America/Los_Angeles`.
    pub timezone: String,
    pub temp_c: f64,
    pub min_c: f64,
    pub max_c: f64,
    pub icon: Icon,
    /// Minutes since local midnight, or [`SUN_ABSENT`].
    pub sunrise_min: i16,
    /// Minutes since local midnight, or [`SUN_ABSENT`].
    pub sunset_min: i16,
    pub fetched_at_monotonic_ms: u64,
    /// Today plus the following days, as the provider returned them.
    pub forecast: Vec<ForecastDay>,
}

impl CurrentWeather {
    /// The record body. The key set is the schema; [`Self::from_body`] reads
    /// exactly what this writes.
    #[must_use]
    pub fn body(&self) -> Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "location": self.location,
            "latitude": self.latitude,
            "longitude": self.longitude,
            "timestamp": self.timestamp,
            "utc_offset_seconds": self.utc_offset_seconds,
            "timezone": self.timezone,
            "temp_c": self.temp_c,
            "min_c": self.min_c,
            "max_c": self.max_c,
            "icon": self.icon.as_str(),
            "sunrise_min": self.sunrise_min,
            "sunset_min": self.sunset_min,
            "source": SOURCE,
            "model_version": MODEL_VERSION,
            "fetched_at_monotonic_ms": self.fetched_at_monotonic_ms,
            "forecast": self.forecast.iter().map(day_body).collect::<Vec<_>>(),
        })
    }

    /// Read a body another writer (or a later one) stored.
    pub fn from_body(body: &[u8]) -> Result<Self, Error> {
        let value: Value =
            serde_json::from_slice(body).map_err(|error| Error::Json(error.to_string()))?;
        let body = value.as_object().ok_or(Error::WrongType {
            field: "body".into(),
            expected: "an object",
        })?;

        let version = whole(body, "schema_version", "schema_version")?;
        let version = u64::try_from(version).unwrap_or(u64::MAX);
        if version != SCHEMA_VERSION {
            return Err(Error::UnsupportedSchema(version));
        }
        let location = text(body, "location", "location")?.to_string();
        check_location(&location)?;
        let forecast = match body.get("forecast") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(days)) => days
                .iter()
                .enumerate()
                .map(|(index, day)| day_from_body(day, index))
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(Error::WrongType {
                    field: "forecast".into(),
                    expected: "an array",
                })
            }
        };
        let offset = whole(body, "utc_offset_seconds", "utc_offset_seconds")?;
        Ok(Self {
            location,
            latitude: number(body, "latitude", "latitude")?,
            longitude: number(body, "longitude", "longitude")?,
            timestamp: whole(body, "timestamp", "timestamp")?,
            utc_offset_seconds: i32::try_from(offset).map_err(|_| Error::WrongType {
                field: "utc_offset_seconds".into(),
                expected: "a 32-bit offset in seconds",
            })?,
            timezone: text(body, "timezone", "timezone")?.to_string(),
            temp_c: number(body, "temp_c", "temp_c")?,
            min_c: number(body, "min_c", "min_c")?,
            max_c: number(body, "max_c", "max_c")?,
            icon: Icon::parse(text(body, "icon", "icon")?)?,
            sunrise_min: minutes(body, "sunrise_min", "sunrise_min")?,
            sunset_min: minutes(body, "sunset_min", "sunset_min")?,
            fetched_at_monotonic_ms: u64::try_from(whole(
                body,
                "fetched_at_monotonic_ms",
                "fetched_at_monotonic_ms",
            )?)
            .map_err(|_| Error::WrongType {
                field: "fetched_at_monotonic_ms".into(),
                expected: "a positive number of milliseconds",
            })?,
            forecast,
        })
    }
}

fn day_body(day: &ForecastDay) -> Value {
    json!({
        "date": day.date,
        "min_c": day.min_c,
        "max_c": day.max_c,
        "icon": day.icon.as_str(),
    })
}

fn day_from_body(day: &Value, index: usize) -> Result<ForecastDay, Error> {
    let day = day.as_object().ok_or(Error::WrongType {
        field: format!("forecast[{index}]"),
        expected: "an object",
    })?;
    let prefix = format!("forecast[{index}]");
    Ok(ForecastDay {
        date: text(day, "date", &format!("{prefix}.date"))?.to_string(),
        min_c: number(day, "min_c", &format!("{prefix}.min_c"))?,
        max_c: number(day, "max_c", &format!("{prefix}.max_c"))?,
        icon: Icon::parse(text(day, "icon", &format!("{prefix}.icon"))?)?,
    })
}

/// A record id derived from the place: the label's slug, or the coordinates.
///
/// The id is a key, so it must be stable across runs for the same place and
/// different for different places. A label that reduces to nothing is refused
/// rather than replaced with a coordinate key the caller did not choose.
pub fn record_id(label: Option<&str>, latitude: f64, longitude: f64) -> Result<String, Error> {
    match label {
        Some(label) => slug(label).ok_or_else(|| Error::EmptyRecordId(label.to_string())),
        None => Ok(coordinates(latitude, longitude)),
    }
}

/// `47.6062,-122.3321`: four decimals is about 11 m, enough to tell places
/// apart and coarse enough that two geocodes of one point agree.
#[must_use]
pub fn coordinates(latitude: f64, longitude: f64) -> String {
    format!("{latitude:.4},{longitude:.4}")
}

/// Lowercase ASCII letters and digits, every other run collapsed to one `-`.
#[must_use]
pub fn slug(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut pending_dash = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(character.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Refuse a label the watch would have to truncate.
pub fn check_location(location: &str) -> Result<(), Error> {
    let bytes = location.len();
    if bytes > LOCATION_BYTES {
        return Err(Error::LocationTooLong(bytes));
    }
    Ok(())
}

/// Turn the `/v1/forecast` body into the record the watch will read.
pub fn parse_forecast(body: &[u8], request: &SampleRequest<'_>) -> Result<CurrentWeather, Error> {
    let value: Value =
        serde_json::from_slice(body).map_err(|error| Error::Json(error.to_string()))?;
    if let Some(error) = provider_error(&value) {
        return Err(error);
    }
    check_location(request.location)?;
    let body = value.as_object().ok_or(Error::WrongType {
        field: "body".into(),
        expected: "an object",
    })?;

    let offset = whole(body, "utc_offset_seconds", "utc_offset_seconds")?;
    let utc_offset_seconds = i32::try_from(offset).map_err(|_| Error::WrongType {
        field: "utc_offset_seconds".into(),
        expected: "a 32-bit offset in seconds",
    })?;
    let timezone = text(body, "timezone", "timezone")?.to_string();

    let current = object(body, "current", "current")?;
    let timestamp = local_seconds(text(current, "time", "current.time")?, "current.time")?;
    let temp_c = number(current, "temperature_2m", "current.temperature_2m")?;
    let current_code = whole(current, "weather_code", "current.weather_code")?;

    let daily = object(body, "daily", "daily")?;
    let dates = array(daily, "time", "daily.time")?;
    let codes = array(daily, "weather_code", "daily.weather_code")?;
    let maxima = array(daily, "temperature_2m_max", "daily.temperature_2m_max")?;
    let minima = array(daily, "temperature_2m_min", "daily.temperature_2m_min")?;
    let sunrises = array(daily, "sunrise", "daily.sunrise")?;
    let sunsets = array(daily, "sunset", "daily.sunset")?;

    let mut forecast = Vec::with_capacity(dates.len());
    for index in 0..dates.len() {
        forecast.push(ForecastDay {
            date: local_date(element(dates, index, "daily.time")?, index)?,
            min_c: element_number(minima, index, "daily.temperature_2m_min")?,
            max_c: element_number(maxima, index, "daily.temperature_2m_max")?,
            icon: icon_for_code(element_whole(codes, index, "daily.weather_code")?)?,
        });
    }
    let today = forecast
        .first()
        .ok_or_else(|| Error::Missing("daily.time[0]".into()))?;
    let (min_c, max_c) = (today.min_c, today.max_c);

    Ok(CurrentWeather {
        location: request.location.to_string(),
        latitude: request.latitude,
        longitude: request.longitude,
        timestamp,
        utc_offset_seconds,
        timezone,
        temp_c,
        min_c,
        max_c,
        icon: icon_for_code(current_code)?,
        sunrise_min: sun_minute(element(sunrises, 0, "daily.sunrise")?, "daily.sunrise[0]")?,
        sunset_min: sun_minute(element(sunsets, 0, "daily.sunset")?, "daily.sunset[0]")?,
        fetched_at_monotonic_ms: request.fetched_at_monotonic_ms,
        forecast,
    })
}

/// Parse the geocoding response. `Ok(None)` is the documented no-match body
/// (`{}`); the caller names the query it failed to resolve.
pub fn parse_geocode(body: &[u8]) -> Result<Option<Place>, Error> {
    let value: Value =
        serde_json::from_slice(body).map_err(|error| Error::Json(error.to_string()))?;
    if let Some(error) = provider_error(&value) {
        return Err(error);
    }
    let results = match value.get("results") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(results)) if results.is_empty() => return Ok(None),
        Some(Value::Array(results)) => results,
        Some(_) => {
            return Err(Error::WrongType {
                field: "results".into(),
                expected: "an array",
            })
        }
    };
    let first = results[0].as_object().ok_or(Error::WrongType {
        field: "results[0]".into(),
        expected: "an object",
    })?;
    Ok(Some(Place {
        name: text(first, "name", "results[0].name")?.to_string(),
        latitude: number(first, "latitude", "results[0].latitude")?,
        longitude: number(first, "longitude", "results[0].longitude")?,
        admin1: optional_text(first, "admin1"),
        country: optional_text(first, "country"),
    }))
}

/// Seconds since the epoch for an ISO 8601 local timestamp.
///
/// Open-Meteo returns `time` values as local wall time in the requested zone
/// (`timezone=auto`), without an offset suffix. The seconds of that wall clock
/// are the local timestamp the watch displays, and equal the UTC epoch plus
/// `utc_offset_seconds`. An offset suffix is refused rather than guessed at.
pub fn local_seconds(value: &str, field: &str) -> Result<i64, Error> {
    let bad = || Error::BadTime {
        field: field.to_string(),
        value: value.to_string(),
    };
    let (date, clock) = value.split_once('T').ok_or_else(bad)?;
    let mut date = date.split('-');
    let year: i64 = date.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let month: i64 = date.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let day: i64 = date.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(bad());
    }
    let mut clock = clock.split(':');
    let hour: i64 = clock.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let minute: i64 = clock.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let second: i64 = match clock.next() {
        Some(second) => second.parse().map_err(|_| bad())?,
        None => 0,
    };
    if clock.next().is_some()
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return Err(bad());
    }
    Ok(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// The provider's local date, `YYYY-MM-DD`, from `daily.time[i]`.
fn local_date(value: &Value, index: usize) -> Result<String, Error> {
    let text = value.as_str().ok_or_else(|| Error::WrongType {
        field: format!("daily.time[{index}]"),
        expected: "an ISO 8601 date",
    })?;
    let date = text.split_once('T').map_or(text, |(date, _)| date);
    let shaped = date.len() == 10 && date.as_bytes().get(4) == Some(&b'-');
    if !shaped {
        return Err(Error::BadTime {
            field: format!("daily.time[{index}]"),
            value: text.to_string(),
        });
    }
    Ok(date.to_string())
}

/// Minutes since local midnight for a sun event, or [`SUN_ABSENT`] when the
/// provider reports `null` (a polar day or night).
fn sun_minute(value: &Value, field: &str) -> Result<i16, Error> {
    match value {
        Value::Null => Ok(SUN_ABSENT),
        Value::String(text) => {
            let seconds = local_seconds(text, field)?;
            Ok(((seconds.rem_euclid(86_400)) / 60) as i16)
        }
        _ => Err(Error::WrongType {
            field: field.to_string(),
            expected: "an ISO 8601 string or null",
        }),
    }
}

/// Howard Hinnant's `days_from_civil`: (year, month, day) to days since
/// 1970-01-01. The inverse of the `civil_from_days` the InfiniTime wire uses.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// RFC 3986 query-value escaping: unreserved characters pass, everything else
/// is `%XX` over UTF-8 bytes.
#[must_use]
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn provider_error(value: &Value) -> Option<Error> {
    (value.get("error").and_then(Value::as_bool) == Some(true)).then(|| {
        Error::Provider(
            value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("no reason")
                .to_string(),
        )
    })
}

fn member<'a>(map: &'a Map<String, Value>, key: &str, field: &str) -> Result<&'a Value, Error> {
    map.get(key)
        .ok_or_else(|| Error::Missing(field.to_string()))
}

fn object<'a>(
    map: &'a Map<String, Value>,
    key: &str,
    field: &str,
) -> Result<&'a Map<String, Value>, Error> {
    member(map, key, field)?
        .as_object()
        .ok_or_else(|| Error::WrongType {
            field: field.to_string(),
            expected: "an object",
        })
}

fn array<'a>(map: &'a Map<String, Value>, key: &str, field: &str) -> Result<&'a Vec<Value>, Error> {
    member(map, key, field)?
        .as_array()
        .ok_or_else(|| Error::WrongType {
            field: field.to_string(),
            expected: "an array",
        })
}

fn text<'a>(map: &'a Map<String, Value>, key: &str, field: &str) -> Result<&'a str, Error> {
    member(map, key, field)?
        .as_str()
        .ok_or_else(|| Error::WrongType {
            field: field.to_string(),
            expected: "a string",
        })
}

fn optional_text(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_string)
}

fn number(map: &Map<String, Value>, key: &str, field: &str) -> Result<f64, Error> {
    member(map, key, field)?
        .as_f64()
        .ok_or_else(|| Error::WrongType {
            field: field.to_string(),
            expected: "a number",
        })
}

/// An integer-valued JSON number. Open-Meteo sends whole numbers as `3`, but a
/// reader that insists on `as_i64` would reject `3.0` from a writer that chose
/// floats, so both forms are accepted and anything fractional is not.
fn whole(map: &Map<String, Value>, key: &str, field: &str) -> Result<i64, Error> {
    let value = number(map, key, field)?;
    if value.fract() != 0.0 || value.abs() > 9_007_199_254_740_992.0 {
        return Err(Error::WrongType {
            field: field.to_string(),
            expected: "a whole number",
        });
    }
    Ok(value as i64)
}

fn minutes(map: &Map<String, Value>, key: &str, field: &str) -> Result<i16, Error> {
    let minutes = whole(map, key, field)?;
    i16::try_from(minutes).map_err(|_| Error::WrongType {
        field: field.to_string(),
        expected: "minutes since midnight or a negative sentinel",
    })
}

fn element<'a>(values: &'a [Value], index: usize, field: &str) -> Result<&'a Value, Error> {
    values
        .get(index)
        .ok_or_else(|| Error::Missing(format!("{field}[{index}]")))
}

fn element_number(values: &[Value], index: usize, field: &str) -> Result<f64, Error> {
    element(values, index, field)?
        .as_f64()
        .ok_or_else(|| Error::WrongType {
            field: format!("{field}[{index}]"),
            expected: "a number",
        })
}

fn element_whole(values: &[Value], index: usize, field: &str) -> Result<i64, Error> {
    let value = element_number(values, index, field)?;
    if value.fract() != 0.0 {
        return Err(Error::WrongType {
            field: format!("{field}[{index}]"),
            expected: "a whole number",
        });
    }
    Ok(value as i64)
}

#[cfg(test)]
mod tests;
