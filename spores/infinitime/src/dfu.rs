//! Nordic Secure DFU over BLE, as InfiniTime's `doc/ble.md` describes it.
//!
//! Source: <https://github.com/InfiniTimeOrg/InfiniTime/blob/main/doc/ble.md>
//!
//! This is the state machine and nothing else: no radio, no file, no clock. A
//! caller feeds it the image bytes and drives it with `next`, answering
//! `AwaitControl` from the control-point characteristic. That is what makes a
//! nine-step procedure that can brick a watch testable without one.
//!
//! The byte-level contract, in the document's order:
//! `0x01 0x04` starts DFU, the packet characteristic carries eight zero bytes
//! then the image length as `u32` LE, `0x02 0x00` announces the init packet,
//! `0x02 0x01` closes it, `0x08 <n>` sets the receipt interval in segments,
//! `0x03` begins the image, `0x04` validates and `0x05` activates.

use std::fmt;

pub const CONTROL_POINT: &str = "00001531-1212-efde-1523-785feabcd123";
pub const PACKET: &str = "00001532-1212-efde-1523-785feabcd123";

/// The firmware is sent in 20-byte segments, per the document.
pub const SEGMENT_BYTES: usize = 20;
/// A notification starting with this opcode reports bytes received.
pub const RECEIPT_OPCODE: u8 = 0x11;
/// A notification starting with this opcode is a command response.
pub const RESPONSE_OPCODE: u8 = 0x10;

/// One thing the caller has to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write these bytes to the control point characteristic.
    ControlPoint(Vec<u8>),
    /// Write these bytes to the packet characteristic.
    Packet(Vec<u8>),
    /// Wait for a notification on the control point, then call `on_control`.
    AwaitControl,
    /// Nothing left. The watch is validating and will reboot.
    Done,
}

/// A DFU procedure over one image and its init packet.
pub struct Dfu<'a> {
    image: &'a [u8],
    init: &'a [u8],
    interval: u32,
    phase: Phase,
    /// Firmware bytes written to the packet characteristic, which is what a
    /// receipt reports.
    sent: u32,
    segments_since_receipt: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Start,
    Size,
    AwaitStart,
    InitRequest,
    InitPacket,
    InitDone,
    AwaitInit,
    Interval,
    Begin,
    Data(usize),
    AwaitReceipt,
    AwaitImage,
    Validate,
    AwaitValidated,
    Activate,
    Done,
}

impl<'a> Dfu<'a> {
    /// `interval` is the number of 20-byte segments between receipts. `itd`
    /// uses ten, which is also the default here.
    pub fn new(image: &'a [u8], init: &'a [u8], interval: u32) -> Result<Self, DfuError> {
        if image.is_empty() {
            return Err(DfuError::EmptyImage);
        }
        if init.is_empty() {
            return Err(DfuError::EmptyInit);
        }
        if interval == 0 || interval > u32::from(u8::MAX) {
            return Err(DfuError::BadInterval(interval));
        }
        if u32::try_from(image.len()).is_err() {
            return Err(DfuError::ImageTooLarge(image.len()));
        }
        Ok(Self {
            image,
            init,
            interval,
            phase: Phase::Start,
            sent: 0,
            segments_since_receipt: 0,
        })
    }

