//! Razer HID control protocol: 90-byte feature reports.
//!
//! `no_std`, no allocations, no dependencies: building and checking reports only.
//!
//! ```text
//! 0      status            (0x00 in requests; reply status from the device)
//! 1      transaction_id    (0x1F for Naga Pro and most 2019+ mice)
//! 2..3   remaining_packets (big-endian)
//! 4      protocol_type     (0x00)
//! 5      data_size         (meaningful bytes in arguments)
//! 6      command_class
//! 7      command_id        (bit 7 set = read / "get")
//! 8..87  arguments[80]
//! 88     crc               (XOR of bytes 2..87)
//! 89     reserved
//! ```

#![no_std]
#![deny(clippy::unwrap_used, clippy::expect_used)]

use core::fmt;

pub const REPORT_LEN: usize = 90;
pub const ARGS_LEN: usize = 80;

/// Reply status (byte 0).
pub mod status {
    pub const NEW: u8 = 0x00;
    pub const BUSY: u8 = 0x01;
    pub const OK: u8 = 0x02;
    pub const FAIL: u8 = 0x03;
    pub const TIMEOUT: u8 = 0x04;
    pub const NOT_SUPPORTED: u8 = 0x05;

    /// Name of a known status ("BUSY", "OK", …); `None` for other values.
    pub fn label(s: u8) -> Option<&'static str> {
        Some(match s {
            NEW => "NEW",
            BUSY => "BUSY",
            OK => "OK",
            FAIL => "FAIL",
            TIMEOUT => "TIMEOUT",
            NOT_SUPPORTED => "NOT_SUPPORTED",
            _ => return None,
        })
    }
}

/// What can be wrong with a report itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    /// more than 80 argument bytes
    ArgsTooLong(usize),
    /// fewer than 90 bytes received
    TooShort(usize),
    /// byte 88 is not the XOR of bytes 2..87
    BadCrc { expected: u8, found: u8 },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::ArgsTooLong(n) => write!(f, "arguments too long ({n} bytes, at most {ARGS_LEN})"),
            ProtocolError::TooShort(n) => write!(f, "report too short ({n} bytes, {REPORT_LEN} expected)"),
            ProtocolError::BadCrc { expected, found } => write!(f, "bad checksum: 0x{found:02x}, expected 0x{expected:02x}"),
        }
    }
}

/// XOR of bytes 2..87.
pub fn crc(data: &[u8; REPORT_LEN]) -> u8 {
    data[2..88].iter().fold(0u8, |c, b| c ^ b)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub status: u8,
    pub transaction_id: u8,
    pub remaining_packets: u16,
    pub protocol_type: u8,
    pub data_size: u8,
    pub command_class: u8,
    pub command_id: u8,
    pub arguments: [u8; ARGS_LEN],
}

impl Report {
    /// A request: `cmd` = class << 8 | id; `data_size` defaults to the number of arguments.
    pub fn new(cmd: u16, args: &[u8], data_size: Option<u8>, transaction_id: u8) -> Result<Report, ProtocolError> {
        if args.len() > ARGS_LEN {
            return Err(ProtocolError::ArgsTooLong(args.len()));
        }
        let mut a = [0u8; ARGS_LEN];
        a[..args.len()].copy_from_slice(args);
        Ok(Report {
            status: status::NEW,
            transaction_id,
            remaining_packets: 0,
            protocol_type: 0,
            data_size: data_size.unwrap_or(args.len() as u8),
            command_class: (cmd >> 8) as u8,
            command_id: (cmd & 0xFF) as u8,
            arguments: a,
        })
    }

    pub fn cmd(&self) -> u16 {
        ((self.command_class as u16) << 8) | self.command_id as u16
    }

    /// The 90 bytes to send, with the checksum.
    pub fn to_bytes(&self) -> [u8; REPORT_LEN] {
        let mut b = [0u8; REPORT_LEN];
        b[0] = self.status;
        b[1] = self.transaction_id;
        b[2..4].copy_from_slice(&self.remaining_packets.to_be_bytes());
        b[4] = self.protocol_type;
        b[5] = self.data_size;
        b[6] = self.command_class;
        b[7] = self.command_id;
        b[8..88].copy_from_slice(&self.arguments);
        b[88] = crc(&b);
        b
    }

