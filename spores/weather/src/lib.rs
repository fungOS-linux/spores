//! The weather Spore: "what is the weather here", as a record the watch reads.
//!
//! One provider (Open-Meteo, no key), one `weather.current` record per place,
//! and one line of output for a person. The Spore owns argument parsing, the
//! place lookup, and the record; the host owns the connection, the origin
//! allowlist, and durability. Nothing here opens a socket or a file.
//!
//! The record is the product. `pinetime weather --from-record` reads it back,
//! so the same sample reaches the watch without a second hand-typed copy of
//! the numbers.

#[cfg(target_arch = "wasm32")]
mod guest;

use serde_json::{Value, json};
use weather_protocol as weather;

/// The host capabilities this Spore needs, in plain Rust so the command logic
/// is host-testable without a wasm runtime.
pub trait Host {
    /// One bounded, allowlisted GET. The host decides which origins exist.
    fn get(&mut self, url: &str) -> Result<Vec<u8>, String>;
    /// Write the record. `id` is stable within [`weather::COLLECTION`].
    fn upsert(&mut self, id: &str, body: &[u8]) -> Result<(), String>;
    /// The host's monotonic clock, for `fetched_at_monotonic_ms`.
    fn monotonic_ms(&mut self) -> Result<u64, String>;
}

/// What one successful run produced: the line to print, and the snapshot a
/// host can show without re-reading the store.
pub struct Outcome {
    pub output: String,
    pub snapshot: Value,
}

/// Run one command and return its output.
pub fn run(args: &[String], host: &mut dyn Host) -> Result<String, String> {
    execute(args, host).map(|outcome| outcome.output)
}

pub fn execute(args: &[String], host: &mut dyn Host) -> Result<Outcome, String> {
    Command::parse(args)?.execute(host)
}

struct Command {
    place: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    label: Option<String>,
    json: bool,
    help: bool,
}

