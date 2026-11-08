//! Razer HID control protocol: 90-byte feature reports.
//!
//! ```text
//! 0      status            (0x00 in requests; reply status from the device)
//! 1      transaction_id    (0x1F for Naga Pro and most 2019+ mice)
//! 2..3   remaining_packets
//! 4      protocol_type     (0x00)
//! 5      data_size         (meaningful bytes in arguments)
//! 6      command_class
//! 7      command_id        (bit 7 set = read / "get")
//! 8..87  arguments[80]
//! 88     crc               (XOR of bytes 2..87)
//! 89     reserved
//! ```

use std::fmt;

pub const REPORT_LEN: usize = 90;
pub const ARGS_LEN: usize = 80;

pub mod status {
    pub const NEW: u8 = 0x00;
    pub const BUSY: u8 = 0x01;
    pub const OK: u8 = 0x02;
    pub const FAIL: u8 = 0x03;
    pub const TIMEOUT: u8 = 0x04;
    pub const NOT_SUPPORTED: u8 = 0x05;

    pub fn name(s: u8) -> String {
        match s {
            NEW => "NEW".into(),
            BUSY => "BUSY".into(),
            OK => "OK".into(),
            FAIL => "FAIL".into(),
            TIMEOUT => "TIMEOUT".into(),
            NOT_SUPPORTED => "NOT_SUPPORTED".into(),
            x => format!("0x{x:02x}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    /// I/O with the device or the OS failed.
    Io(String),
    /// The mouse answered with a non-OK status.
    Failed { cmd: u16, status: u8 },
    /// The reply belonged to another command (another program talks to the mouse).
    Conflict,
    /// No device / nothing to open.
    NotFound(String),
    /// Bad argument from the caller.
    Invalid(String),
    /// The OS denied access to the device (Linux: udev rule missing).
    NoAccess(String),
    /// the mouse (or the way it is connected) has no such feature
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(s) => write!(f, "{s}"),
            Error::Failed { cmd, status } => {
                write!(f, "command 0x{cmd:04x} failed: {}", status::name(*status))
            }
            Error::Conflict => write!(
                f,
                "The reply belongs to another command: another program is talking to the mouse \
                 (Razer Synapse / OpenRazer). Close it."
            ),
            Error::NoAccess(s) => write!(f, "{s}"),
            Error::NotFound(s) => write!(f, "{s}"),
            Error::Invalid(s) => write!(f, "{s}"),
            Error::Unsupported(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Stable code for the UI to translate the message: io, failed, conflict, not_found, invalid, no_access.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io(_) => "io",
            Error::Failed { .. } => "failed",
            Error::Conflict => "conflict",
            Error::NotFound(_) => "not_found",
            Error::Invalid(_) => "invalid",
            Error::NoAccess(_) => "no_access",
            Error::Unsupported(_) => "unsupported",
        }
    }

    /// True when the mouse simply does not support/accept this command.
    pub fn is_failed(&self) -> bool {
        matches!(self, Error::Failed { .. })
    }
}

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
    pub fn new(cmd: u16, args: &[u8], data_size: Option<u8>, transaction_id: u8) -> Result<Report> {
        if args.len() > ARGS_LEN {
            return Err(Error::Invalid("arguments too long".into()));
        }
        let mut a = [0u8; ARGS_LEN];
        a[..args.len()].copy_from_slice(args);
        Ok(Report {
            status: 0,
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

    pub fn hexdump(&self) -> String {
        let n = (self.data_size as usize).clamp(1, ARGS_LEN);
        let args: Vec<String> = self.arguments[..n].iter().map(|b| format!("{b:02x}")).collect();
        format!(
            "st={:02x} tid={:02x} size={:02x} cmd={:02x}{:02x} args={}",
            self.status,
            self.transaction_id,
            self.data_size,
            self.command_class,
            self.command_id,
            args.join(" ")
        )
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
mod tests {
    use super::*;

    #[test]
    fn layout_matches_openrazer() {
        let r = Report::new(cmd::GET_BATTERY, &[], Some(2), 0x1F).unwrap().to_bytes();
        assert_eq!(&r[..8], &[0, 0x1F, 0, 0, 0, 0x02, 0x07, 0x80]);
        assert_eq!(r[88], 0x02 ^ 0x07 ^ 0x80);
    }

    #[test]
    fn roundtrip() {
        let r = Report::new(cmd::SET_DPI, &[1, 6, 0x40, 6, 0x40, 0, 0], Some(7), 0x1F).unwrap();
        let r2 = Report::from_bytes(&r.to_bytes());
        assert_eq!(r, r2);
        assert_eq!(r2.cmd(), cmd::SET_DPI);
    }
}
