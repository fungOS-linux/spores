//! PineTime / InfiniTime wire formats.
//!
//! This crate is deliberately free of wasm imports: the protocol is testable
//! on the host, and the Spore is a thin BLE + Records wrapper around it.
//!
//! Sources:
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/doc/ble.md>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/doc/MotionService.md>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/doc/NavigationService.md>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/doc/SimpleWeatherService.md>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/src/components/ble/HeartRateService.cpp>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/src/components/ble/AlertNotificationService.cpp>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/src/components/ble/MusicService.h>

pub mod dfu;

use std::fmt;

pub const DIS_SERIAL: &str = "00002a25-0000-1000-8000-00805f9b34fb";
pub const DIS_MODEL: &str = "00002a24-0000-1000-8000-00805f9b34fb";
pub const DIS_FIRMWARE: &str = "00002a26-0000-1000-8000-00805f9b34fb";
pub const HEART_RATE_MEASUREMENT: &str = "00002a37-0000-1000-8000-00805f9b34fb";
pub const BATTERY_LEVEL: &str = "00002a19-0000-1000-8000-00805f9b34fb";
pub const NEW_ALERT: &str = "00002a46-0000-1000-8000-00805f9b34fb";
pub const ALERT_LEVEL: &str = "00002a06-0000-1000-8000-00805f9b34fb";
pub const CURRENT_TIME: &str = "00002a2b-0000-1000-8000-00805f9b34fb";
pub const NOTIFICATION_EVENT: &str = "00020001-78fc-48fe-8e23-433b3a1942d0";
pub const MOTION_SERVICE: &str = "00030000-78fc-48fe-8e23-433b3a1942d0";
pub const STEP_COUNT: &str = "00030001-78fc-48fe-8e23-433b3a1942d0";
pub const MOTION_VALUES: &str = "00030002-78fc-48fe-8e23-433b3a1942d0";
pub const MUSIC_SERVICE: &str = "00000000-78fc-48fe-8e23-433b3a1942d0";
pub const MUSIC_EVENT: &str = "00000001-78fc-48fe-8e23-433b3a1942d0";
pub const MUSIC_STATUS: &str = "00000002-78fc-48fe-8e23-433b3a1942d0";
pub const MUSIC_ARTIST: &str = "00000003-78fc-48fe-8e23-433b3a1942d0";
pub const MUSIC_TRACK: &str = "00000004-78fc-48fe-8e23-433b3a1942d0";
pub const MUSIC_ALBUM: &str = "00000005-78fc-48fe-8e23-433b3a1942d0";

/// Navigation service (InfiniTime 0.11+). The watch only knows the icon names
/// `pure-maps` emits, so the name is validated before it is written.
pub const NAVIGATION_SERVICE: &str = "00010000-78fc-48fe-8e23-433b3a1942d0";
pub const NAVIGATION_FLAGS: &str = "00010001-78fc-48fe-8e23-433b3a1942d0";
pub const NAVIGATION_NARRATIVE: &str = "00010002-78fc-48fe-8e23-433b3a1942d0";
pub const NAVIGATION_MAN_DIST: &str = "00010003-78fc-48fe-8e23-433b3a1942d0";
pub const NAVIGATION_PROGRESS: &str = "00010004-78fc-48fe-8e23-433b3a1942d0";

/// Simple Weather service (InfiniTime 1.14+); replaces the 1.8 weather service.
pub const WEATHER_SERVICE: &str = "00050000-78fc-48fe-8e23-433b3a1942d0";
pub const WEATHER_DATA: &str = "00050001-78fc-48fe-8e23-433b3a1942d0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    TooShort { expected: usize, actual: usize },
    BadFlags(u8),
    UnknownMusicEvent(u8),
    UnknownCallResponse(u8),
    MessageTooLong(usize),
    UnknownIcon(String),
    BadDate(String),
    UnknownAlertCategory(String),
    UnknownWeatherIcon(String),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { expected, actual } => {
                write!(f, "need {expected} bytes, got {actual}")
            }
            Self::BadFlags(flags) => write!(f, "unsupported HRS flags 0x{flags:02x}"),
            Self::UnknownMusicEvent(event) => write!(f, "unknown music event 0x{event:02x}"),
            Self::UnknownCallResponse(response) => {
                write!(f, "unknown call response 0x{response:02x}")
            }
            Self::MessageTooLong(size) => write!(f, "notification is {size} bytes; max is 512"),
            Self::UnknownIcon(name) => write!(f, "unknown navigation icon {name:?}"),
            Self::BadDate(reason) => write!(f, "bad date: {reason}"),
            Self::UnknownAlertCategory(value) => write!(f, "unknown alert category {value:?}"),
            Self::UnknownWeatherIcon(name) => {
                write!(f, "unknown weather icon {name:?}")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

/// One HRS measurement. `bpm = None` means the sensor reported no measurement;
/// it must not be turned into the number zero downstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartRate {
    pub bpm: Option<u16>,
    pub contact: bool,
    pub energy_expended: Option<u16>,
    pub rr_intervals: Vec<u16>,
}