    /// Lenient parse: missing bytes are zeros, the checksum is not checked
    /// (mice do not always fill it in their replies).
    pub fn from_bytes(d: &[u8]) -> Report {
        let mut b = [0u8; REPORT_LEN];
        let n = d.len().min(REPORT_LEN);
        b[..n].copy_from_slice(&d[..n]);
        let mut a = [0u8; ARGS_LEN];
        a.copy_from_slice(&b[8..88]);
        Report {
            status: b[0],
            transaction_id: b[1],
            remaining_packets: u16::from_be_bytes([b[2], b[3]]),
            protocol_type: b[4],
            data_size: b[5],
            command_class: b[6],
            command_id: b[7],
            arguments: a,
        }
    }

    /// Strict parse: exactly a full report with a valid checksum.
    pub fn parse(d: &[u8]) -> Result<Report, ProtocolError> {
        if d.len() < REPORT_LEN {
            return Err(ProtocolError::TooShort(d.len()));
        }
        let mut b = [0u8; REPORT_LEN];
        b.copy_from_slice(&d[..REPORT_LEN]);
        let expected = crc(&b);
        if b[88] != expected {
            return Err(ProtocolError::BadCrc { expected, found: b[88] });
        }
        Ok(Report::from_bytes(&b))
    }

    /// The meaningful arguments (`data_size` bytes, at least one).
    pub fn args(&self) -> &[u8] {
        let n = (self.data_size as usize).clamp(1, ARGS_LEN);
        &self.arguments[..n]
    }

    /// Does this reply answer `request` (same command and transaction id)?
    pub fn answers(&self, request: &Report) -> bool {
        self.cmd() == request.cmd() && self.transaction_id == request.transaction_id
    }

    /// Reply status: OK.
    pub fn is_ok(&self) -> bool {
        self.status == status::OK
    }
}

/// One-line dump for logs: `st=02 tid=1f size=02 cmd=0081 args=02 01`.
impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "st={:02x} tid={:02x} size={:02x} cmd={:02x}{:02x} args=",
            self.status, self.transaction_id, self.data_size, self.command_class, self.command_id
        )?;
        for (i, b) in self.args().iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// Known command codes (class << 8 | id). Read variants have bit 7 of id set.
pub mod cmd {
    pub const GET_FIRMWARE: u16 = 0x0081;
    pub const GET_SERIAL: u16 = 0x0082;
    /// Naga Pro: attached side plate (0 none, 1 = 2-button, 3 = 12-button, 4 = 6-button)
    pub const GET_SIDE_PLATE: u16 = 0x00B9;
    pub const SET_POLLING: u16 = 0x0005;
    pub const GET_POLLING: u16 = 0x0085;
    pub const SET_BUTTON: u16 = 0x020C;
    pub const GET_BUTTON: u16 = 0x028C;
    pub const SET_DPI: u16 = 0x0405;
    pub const GET_DPI: u16 = 0x0485;
    pub const SET_DPI_STAGES: u16 = 0x0406;
    pub const GET_DPI_STAGES: u16 = 0x0486;
    pub const GET_PROFILE_COUNT: u16 = 0x0580;
    pub const GET_PROFILE_LIST: u16 = 0x0581;
    pub const GET_MACRO_COUNT: u16 = 0x0680;
    pub const GET_MACRO_LIST: u16 = 0x068B;
    pub const DELETE_MACRO: u16 = 0x0603;
    pub const SET_MACRO_SIZE: u16 = 0x0608;
    pub const GET_MACRO_SIZE: u16 = 0x0688;
    pub const SET_MACRO_DATA: u16 = 0x0609;
    pub const GET_MACRO_DATA: u16 = 0x0689;
    pub const GET_FLASH_USAGE: u16 = 0x068E;
    pub const GET_BATTERY: u16 = 0x0780;
    pub const GET_CHARGING: u16 = 0x0784;
    pub const SET_IDLE_TIME: u16 = 0x0703;
    pub const GET_IDLE_TIME: u16 = 0x0783;
    pub const SET_LOW_BATTERY: u16 = 0x0701;
    pub const GET_LOW_BATTERY: u16 = 0x0781;
    pub const SET_EFFECT: u16 = 0x0F02;
    pub const SET_FRAME: u16 = 0x0F03;
    pub const SET_BRIGHTNESS: u16 = 0x0F04;
    pub const GET_BRIGHTNESS: u16 = 0x0F84;
    pub const SET_MOUSE_EFFECT: u16 = 0x030D;

