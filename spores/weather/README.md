# Weather Spore

Answers one question — *what is the weather here* — and writes the answer where
other Spores can read it: a `weather.current` record with a local timestamp,
today's high and low, the icon the watch should draw, and the next five days.

The provider is [Open-Meteo](https://open-meteo.com): free, no API key, so this
Spore holds no credential and the host needs no credential store. The host owns
the connection and its origin allowlist; the Spore owns the place lookup, the
WMO mapping, and the record.

## Commands

```text
weather now --place "Seattle"   [--label LABEL] [--json]
weather now --lat 47.6 --lon -122.3 [--label "Living room"] [--json]
```

- `now --place NAME` geocodes the name (first match, English names), fetches the
  forecast for that point, writes the record, and prints one line.
- `now --lat --lon` skips geocoding. `--label` sets what the watch shows and what
  the record id is derived from; without it the id is the point, `47.6000,-122.3000`.
- `--json` prints the record body instead of the summary line. The record is
  written either way.

Output, against the live service:

```text
Seattle: 12.8°C rain, 8.1°C to 14.2°C, sunrise 07:12, sunset 16:38 [weather.current/seattle]
```

In a polar winter the sun line becomes `no sunrise or sunset today`; the record
carries the watch's `-2` sentinel, not a zero.

## The record

Collection `weather.current`, schema `weather.current.v1`, one record per place
(id is the label's slug, or `lat,lon`).

| Field | Meaning |
|---|---|
| `schema_version` | `1` |
| `location` | what the watch shows, at most 32 bytes |
| `latitude`, `longitude` | the point the record describes (the request, not the grid cell) |
| `timestamp` | unix seconds shifted into the place's zone: the instant the numbers describe, as the watch displays it |
| `utc_offset_seconds` | the provider's offset for that zone, resolved at request time |
| `timezone` | the zone name it came from, e.g. `America/Los_Angeles` |
| `temp_c`, `min_c`, `max_c` | current temperature and today's low/high, °C |
| `icon` | one of the nine InfiniTime icon names |
| `sunrise_min`, `sunset_min` | minutes since local midnight, or `-2` when the sun does not rise or set |
| `source`, `model_version` | `open-meteo`, `weather-spore@0.1.0` |
| `fetched_at_monotonic_ms` | when the host handed the response over |
| `forecast` | one entry per day: `date`, `min_c`, `max_c`, `icon` |

`timestamp` is `utc_epoch + utc_offset_seconds`. Open-Meteo returns `current.time`
as local wall time in the requested zone, so the wall clock read as seconds is
already that value; a test proves the identity from both forms of the same
instant.

## WMO weather codes

`weather_code` is a WMO WW number. The mapping is the table Open-Meteo
publishes:

| Code | Icon |
|---|---|
| 0 | `clear` |
| 1-2 | `few-clouds` |
| 3 | `clouds` |
| 45, 48 | `mist` |
| 51-67 | `rain` |
| 71-77 | `snow` |
| 80-82 | `clouds-rain` (showers) |
| 85, 86 | `snow` (snow showers) |
| 95-99 | `thunderstorm` |

Anything else is an error that names the code; there is no default icon.
`heavy-clouds` is a real watch icon this provider never selects — overcast is
`clouds` — and stays available to `pinetime weather --icon heavy-clouds`.

## Failures name what is missing

- `weather now needs a place: pass --place NAME, or --lat and --lon`
- `--lat needs --lon as well`
- `geocoding: no place matched "Seattl"`
- `location is 40 bytes; the watch field holds 32; pass --label with a shorter name`
- `weather code 46 is not in the WMO table this Spore maps`
- `missing daily.sunrise`
- `open-meteo: denied: the host's origin allowlist does not include this URL (https://api.open-meteo.com/v1/forecast?…)`

Nothing is invented to fill a gap: no zero temperature, no zero sunrise, no
default icon, no guessed unit.

## Companion: the watch

`pinetime weather --from-record [--id ID]` reads the newest `weather.current.v1`
record and writes the same Simple Weather frames the flag path writes. The
record struct and the schema live in `protocol/`, and the PineTime Spore depends
on that crate, so there is one definition of the body rather than two.

## Grants

| Grant | Why |
|---|---|
| `soil:http` | read the forecast and the place lookup from the two Open-Meteo origins the host allowlists |
| `soil:records` | write the `weather.current.v1` record |
| `soil:time` | stamp `fetched_at_monotonic_ms`; no wall clock |
| `wasi:cli`, `wasi:clocks`, `wasi:io` | Rust std runtime on `wasm32-wasip2`; no file or socket I/O |

No socket, process, secret, environment, raw filesystem, or wall-clock grant.

## Build

```sh
cargo test                              # protocol + command logic, offline
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/weather_spore.wasm module.wasm
soil-spore lock . && soil-spore check .
```

Tests use inline fixtures and a fake host; nothing in them opens a socket.

## Host requirement

The module imports `soil:http`, `soil:records`, and `soil:time`. `soil-dev-host`
composes them, and it takes `--allow-origin` once per origin:

```sh
soil-dev-host --allow-origin https://api.open-meteo.com \
              --allow-origin https://geocoding-api.open-meteo.com \
              module.wasm now --place "Seattle"
```