/// Parse the standard Heart Rate Measurement characteristic. InfiniTime sends
/// two bytes (flags 0, uint8 bpm); standard 16-bit and RR forms are accepted
/// because the same Spore may meet a different firmware.
pub fn parse_heart_rate(data: &[u8]) -> Result<HeartRate, ProtocolError> {
    if data.len() < 2 {
        return Err(ProtocolError::TooShort {
            expected: 2,
            actual: data.len(),
        });
    }
    let flags = data[0];
    let mut offset = 1;
    let bpm = if flags & 0x01 == 0 {
        let value = u16::from(data[1]);
        offset += 1;
        value
    } else {
        if data.len() < 3 {
            return Err(ProtocolError::TooShort {
                expected: 3,
                actual: data.len(),
            });
        }
        let value = u16::from_le_bytes([data[1], data[2]]);
        offset += 2;
        value
    };

    let contact = flags & 0x04 != 0 && flags & 0x02 != 0;
    let energy_expended = if flags & 0x08 != 0 {
        if data.len() < offset + 2 {
            return Err(ProtocolError::TooShort {
                expected: offset + 2,
                actual: data.len(),
            });
        }
        let value = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        Some(value)
    } else {
        None
    };

    let mut rr_intervals = Vec::new();
    if flags & 0x10 != 0 {
        while offset + 2 <= data.len() {
            rr_intervals.push(u16::from_le_bytes([data[offset], data[offset + 1]]));
            offset += 2;
        }
        if offset != data.len() {
            return Err(ProtocolError::BadFlags(flags));
        }
    }

    Ok(HeartRate {
        bpm: (bpm != 0).then_some(bpm),
        contact,
        energy_expended,
        rr_intervals,
    })
}

pub fn parse_battery(data: &[u8]) -> Result<u8, ProtocolError> {
    data.first().copied().ok_or(ProtocolError::TooShort {
        expected: 1,
        actual: data.len(),
    })
}

/// InfiniTime motion service: uint32 little-endian step count.
pub fn parse_steps(data: &[u8]) -> Result<u32, ProtocolError> {
    let bytes: [u8; 4] = data
        .get(..4)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(ProtocolError::TooShort {
            expected: 4,
            actual: data.len(),
        })?;
    Ok(u32::from_le_bytes(bytes))
}

/// InfiniTime motion values: three int16 little-endian axes in binary milli-g.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Motion {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

pub fn parse_motion(data: &[u8]) -> Result<Motion, ProtocolError> {
    let bytes: [u8; 6] = data
        .get(..6)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(ProtocolError::TooShort {
            expected: 6,
            actual: data.len(),
        })?;
    Ok(Motion {
        x: i16::from_le_bytes([bytes[0], bytes[1]]),
        y: i16::from_le_bytes([bytes[2], bytes[3]]),
        z: i16::from_le_bytes([bytes[4], bytes[5]]),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicEvent {
    Open,
    Play,
    Pause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
}

impl MusicEvent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Play => "play",
            Self::Pause => "pause",
            Self::Next => "next",
            Self::Previous => "previous",
            Self::VolumeUp => "volume-up",
            Self::VolumeDown => "volume-down",
        }
    }
}