    /// Is this a read ("get") command?
    pub const fn is_read(cmd: u16) -> bool {
        cmd & 0x80 != 0
    }
}

/// Legacy polling-rate codes.
pub fn polling_code(hz: u32) -> Option<u8> {
    match hz {
        1000 => Some(0x01),
        500 => Some(0x02),
        125 => Some(0x08),
        _ => None,
    }
}

pub fn polling_hz(code: u8) -> u32 {
    match code {
        0x01 => 1000,
        0x02 => 500,
        0x08 => 125,
        _ => 0,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    extern crate std;
    use super::*;
    use std::string::ToString;

    #[test]
    fn layout_matches_openrazer() {
        let r = Report::new(cmd::GET_BATTERY, &[], Some(2), 0x1F).unwrap().to_bytes();
        assert_eq!(&r[..8], &[0, 0x1F, 0, 0, 0, 0x02, 0x07, 0x80]);
        assert_eq!(r[88], 0x02 ^ 0x07 ^ 0x80);
        assert_eq!(r[89], 0);
    }

    #[test]
    fn roundtrip() {
        let r = Report::new(cmd::SET_DPI, &[1, 6, 0x40, 6, 0x40, 0, 0], Some(7), 0x1F).unwrap();
        let r2 = Report::from_bytes(&r.to_bytes());
        assert_eq!(r, r2);
        assert_eq!(r2.cmd(), cmd::SET_DPI);
        assert_eq!(Report::parse(&r.to_bytes()).unwrap(), r);
    }

    #[test]
    fn remaining_packets_big_endian() {
        let mut r = Report::new(cmd::SET_MACRO_DATA, &[], None, 0x1F).unwrap();
        r.remaining_packets = 0x0102;
        let b = r.to_bytes();
        assert_eq!(&b[2..4], &[0x01, 0x02]);
        assert_eq!(Report::from_bytes(&b).remaining_packets, 0x0102);
    }

    #[test]
    fn errors() {
        assert_eq!(Report::new(0, &[0; 81], None, 0x1F), Err(ProtocolError::ArgsTooLong(81)));
        assert_eq!(Report::parse(&[0; 10]), Err(ProtocolError::TooShort(10)));
        let mut b = Report::new(cmd::GET_FIRMWARE, &[0, 0], None, 0x1F).unwrap().to_bytes();
        b[88] ^= 0xFF;
        assert!(matches!(Report::parse(&b), Err(ProtocolError::BadCrc { .. })));
        // a short reply is still readable leniently
        assert_eq!(Report::from_bytes(&b[..8]).cmd(), cmd::GET_FIRMWARE);
    }

    #[test]
    fn display_and_helpers() {
        let mut r = Report::new(cmd::GET_FIRMWARE, &[0x02, 0x01], None, 0x1F).unwrap();
        r.status = status::OK;
        assert_eq!(r.to_string(), "st=02 tid=1f size=02 cmd=0081 args=02 01");
        assert!(r.is_ok());
        assert!(r.answers(&Report::new(cmd::GET_FIRMWARE, &[], Some(2), 0x1F).unwrap()));
        assert!(!r.answers(&Report::new(cmd::GET_SERIAL, &[], Some(2), 0x1F).unwrap()));
        assert!(cmd::is_read(cmd::GET_DPI) && !cmd::is_read(cmd::SET_DPI));
        assert_eq!(status::label(status::BUSY), Some("BUSY"));
        assert_eq!(status::label(0x42), None);
        assert_eq!((polling_code(500), polling_hz(0x08)), (Some(0x02), 125));
    }
}