    /// What to do next. `AwaitControl` stays put until `on_control` advances
    /// the phase, so a caller can loop on it without losing its place.
    pub fn next(&mut self) -> Action {
        match self.phase {
            Phase::Start => {
                self.phase = Phase::Size;
                Action::ControlPoint(vec![0x01, 0x04])
            }
            Phase::Size => {
                self.phase = Phase::AwaitStart;
                let mut packet = vec![0u8; 8];
                packet.extend_from_slice(&(self.image.len() as u32).to_le_bytes());
                Action::Packet(packet)
            }
            Phase::AwaitStart => Action::AwaitControl,
            Phase::InitRequest => {
                self.phase = Phase::InitPacket;
                Action::ControlPoint(vec![0x02, 0x00])
            }
            Phase::InitPacket => {
                self.phase = Phase::InitDone;
                Action::Packet(self.init.to_vec())
            }
            Phase::InitDone => {
                self.phase = Phase::AwaitInit;
                Action::ControlPoint(vec![0x02, 0x01])
            }
            Phase::AwaitInit => Action::AwaitControl,
            Phase::Interval => {
                self.phase = Phase::Begin;
                Action::ControlPoint(vec![0x08, self.interval as u8])
            }
            Phase::Begin => {
                self.phase = Phase::Data(0);
                Action::ControlPoint(vec![0x03])
            }
            Phase::Data(offset) => {
                if offset >= self.image.len() {
                    self.phase = Phase::AwaitImage;
                    return Action::AwaitControl;
                }
                let end = (offset + SEGMENT_BYTES).min(self.image.len());
                let chunk = self.image[offset..end].to_vec();
                self.sent += chunk.len() as u32;
                self.segments_since_receipt += 1;
                let next = if self.segments_since_receipt >= self.interval {
                    self.segments_since_receipt = 0;
                    Phase::AwaitReceipt
                } else {
                    Phase::Data(end)
                };
                self.phase = next;
                Action::Packet(chunk)
            }
            Phase::AwaitReceipt => Action::AwaitControl,
            Phase::AwaitImage => Action::AwaitControl,
            Phase::Validate => {
                self.phase = Phase::AwaitValidated;
                Action::ControlPoint(vec![0x04])
            }
            Phase::AwaitValidated => Action::AwaitControl,
            Phase::Activate => {
                self.phase = Phase::Done;
                Action::ControlPoint(vec![0x05])
            }
            Phase::Done => Action::Done,
        }
    }

    /// Feed one control-point notification.
    pub fn on_control(&mut self, data: &[u8]) -> Result<(), DfuError> {
        match self.phase {
            Phase::AwaitStart => {
                expect_response(data, 0x01)?;
                self.phase = Phase::InitRequest;
            }
            Phase::AwaitInit => {
                expect_response(data, 0x02)?;
                self.phase = Phase::Interval;
            }
            Phase::AwaitReceipt => {
                let receipt = parse_receipt(data)?;
                if receipt != self.sent {
                    return Err(DfuError::ReceiptMismatch {
                        sent: self.sent,
                        reported: receipt,
                    });
                }
                self.phase = Phase::Data(next_offset(self.sent as usize, self.image.len()));
            }
            Phase::AwaitImage => {
                expect_response(data, 0x03)?;
                self.phase = Phase::Validate;
            }
            Phase::AwaitValidated => {
                expect_response(data, 0x04)?;
                self.phase = Phase::Activate;
            }
            other => {
                return Err(DfuError::UnexpectedNotification {
                    phase: format!("{other:?}"),
                    data: data.to_vec(),
                });
            }
        }
        Ok(())
    }

    /// Bytes written to the packet characteristic so far. The caller can use it
    /// for a progress line; a receipt must always equal it.
    #[must_use]
    pub fn sent(&self) -> u32 {
        self.sent
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Total bytes the watch will receive, for a percentage.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.image.len() as u32
    }
}

fn next_offset(sent: usize, len: usize) -> usize {
    // `sent` counts firmware bytes; the image is contiguous, so the offset is
    // the same number. Kept as a named helper so the invariant is visible.
    sent.min(len)
}

fn expect_response(data: &[u8], opcode: u8) -> Result<(), DfuError> {
    if data.len() >= 3 && data[0] == RESPONSE_OPCODE && data[1] == opcode && data[2] == 0x01 {
        return Ok(());
    }
    Err(DfuError::UnexpectedResponse {
        expected: opcode,
        data: data.to_vec(),
    })
}