pub fn parse_music_event(byte: u8) -> Result<MusicEvent, ProtocolError> {
    match byte {
        0xe0 => Ok(MusicEvent::Open),
        0x00 => Ok(MusicEvent::Play),
        0x01 => Ok(MusicEvent::Pause),
        0x03 => Ok(MusicEvent::Next),
        0x04 => Ok(MusicEvent::Previous),
        0x05 => Ok(MusicEvent::VolumeUp),
        0x06 => Ok(MusicEvent::VolumeDown),
        other => Err(ProtocolError::UnknownMusicEvent(other)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallResponse {
    Declined,
    Accepted,
    Muted,
}

impl CallResponse {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declined => "declined",
            Self::Accepted => "accepted",
            Self::Muted => "muted",
        }
    }
}

pub fn parse_call_response(byte: u8) -> Result<CallResponse, ProtocolError> {
    match byte {
        0 => Ok(CallResponse::Declined),
        1 => Ok(CallResponse::Accepted),
        2 => Ok(CallResponse::Muted),
        other => Err(ProtocolError::UnknownCallResponse(other)),
    }
}

/// ANS New Alert: `<category><amount>\x00<\x00-separated data>`.
/// The third byte is the null separator before the title.
#[must_use]
pub fn notification_packet(category: u8, title: &str, body: &str) -> Vec<u8> {
    let mut out = vec![category, 1, 0];
    out.extend_from_slice(title.as_bytes());
    out.push(0);
    out.extend_from_slice(body.as_bytes());
    out
}

pub fn ensure_notification_packet(data: &[u8]) -> Result<(), ProtocolError> {
    if data.len() > 512 {
        return Err(ProtocolError::MessageTooLong(data.len()));
    }
    Ok(())
}

#[must_use]
pub const fn music_status(playing: bool) -> u8 {
    if playing {
        1
    } else {
        0
    }
}

/// Alert Notification Service categories. The watch colours and groups a
/// notification by this byte; 0xff is `All alerts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertCategory {
    Simple,
    Email,
    News,
    Call,
    MissedCall,
    Sms,
    Voicemail,
    Schedule,
    HighPriority,
    InstantMessage,
    All,
}

impl AlertCategory {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Simple => 0,
            Self::Email => 1,
            Self::News => 2,
            Self::Call => 3,
            Self::MissedCall => 4,
            Self::Sms => 5,
            Self::Voicemail => 6,
            Self::Schedule => 7,
            Self::HighPriority => 8,
            Self::InstantMessage => 9,
            Self::All => 0xff,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Email => "email",
            Self::News => "news",
            Self::Call => "call",
            Self::MissedCall => "missed-call",
            Self::Sms => "sms",
            Self::Voicemail => "voicemail",
            Self::Schedule => "schedule",
            Self::HighPriority => "high-priority",
            Self::InstantMessage => "instant-message",
            Self::All => "all",
        }
    }

    /// A name from the spec's table, or the raw byte the bridge would carry.
    pub fn parse(value: &str) -> Result<Self, ProtocolError> {
        if let Ok(number) = value.parse::<u8>() {
            return Self::from_u8(number)
                .ok_or_else(|| ProtocolError::UnknownAlertCategory(value.to_string()));
        }
        let wanted = value.to_ascii_lowercase().replace('_', "-");
        [
            Self::Simple,
            Self::Email,
            Self::News,
            Self::Call,
            Self::MissedCall,
            Self::Sms,
            Self::Voicemail,
            Self::Schedule,
            Self::HighPriority,
            Self::InstantMessage,
            Self::All,
        ]
        .into_iter()
        .find(|category| category.as_str() == wanted)
        .ok_or_else(|| ProtocolError::UnknownAlertCategory(value.to_string()))
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Simple),
            1 => Some(Self::Email),
            2 => Some(Self::News),
            3 => Some(Self::Call),
            4 => Some(Self::MissedCall),
            5 => Some(Self::Sms),
            6 => Some(Self::Voicemail),
            7 => Some(Self::Schedule),
            8 => Some(Self::HighPriority),
            9 => Some(Self::InstantMessage),
            0xff => Some(Self::All),
            _ => None,
        }
    }
}

