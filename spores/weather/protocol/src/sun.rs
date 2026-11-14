//! Sunrise and sunset from the date and the coordinates — astronomy, not a
//! forecast. This is what fills the gap a provider leaves: NWS publishes no sun
//! times, and the watch shows them.
//!
//! Source: NOAA Global Monitoring Laboratory, "General Solar Position
//! Calculations" (<https://gml.noaa.gov/grad/solcalc/solareqns.PDF>). The
//! equations are good to about a minute, which is finer than the watch renders.

use crate::{SUN_ABSENT};

/// Solar zenith at sunrise/sunset: the sun's centre 0.833° below the horizon,
/// which accounts for refraction and the solar disc.
const ZENITH_DEGREES: f64 = 90.833;

/// Local minutes since midnight, or [`SUN_ABSENT`] when the sun does not cross
/// the horizon that day (polar day or night).
#[must_use]
pub fn sunrise_sunset(
    latitude: f64,
    longitude: f64,
    unix_seconds: i64,
    utc_offset_seconds: i32,
) -> (i16, i16) {
    let days = unix_seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let n = day_of_year(year, month, day) as f64;
    let gamma = 2.0 * std::f64::consts::PI / 365.0 * (n - 1.0);
    let eqtime = 229.18
        * (0.000_075 + 0.001_868 * gamma.cos()
            - 0.032_077 * gamma.sin()
            - 0.014_615 * (2.0 * gamma).cos()
            - 0.040_849 * (2.0 * gamma).sin());
    let decl = 0.006_918 - 0.399_912 * gamma.cos() + 0.070_257 * gamma.sin()
        - 0.006_758 * (2.0 * gamma).cos()
        + 0.000_907 * (2.0 * gamma).sin()
        - 0.002_697 * (3.0 * gamma).cos()
        + 0.001_480 * (3.0 * gamma).sin();
    let lat = latitude.to_radians();
    let cos_hour_angle = (ZENITH_DEGREES.to_radians().cos() / (lat.cos() * decl.cos()))
        - lat.tan() * decl.tan();
    if !(-1.0..=1.0).contains(&cos_hour_angle) {
        return (SUN_ABSENT, SUN_ABSENT);
    }
    let hour_angle = cos_hour_angle.acos().to_degrees();
    let offset_minutes = f64::from(utc_offset_seconds) / 60.0;
    // NOAA: minutes are UTC; the longitude is east-positive, and the caller
    // keeps the sign convention it already uses for the record.
    let sunrise_utc = 720.0 - 4.0 * (longitude + hour_angle) - eqtime;
    let sunset_utc = 720.0 - 4.0 * (longitude - hour_angle) - eqtime;
    (
        to_local_minute(sunrise_utc, offset_minutes),
        to_local_minute(sunset_utc, offset_minutes),
    )
}

/// A UTC minute-of-day folded into local minutes.
///
/// The raw value can be negative or past midnight — a sunset at 26:12 UTC is
/// 18:12 in Seattle — so the fold is the point, not an error. `SUN_ABSENT` is
/// returned only by the polar case above, never by ordinary wrapping.
fn to_local_minute(utc_minutes: f64, offset_minutes: f64) -> i16 {
    (utc_minutes + offset_minutes).round().rem_euclid(1440.0) as i16
}

fn day_of_year(year: i64, month: u32, day: u32) -> u32 {
    const CUMULATIVE: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let mut doy = CUMULATIVE[(month - 1) as usize] + day;
    if leap && month > 2 {
        doy += 1;
    }
    doy
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

    /// 2026-03-20T00:00:00Z, the March equinox.
    const EQUINOX: i64 = 1_774_051_200;

    #[test]
    fn the_equator_sees_twelve_hour_days_at_the_equinox() {
        let (sunrise, sunset) = sunrise_sunset(0.0, 0.0, EQUINOX, 0);
        // 06:00 and 18:00, within the equation-of-time wobble.
        assert!((350..=370).contains(&sunrise), "sunrise {sunrise}");
        assert!((1070..=1100).contains(&sunset), "sunset {sunset}");
    }

    #[test]
    fn the_offset_moves_the_clock_not_the_day_length() {
        let (rise_utc, set_utc) = sunrise_sunset(47.6062, -122.3321, EQUINOX, 0);
        let (rise_local, set_local) = sunrise_sunset(47.6062, -122.3321, EQUINOX, -8 * 3600);
        let day = |rise: i16, set: i16| (i32::from(set) - i32::from(rise)).rem_euclid(1440);
        assert_eq!(day(rise_utc, set_utc), day(rise_local, set_local));
        assert!(set_local > rise_local);
        // Seattle in March: about 12 hours of daylight.
        assert!((660..=780).contains(&day(rise_local, set_local)));
    }

    #[test]
    fn a_polar_winter_has_no_sunrise_rather_than_midnight() {
        // Tromsø, 69.65 N, in December: the sun never rises.
        let december = 1_797_000_000; // 2026-12-13
        let (sunrise, sunset) = sunrise_sunset(69.6492, 18.9553, december, 3600);
        assert_eq!(sunrise, SUN_ABSENT);
        assert_eq!(sunset, SUN_ABSENT);
    }

    #[test]
    fn seattle_in_november_rises_late_and_sets_before_five() {
        let november = 1_763_056_800; // 2025-11-13T18:00Z
        let (sunrise, sunset) = sunrise_sunset(47.6062, -122.3321, november, -8 * 3600);
        assert!((400..=470).contains(&sunrise), "sunrise {sunrise}");
        assert!((960..=1030).contains(&sunset), "sunset {sunset}");
    }
}