fn parse_receipt(data: &[u8]) -> Result<u32, DfuError> {
    if data.len() < 5 || data[0] != RECEIPT_OPCODE {
        return Err(DfuError::UnexpectedReceipt(data.to_vec()));
    }
    Ok(u32::from_le_bytes([data[1], data[2], data[3], data[4]]))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DfuError {
    EmptyImage,
    EmptyInit,
    BadInterval(u32),
    ImageTooLarge(usize),
    UnexpectedResponse { expected: u8, data: Vec<u8> },
    UnexpectedReceipt(Vec<u8>),
    ReceiptMismatch { sent: u32, reported: u32 },
    UnexpectedNotification { phase: String, data: Vec<u8> },
}

impl fmt::Display for DfuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyImage => write!(f, "the firmware image is empty"),
            Self::EmptyInit => write!(f, "the init packet is empty"),
            Self::BadInterval(interval) => {
                write!(f, "receipt interval {interval} does not fit one byte")
            }
            Self::ImageTooLarge(size) => write!(f, "image is {size} bytes; DFU carries u32"),
            Self::UnexpectedResponse { expected, data } => {
                write!(f, "expected response {expected:#04x}, got {data:02x?}")
            }
            Self::UnexpectedReceipt(data) => write!(f, "not a receipt: {data:02x?}"),
            Self::ReceiptMismatch { sent, reported } => {
                write!(f, "watch received {reported} bytes; {sent} were sent")
            }
            Self::UnexpectedNotification { phase, data } => {
                write!(f, "unexpected notification {data:02x?} in phase {phase}")
            }
        }
    }
}