/// Current Time Service payload: local date and time, little-endian, ten bytes.
///
/// The watch has no timezone and no network: it displays exactly these fields.
/// `utc_offset_min` is therefore applied here, by the caller's choice, and the
/// last byte is the CTS manual-update reason (`0x01`).
pub fn current_time_packet(epoch_secs: u64, utc_offset_min: i32) -> Result<Vec<u8>, ProtocolError> {
    let local = i64::try_from(epoch_secs)
        .map_err(|_| ProtocolError::BadDate("timestamp out of range".into()))?
        + i64::from(utc_offset_min) * 60;
    if local < 0 {
        return Err(ProtocolError::BadDate(
            "local time is before 1970".to_string(),
        ));
    }
    let days = local.div_euclid(86_400);
    let seconds = local.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let year = u16::try_from(year)
        .map_err(|_| ProtocolError::BadDate(format!("year {year} does not fit CTS")))?;
    // 1970-01-01 was a Thursday, which is weekday 4 in the 1=Monday CTS scale.
    let weekday = ((days + 3).rem_euclid(7) + 1) as u8;
    Ok(vec![
        (year & 0xff) as u8,
        (year >> 8) as u8,
        month as u8,
        day as u8,
        (seconds / 3600) as u8,
        ((seconds % 3600) / 60) as u8,
        (seconds % 60) as u8,
        weekday,
        0,
        0x01,
    ])
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to (year, month, day).
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

/// The icon names the Navigation service accepts, from the `pure-maps` set the
/// InfiniTime document lists. The watch silently ignores anything else.
pub const NAVIGATION_ICONS: &[&str] = &[
    "arrive",
    "arrive-left",
    "arrive-right",
    "arrive-straight",
    "close",
    "continue",
    "continue-left",
    "continue-right",
    "continue-slight-left",
    "continue-slight-right",
    "continue-straight",
    "continue-uturn",
    "depart",
    "depart-left",
    "depart-right",
    "depart-straight",
    "end-of-road-left",
    "end-of-road-right",
    "ferry",
    "flag",
    "fork",
    "fork-left",
    "fork-right",
    "fork-slight-left",
    "fork-slight-right",
    "fork-straight",
    "invalid",
    "invalid-left",
    "invalid-right",
    "invalid-slight-left",
    "invalid-slight-right",
    "invalid-straight",
    "invalid-uturn",
    "merge-left",
    "merge-right",
    "merge-slight-left",
    "merge-slight-right",
    "merge-straight",
    "new-name-left",
    "new-name-right",
    "new-name-sharp-left",
    "new-name-sharp-right",
    "new-name-slight-left",
    "new-name-slight-right",
    "new-name-straight",
    "notification-left",
    "notification-right",
    "notification-sharp-left",
    "notification-sharp-right",
    "notification-slight-left",
    "notification-slight-right",
    "notification-straight",
    "off-ramp-left",
    "off-ramp-right",
    "off-ramp-sharp-left",
    "off-ramp-sharp-right",
    "off-ramp-slight-left",
    "off-ramp-slight-right",
    "off-ramp-straight",
    "on-ramp-left",
    "on-ramp-right",
    "on-ramp-sharp-left",
    "on-ramp-sharp-right",
    "on-ramp-slight-left",
    "on-ramp-slight-right",
    "on-ramp-straight",
    "rotary",
    "rotary-left",
    "rotary-right",
    "rotary-sharp-left",
    "rotary-sharp-right",
    "rotary-slight-left",
    "rotary-slight-right",
    "rotary-straight",
    "roundabout",
    "roundabout-left",
    "roundabout-right",
    "roundabout-sharp-left",
    "roundabout-sharp-right",
    "roundabout-slight-left",
    "roundabout-slight-right",
    "roundabout-straight",
    "turn-left",
    "turn-right",
    "turn-sharp-left",
    "turn-sharp-right",
    "turn-slight-left",
    "turn-slight-right",
    "turn-stright",
    "updown",
    "uturn",
];

pub fn validate_navigation_icon(name: &str) -> Result<(), ProtocolError> {
    if NAVIGATION_ICONS.contains(&name) {
        Ok(())
    } else {
        Err(ProtocolError::UnknownIcon(name.to_string()))
    }
}

/// One navigation instruction, as four characteristic writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation {
    pub icon: String,
    pub narrative: String,
    pub distance: String,
    pub progress: u8,
}

impl Navigation {
    /// `(characteristic, bytes)` in the order the watch expects them.
    pub fn packets(&self) -> Result<Vec<(&'static str, Vec<u8>)>, ProtocolError> {
        validate_navigation_icon(&self.icon)?;
        Ok(vec![
            (NAVIGATION_FLAGS, self.icon.as_bytes().to_vec()),
            (NAVIGATION_NARRATIVE, self.narrative.as_bytes().to_vec()),
            (NAVIGATION_MAN_DIST, self.distance.as_bytes().to_vec()),
            (NAVIGATION_PROGRESS, vec![self.progress.min(100)]),
        ])
    }
}

/// Icon ids of the Simple Weather service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherIcon {
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