impl Command {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut command = Self {
            place: None,
            latitude: None,
            longitude: None,
            label: None,
            json: false,
            help: false,
        };
        let mut words = Vec::new();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--place" => command.place = Some(take(args, &mut index, "--place")?),
                "--lat" => {
                    let value = take(args, &mut index, "--lat")?;
                    command.latitude = Some(number(&value, "--lat")?);
                }
                "--lon" => {
                    let value = take(args, &mut index, "--lon")?;
                    command.longitude = Some(number(&value, "--lon")?);
                }
                "--label" => command.label = Some(take(args, &mut index, "--label")?),
                "--json" => command.json = true,
                "--help" | "-h" | "help" => command.help = true,
                flag if flag.starts_with("--") => return Err(format!("unknown argument {flag:?}")),
                word => words.push(word.to_string()),
            }
            index += 1;
        }
        if command.help {
            return Ok(command);
        }
        match words.as_slice() {
            [verb] if verb == "now" => Ok(command),
            [] => Err(format!(
                "weather needs a command; try `weather now --place Seattle`\n\n{HELP}"
            )),
            [verb, ..] => Err(format!("unknown command {verb:?}; expected now")),
        }
    }

    fn execute(self, host: &mut dyn Host) -> Result<Outcome, String> {
        if self.help {
            return Ok(Outcome {
                output: HELP.to_string(),
                snapshot: json!({"ok": true, "help": true}),
            });
        }
        let json = self.json;
        let (latitude, longitude, label, id_label) = self.resolve(host)?;
        if !(-90.0..=90.0).contains(&latitude) {
            return Err(format!("--lat: {latitude} is outside -90..90"));
        }
        if !(-180.0..=180.0).contains(&longitude) {
            return Err(format!("--lon: {longitude} is outside -180..180"));
        }
        weather::check_location(&label)
            .map_err(|error| format!("{error}; pass --label with a shorter name"))?;
        let id = weather::record_id(id_label.as_deref(), latitude, longitude)
            .map_err(|error| error.to_string())?;

        let url = weather::forecast_url(weather::FORECAST_ORIGIN, latitude, longitude);
        let body = fetch(host, &url, "open-meteo")?;
        // The fetch is done; this is when the sample arrived, not when it left.
        let fetched_at_monotonic_ms = host
            .monotonic_ms()
            .map_err(|error| format!("time: {error}"))?;
        let record = weather::parse_forecast(
            &body,
            &weather::SampleRequest {
                location: &label,
                latitude,
                longitude,
                fetched_at_monotonic_ms,
            },
        )
        .map_err(|error| error.to_string())?;

        let record_body = record.body();
        let bytes =
            serde_json::to_vec(&record_body).map_err(|error| format!("record JSON: {error}"))?;
        host.upsert(&id, &bytes)
            .map_err(|error| format!("records: {}/{id}: {error}", weather::COLLECTION))?;

        let output = if json {
            serde_json::to_string_pretty(&record_body)
                .map_err(|error| format!("record JSON: {error}"))?
        } else {
            summary(&record, &id)
        };
        Ok(Outcome {
            output,
            snapshot: json!({
                "ok": true,
                "collection": weather::COLLECTION,
                "schema": weather::SCHEMA,
                "id": id,
                "location": record.location,
                "timestamp": record.timestamp,
                "fetched_at_monotonic_ms": record.fetched_at_monotonic_ms,
            }),
        })
    }

    /// Turn the flags into a point and the two names a record needs: the string
    /// the watch shows, and the one the record id is derived from (`None` for a
    /// bare coordinate query, whose id is the coordinates themselves).
    fn resolve(
        self,
        host: &mut dyn Host,
    ) -> Result<(f64, f64, String, Option<String>), String> {
        match (self.place, self.latitude, self.longitude) {
            (Some(place), None, None) => {
                let url = weather::geocode_url(weather::GEOCODING_ORIGIN, &place);
                let body = fetch(host, &url, "geocoding")?;
                let found = weather::parse_geocode(&body)
                    .map_err(|error| format!("geocoding: {error}"))?
                    .ok_or_else(|| format!("geocoding: no place matched {place:?}"))?;
                let label = self.label.unwrap_or(found.name);
                Ok((found.latitude, found.longitude, label.clone(), Some(label)))
            }
            (None, Some(latitude), Some(longitude)) => {
                // No label means the record id is the point itself, not a slug
                // of one: the coordinates are already stable and readable.
                let id_label = self.label.clone();
                let label = self
                    .label
                    .unwrap_or_else(|| weather::coordinates(latitude, longitude));
                Ok((latitude, longitude, label, id_label))
            }
            (Some(_), _, _) => Err("use --place, or --lat and --lon, not both".to_string()),
            (None, None, None) => Err(
                "weather now needs a place: pass --place NAME, or --lat and --lon".to_string(),
            ),
            (None, Some(_), None) => Err("--lat needs --lon as well".to_string()),
            (None, None, Some(_)) => Err("--lon needs --lat as well".to_string()),
        }
    }
}

fn take(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn number(value: &str, flag: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|_| format!("{flag}: {value:?} is not a number"))
}

fn fetch(host: &mut dyn Host, url: &str, what: &str) -> Result<Vec<u8>, String> {
    host.get(url)
        .map_err(|error| format!("{what}: {error} ({url})"))
}

/// The one line a person reads: what it is like, and where it was written.
fn summary(record: &weather::CurrentWeather, id: &str) -> String {
    let sun = if record.sunrise_min < 0 && record.sunset_min < 0 {
        "no sunrise or sunset today".to_string()
    } else {
        format!(
            "sunrise {}, sunset {}",
            clock(record.sunrise_min),
            clock(record.sunset_min)
        )
    };
    format!(
        "{}: {:.1}\u{b0}C {}, {:.1}\u{b0}C to {:.1}\u{b0}C, {sun} [{}/{}]",
        record.location,
        record.temp_c,
        record.icon.as_str().replace('-', " "),
        record.min_c,
        record.max_c,
        weather::COLLECTION,
        id,
    )
}

fn clock(minutes: i16) -> String {
    if minutes < 0 {
        return "none".to_string();
    }
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

const HELP: &str = "\
usage: weather now --place NAME [--label LABEL] [--json]
       weather now --lat DEGREES --lon DEGREES [--label LABEL] [--json]

commands:
  now                 fetch the current weather, write the record, print a line

examples:
  weather now --place \"Seattle\"
  weather now --lat 47.6 --lon -122.3 --label \"Living room\"
  weather now --place Seattle --json";

#[cfg(test)]
mod tests;