impl std::error::Error for DfuError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `next` until the machine needs the watch to answer, which is the
    /// only point where a caller cannot proceed on its own.
    fn until_await(dfu: &mut Dfu<'_>, script: &mut Vec<Action>) {
        for _ in 0..10_000 {
            match dfu.next() {
                Action::AwaitControl | Action::Done => return,
                action => script.push(action),
            }
        }
        panic!("dfu did not converge");
    }

    #[test]
    fn a_short_image_runs_the_documented_steps_in_order() {
        let image = vec![0xAAu8; 50];
        let init = vec![0xBBu8; 4];
        let mut dfu = Dfu::new(&image, &init, 10).expect("dfu");
        let mut script = Vec::new();
        until_await(&mut dfu, &mut script);

        assert_eq!(script[0], Action::ControlPoint(vec![0x01, 0x04]));
        match &script[1] {
            Action::Packet(packet) => {
                assert_eq!(packet.len(), 12);
                assert_eq!(&packet[..8], &[0u8; 8]);
                assert_eq!(
                    u32::from_le_bytes([packet[8], packet[9], packet[10], packet[11]]),
                    50
                );
            }
            other => panic!("expected the size packet, got {other:?}"),
        }
        // Answer the start response: the machine asks for the init packet next.
        dfu.on_control(&[0x10, 0x01, 0x01]).expect("start ack");
        assert_eq!(dfu.next(), Action::ControlPoint(vec![0x02, 0x00]));
        assert_eq!(dfu.next(), Action::Packet(init.clone()));
        assert_eq!(dfu.next(), Action::ControlPoint(vec![0x02, 0x01]));
        assert_eq!(dfu.next(), Action::AwaitControl);
        dfu.on_control(&[0x10, 0x02, 0x01]).expect("init ack");
        assert_eq!(dfu.next(), Action::ControlPoint(vec![0x08, 10]));
        assert_eq!(dfu.next(), Action::ControlPoint(vec![0x03]));
        // Three segments: 20, 20, 10.
        assert_eq!(dfu.next(), Action::Packet(vec![0xAA; 20]));
        assert_eq!(dfu.next(), Action::Packet(vec![0xAA; 20]));
        assert_eq!(dfu.next(), Action::Packet(vec![0xAA; 10]));
        assert_eq!(dfu.sent(), 50);
        assert_eq!(dfu.next(), Action::AwaitControl);
        dfu.on_control(&[0x10, 0x03, 0x01]).expect("image ack");
        assert_eq!(dfu.next(), Action::ControlPoint(vec![0x04]));
        assert_eq!(dfu.next(), Action::AwaitControl);
        dfu.on_control(&[0x10, 0x04, 0x01]).expect("validate ack");
        assert_eq!(dfu.next(), Action::ControlPoint(vec![0x05]));
        assert_eq!(dfu.next(), Action::Done);
        assert!(dfu.is_done());
    }

    #[test]
    fn receipts_arrive_every_interval_and_must_match_what_was_sent() {
        // 230 bytes is 12 segments; interval 10 means one receipt after 200.
        let image = vec![1u8; 230];
        let init = vec![2u8; 3];
        let mut dfu = Dfu::new(&image, &init, 10).expect("dfu");
        assert_eq!(dfu.next(), Action::ControlPoint(vec![0x01, 0x04]));
        assert!(matches!(dfu.next(), Action::Packet(_)));
        dfu.on_control(&[0x10, 0x01, 0x01]).expect("start");
        let _ = dfu.next();
        let _ = dfu.next();
        let _ = dfu.next();
        dfu.on_control(&[0x10, 0x02, 0x01]).expect("init");
        let _ = dfu.next(); // interval
        let _ = dfu.next(); // begin

        for _ in 0..10 {
            assert!(matches!(dfu.next(), Action::Packet(_)));
        }
        assert_eq!(dfu.next(), Action::AwaitControl);
        assert_eq!(dfu.sent(), 200);

        // A receipt for the wrong count is a failure, not a nudge.
        assert_eq!(dfu.on_control(&[0x11, 0xC8, 0x00, 0x00, 0x00]), Ok(()));
        for _ in 0..2 {
            assert!(matches!(dfu.next(), Action::Packet(_)));
        }
        assert_eq!(dfu.sent(), 230);
        assert_eq!(dfu.next(), Action::AwaitControl);
        dfu.on_control(&[0x10, 0x03, 0x01]).expect("image ack");
        assert!(matches!(dfu.next(), Action::ControlPoint(bytes) if bytes == vec![0x04]));
    }

    #[test]
    fn a_receipt_for_the_wrong_count_stops_the_transfer() {
        let image = vec![1u8; 200];
        let init = vec![2u8; 3];
        let mut dfu = Dfu::new(&image, &init, 10).expect("dfu");
        assert_eq!(dfu.next(), Action::ControlPoint(vec![0x01, 0x04]));
        assert!(matches!(dfu.next(), Action::Packet(_)));
        dfu.on_control(&[0x10, 0x01, 0x01]).expect("start");
        let _ = dfu.next();
        let _ = dfu.next();
        let _ = dfu.next();
        dfu.on_control(&[0x10, 0x02, 0x01]).expect("init");
        let _ = dfu.next();
        let _ = dfu.next();
        for _ in 0..10 {
            let _ = dfu.next();
        }
        assert_eq!(dfu.next(), Action::AwaitControl);
        assert_eq!(
            dfu.on_control(&[0x11, 0x01, 0x00, 0x00, 0x00]),
            Err(DfuError::ReceiptMismatch {
                sent: 200,
                reported: 1
            })
        );
    }

    #[test]
    fn an_unexpected_response_names_what_it_expected() {
        let image = vec![0u8; 20];
        let init = vec![0u8; 1];
        let mut dfu = Dfu::new(&image, &init, 10).expect("dfu");
        assert_eq!(
            dfu.on_control(&[0x10, 0x01, 0x01]),
            Err(DfuError::UnexpectedNotification {
                phase: "Start".into(),
                data: vec![0x10, 0x01, 0x01]
            })
        );
        let _ = dfu.next();
        let _ = dfu.next();
        assert_eq!(
            dfu.on_control(&[0x10, 0x02, 0x01]),
            Err(DfuError::UnexpectedResponse {
                expected: 0x01,
                data: vec![0x10, 0x02, 0x01]
            })
        );
    }

    #[test]
    fn empty_inputs_are_refused_before_the_first_write() {
        assert_eq!(Dfu::new(&[], &[1], 10).err(), Some(DfuError::EmptyImage));
        assert_eq!(Dfu::new(&[1], &[], 10).err(), Some(DfuError::EmptyInit));
        assert_eq!(
            Dfu::new(&[1], &[1], 0).err(),
            Some(DfuError::BadInterval(0))
        );
        assert_eq!(
            Dfu::new(&[1], &[1], 300).err(),
            Some(DfuError::BadInterval(300))
        );
    }
}