impl WeatherIcon {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Clear => 0,
            Self::FewClouds => 1,
            Self::Clouds => 2,
            Self::HeavyClouds => 3,
            Self::CloudsRain => 4,
            Self::Rain => 5,
            Self::Thunderstorm => 6,
            Self::Snow => 7,
            Self::Mist => 8,
        }
    }

    pub fn parse(value: &str) -> Result<Self, ProtocolError> {
        match value.to_ascii_lowercase().replace('_', "-").as_str() {
            "clear" | "sun" => Ok(Self::Clear),
            "few-clouds" => Ok(Self::FewClouds),
            "clouds" => Ok(Self::Clouds),
            "heavy-clouds" => Ok(Self::HeavyClouds),
            "clouds-rain" => Ok(Self::CloudsRain),
            "rain" => Ok(Self::Rain),
            "thunderstorm" => Ok(Self::Thunderstorm),
            "snow" => Ok(Self::Snow),
            "mist" => Ok(Self::Mist),
            other => Err(ProtocolError::UnknownWeatherIcon(other.to_string())),
        }
    }
}

/// Sunrise/sunset values the service defines beyond plain minutes.
pub const SUN_UNKNOWN: i16 = -1;
pub const SUN_ABSENT: i16 = -2;

const WEATHER_LOCATION_BYTES: usize = 32;

/// Simple Weather current conditions, version 1 (with sunrise/sunset).
///
/// Temperatures are degrees Celsius x100 as `i16`; the location is a fixed
/// 32-byte zero-padded field, so a shorter name is not a shorter frame.
#[must_use]
pub fn current_weather_packet(
    timestamp: u64,
    current_c: i16,
    min_c: i16,
    max_c: i16,
    location: &str,
    icon: WeatherIcon,
    sunrise_min: i16,
    sunset_min: i16,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(53);
    out.push(0);
    out.push(1);
    out.extend_from_slice(&timestamp.to_le_bytes());
    out.extend_from_slice(&current_c.to_le_bytes());
    out.extend_from_slice(&min_c.to_le_bytes());
    out.extend_from_slice(&max_c.to_le_bytes());
    let mut name = [0u8; WEATHER_LOCATION_BYTES];
    let bytes = location.as_bytes();
    let take = bytes.len().min(WEATHER_LOCATION_BYTES);
    name[..take].copy_from_slice(&bytes[..take]);
    out.extend_from_slice(&name);
    out.push(icon.as_u8());
    out.extend_from_slice(&sunrise_min.to_le_bytes());
    out.extend_from_slice(&sunset_min.to_le_bytes());
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastDay {
    pub min_c: i16,
    pub max_c: i16,
    pub icon: WeatherIcon,
}

/// Simple Weather forecast, version 0: today plus up to four more days.
#[must_use]
pub fn forecast_packet(timestamp: u64, days: &[ForecastDay]) -> Vec<u8> {
    let mut out = Vec::with_capacity(36);
    out.push(1);
    out.push(0);
    out.extend_from_slice(&timestamp.to_le_bytes());
    out.push(days.len().min(5) as u8);
    for index in 0..5 {
        let used = days.get(index).copied();
        let (min_c, max_c, icon) = match used {
            Some(day) => (day.min_c, day.max_c, day.icon.as_u8()),
            None => (0, 0, 0),
        };
        out.extend_from_slice(&min_c.to_le_bytes());
        out.extend_from_slice(&max_c.to_le_bytes());
        out.push(icon);
    }
    out
}

/// A firmware version, parsed from the DIS firmware string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semver {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

/// The services this Spore speaks, and the InfiniTime release each arrived in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// Music service, 0.8.
    Music,
    /// Navigation service, 0.11.
    Navigation,
    /// Call characteristic on ANS, 0.13.
    Call,
    /// Motion service, 1.7.
    Motion,
    /// Simple Weather service, 1.14. The 1.8 weather service it replaced is not
    /// spoken: one weather service is enough, and it is the current one.
    SimpleWeather,
    /// Current Time Service, present since the first release.
    CurrentTime,
}

impl Service {
    #[must_use]
    pub const fn since(self) -> Option<Semver> {
        match self {
            Self::CurrentTime => None,
            Self::Music => Some(Semver {
                major: 0,
                minor: 8,
                patch: 0,
            }),
            Self::Navigation => Some(Semver {
                major: 0,
                minor: 11,
                patch: 0,
            }),
            Self::Call => Some(Semver {
                major: 0,
                minor: 13,
                patch: 0,
            }),
            Self::Motion => Some(Semver {
                major: 1,
                minor: 7,
                patch: 0,
            }),
            Self::SimpleWeather => Some(Semver {
                major: 1,
                minor: 14,
                patch: 0,
            }),
        }
    }
}

