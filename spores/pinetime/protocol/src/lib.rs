//! PineTime / InfiniTime wire formats.
//!
//! This crate is deliberately free of wasm imports: the protocol is testable
//! on the host, and the Spore is a thin BLE + Records wrapper around it.
//!
//! Sources:
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/doc/ble.md>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/doc/MotionService.md>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/src/components/ble/HeartRateService.cpp>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/src/components/ble/AlertNotificationService.cpp>
//! - <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/src/components/ble/MusicService.h>

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    TooShort { expected: usize, actual: usize },
    BadFlags(u8),
    UnknownMusicEvent(u8),
    UnknownCallResponse(u8),
    MessageTooLong(usize),
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
}