/// Find the first `major.minor[.patch]` in a firmware string such as
/// `InfiniTime 1.16.0` or `1.14`.
pub fn parse_firmware(text: &str) -> Option<Semver> {
    let mut digits = String::new();
    let mut parts: Vec<&str> = Vec::new();
    for chunk in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        if chunk.is_empty() {
            continue;
        }
        if chunk.chars().next().is_some_and(|c| c.is_ascii_digit()) && chunk.contains('.') {
            parts = chunk.split('.').collect();
            break;
        }
    }
    if parts.len() < 2 {
        return None;
    }
    digits.clear();
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts.get(2).and_then(|part| part.parse().ok()).unwrap_or(0);
    Some(Semver {
        major,
        minor,
        patch,
    })
}

/// `Some(true)` when the firmware is new enough, `Some(false)` when it predates
/// the service, `None` when the version string is unreadable.
///
/// Unreadable is not the same as old: a custom or patched firmware gets the
/// benefit of the doubt, and the write that follows fails loudly on its own if
/// the characteristic is missing.
#[must_use]
pub fn firmware_supports(firmware: &str, service: Service) -> Option<bool> {
    let since = service.since()?;
    let version = parse_firmware(firmware)?;
    Some(version >= since)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hr_short_is_an_error() {
        assert!(parse_heart_rate(&[0]).is_err());
    }

    #[test]
    fn infinitime_eight_bit_hr_is_parsed() {
        let hr = parse_heart_rate(&[0x00, 72]).expect("hr");
        assert_eq!(hr.bpm, Some(72));
    }

    #[test]
    fn zero_bpm_is_unknown_not_zero() {
        let hr = parse_heart_rate(&[0x00, 0]).expect("hr");
        assert_eq!(hr.bpm, None);
        assert_ne!(hr.bpm, Some(0));
    }

    #[test]
    fn sixteen_bit_and_rr_hr_is_parsed() {
        let hr = parse_heart_rate(&[0x11, 0x2c, 0x01, 0x10, 0x01, 0x20, 0x02]).expect("hr");
        assert_eq!(hr.bpm, Some(300));
        assert_eq!(hr.rr_intervals, vec![272, 544]);
    }

    #[test]
    fn steps_are_little_endian_u32() {
        assert_eq!(parse_steps(&[0x39, 0x30, 0, 0]).expect("steps"), 12_345);
        assert!(parse_steps(&[0, 0, 0]).is_err());
    }

    #[test]
    fn motion_is_three_signed_axes() {
        let motion = parse_motion(&[0x07, 0x00, 0xfd, 0xff, 0xe6, 0x03]).expect("motion");
        assert_eq!(
            motion,
            Motion {
                x: 7,
                y: -3,
                z: 998
            }
        );
    }

    #[test]
    fn notification_packet_matches_infinitime_format() {
        assert_eq!(
            notification_packet(0, "Test Title", "Test Body"),
            b"\x00\x01\x00Test Title\x00Test Body"
        );
    }

    #[test]
    fn music_and_call_events_are_mapped() {
        assert_eq!(parse_music_event(0x01), Ok(MusicEvent::Pause));
        assert_eq!(parse_call_response(1), Ok(CallResponse::Accepted));
        assert!(parse_music_event(0x99).is_err());
        assert!(parse_call_response(0x99).is_err());
    }

    #[test]
    fn current_time_packet_is_cts_little_endian() {
        // 2023-11-14T22:13:20Z, a Tuesday.
        let packet = current_time_packet(1_700_000_000, 0).expect("cts");
        assert_eq!(packet, vec![0xE7, 0x07, 11, 14, 22, 13, 20, 2, 0, 0x01]);
    }

    #[test]
    fn current_time_applies_the_callers_offset() {
        // +02:00 rolls the same instant into the next day.
        let packet = current_time_packet(1_700_000_000, 120).expect("cts");
        assert_eq!(packet, vec![0xE7, 0x07, 11, 15, 0, 13, 20, 3, 0, 0x01]);
    }

    #[test]
    fn the_cts_weekday_scale_starts_at_monday() {
        // 1970-01-01 was a Thursday, which is 4 when Monday is 1.
        let packet = current_time_packet(0, 0).expect("cts");
        assert_eq!(packet[7], 4);
    }

    #[test]
    fn navigation_writes_the_four_characteristics_in_order() {
        let navigation = Navigation {
            icon: "turn-left".to_string(),
            narrative: "Turn left onto Market".to_string(),
            distance: "50 m".to_string(),
            progress: 120,
        };
        let packets = navigation.packets().expect("nav");
        assert_eq!(packets[0].0, NAVIGATION_FLAGS);
        assert_eq!(packets[0].1, b"turn-left");
        assert_eq!(packets[1].0, NAVIGATION_NARRATIVE);
        assert_eq!(packets[2].0, NAVIGATION_MAN_DIST);
        assert_eq!(packets[3], (NAVIGATION_PROGRESS, vec![100]));
    }

    #[test]
    fn an_unknown_navigation_icon_is_refused() {
        let navigation = Navigation {
            icon: "turn-inward".to_string(),
            narrative: String::new(),
            distance: String::new(),
            progress: 0,
        };
        assert!(matches!(
            navigation.packets(),
            Err(ProtocolError::UnknownIcon(_))
        ));
    }

    #[test]
    fn current_weather_packet_matches_the_service_layout() {
        let packet = current_weather_packet(
            1_700_000_000,
            1_250,
            900,
            1_400,
            "Puget Sound",
            WeatherIcon::Rain,
            7 * 60,
            16 * 60 + 32,
        );
        assert_eq!(packet.len(), 53);
        assert_eq!(packet[0], 0);
        assert_eq!(packet[1], 1);
        assert_eq!(&packet[2..10], &1_700_000_000u64.to_le_bytes());
        assert_eq!(&packet[10..12], &1_250i16.to_le_bytes());
        assert_eq!(&packet[12..14], &900i16.to_le_bytes());
        assert_eq!(&packet[14..16], &1_400i16.to_le_bytes());
        assert_eq!(&packet[16..27], b"Puget Sound");
        assert!(packet[27..48].iter().all(|byte| *byte == 0));
        assert_eq!(packet[48], 5);
        assert_eq!(&packet[49..51], &420i16.to_le_bytes());
        assert_eq!(&packet[51..53], &992i16.to_le_bytes());
    }

    #[test]
    fn forecast_keeps_five_day_slots_and_zeroes_the_unused_ones() {
        let packet = forecast_packet(
            1_700_000_000,
            &[
                ForecastDay {
                    min_c: 900,
                    max_c: 1_400,
                    icon: WeatherIcon::Clouds,
                },
                ForecastDay {
                    min_c: -200,
                    max_c: 300,
                    icon: WeatherIcon::Snow,
                },
            ],
        );
        assert_eq!(packet.len(), 36);
        assert_eq!(packet[0], 1);
        assert_eq!(packet[1], 0);
        assert_eq!(packet[10], 2);
        assert_eq!(&packet[11..13], &900i16.to_le_bytes());
        assert_eq!(&packet[13..15], &1_400i16.to_le_bytes());
        assert_eq!(packet[15], 2);
        assert_eq!(&packet[16..18], &(-200i16).to_le_bytes());
        assert_eq!(packet[20], 7);
        assert!(packet[21..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn firmware_versions_are_found_in_dis_strings() {
        assert_eq!(
            parse_firmware("InfiniTime 1.16.0"),
            Some(Semver {
                major: 1,
                minor: 16,
                patch: 0
            })
        );
        assert_eq!(
            parse_firmware("1.14"),
            Some(Semver {
                major: 1,
                minor: 14,
                patch: 0
            })
        );
        assert_eq!(parse_firmware("custom build"), None);
    }

    #[test]
    fn service_minimums_gate_on_the_firmware() {
        assert_eq!(
            firmware_supports("InfiniTime 1.16.0", Service::SimpleWeather),
            Some(true)
        );
        assert_eq!(
            firmware_supports("InfiniTime 1.12.1", Service::SimpleWeather),
            Some(false)
        );
        assert_eq!(
            firmware_supports("InfiniTime 1.6", Service::Navigation),
            Some(true)
        );
        assert_eq!(
            firmware_supports("InfiniTime 0.10", Service::Navigation),
            Some(false)
        );
        assert_eq!(firmware_supports("custom build", Service::Navigation), None);
        // The Current Time Service has no minimum: every release has it.
        assert_eq!(
            firmware_supports("InfiniTime 0.1", Service::CurrentTime),
            None
        );
    }

    #[test]
    fn alert_categories_parse_by_name_and_number() {
        assert_eq!(AlertCategory::parse("call"), Ok(AlertCategory::Call));
        assert_eq!(
            AlertCategory::parse("missed_call"),
            Ok(AlertCategory::MissedCall)
        );
        assert_eq!(AlertCategory::parse("255"), Ok(AlertCategory::All));
        assert_eq!(AlertCategory::Sms.as_u8(), 5);
        assert!(AlertCategory::parse("urgent").is_err());
        assert!(AlertCategory::parse("42").is_err());
    }
}

/// The InfiniTime profile: the wire formats above, presented through the
/// hardware-agnostic seam `watch-core` defines.
pub struct Profile;

impl watch_core::WatchProfile for Profile {
    fn name(&self) -> &'static str {
        "infinitime"
    }

    fn streams(&self) -> &'static [watch_core::Stream] {
        &[
            watch_core::Stream::HeartRate,
            watch_core::Stream::Steps,
            watch_core::Stream::Motion,
            watch_core::Stream::Media,
            watch_core::Stream::Calls,
            watch_core::Stream::Battery,
        ]
    }

    fn decode(
        &self,
        char_uuid: &str,
        data: &[u8],
    ) -> Result<Option<watch_core::Signal>, String> {
        use watch_core::{Signal, Stream};
        let signal = if char_uuid.eq_ignore_ascii_case(HEART_RATE_MEASUREMENT) {
            let heart_rate = parse_heart_rate(data).map_err(|error| error.to_string())?;
            let value = match heart_rate.bpm {
                Some(bpm) => serde_json::json!({
                    "bpm": bpm,
                    "contact": heart_rate.contact,
                    "rr_intervals": heart_rate.rr_intervals,
                }),
                None => serde_json::json!({"bpm": null, "status": "no-measurement"}),
            };
            Signal {
                stream: Stream::HeartRate,
                value,
                privacy_class: "health",
                retention_class: "raw",
                // A watch that reports no measurement is unknown, not zero.
                quality: if heart_rate.bpm.is_some() { 1.0 } else { 0.0 },
            }
        } else if char_uuid.eq_ignore_ascii_case(STEP_COUNT) {
            let steps = parse_steps(data).map_err(|error| error.to_string())?;
            Signal {
                stream: Stream::Steps,
                value: serde_json::json!({"count": steps}),
                privacy_class: "health",
                retention_class: "summary",
                quality: 1.0,
            }
        } else if char_uuid.eq_ignore_ascii_case(MOTION_VALUES) {
            let motion = parse_motion(data).map_err(|error| error.to_string())?;
            Signal {
                stream: Stream::Motion,
                value: serde_json::json!({"x": motion.x, "y": motion.y, "z": motion.z}),
                privacy_class: "health",
                retention_class: "raw",
                quality: 1.0,
            }
        } else if char_uuid.eq_ignore_ascii_case(MUSIC_EVENT) {
            let byte = data.first().copied().ok_or("empty music event")?;
            let event = parse_music_event(byte).map_err(|error| error.to_string())?;
            Signal {
                stream: Stream::Media,
                value: serde_json::json!({"event": event.as_str()}),
                privacy_class: "presence",
                retention_class: "summary",
                quality: 1.0,
            }
        } else if char_uuid.eq_ignore_ascii_case(NOTIFICATION_EVENT) {
            let byte = data.first().copied().ok_or("empty call response")?;
            let response = parse_call_response(byte).map_err(|error| error.to_string())?;
            Signal {
                stream: Stream::Calls,
                value: serde_json::json!({"response": response.as_str()}),
                privacy_class: "presence",
                retention_class: "summary",
                quality: 1.0,
            }
        } else {
            return Ok(None);
        };
        Ok(Some(signal))
    }

    fn supports(&self, firmware: &str, intent: watch_core::Intent) -> Option<bool> {
        use watch_core::Intent;
        let service = match intent {
            // Watching covers the motion service (steps and motion) plus the
            // always-present heart-rate characteristic.
            Intent::Watch => Service::Motion,
            Intent::Media => Service::Music,
            Intent::Nav => Service::Navigation,
            Intent::Weather => Service::SimpleWeather,
            Intent::Find | Intent::Notify | Intent::Time => return None,
        };
        firmware_supports(firmware, service)
    }
}
