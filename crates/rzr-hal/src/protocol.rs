//! Razer HID control protocol — the report format lives in the `rzr-protocol` crate (no_std);
//! this module adds the error type of the whole stack (hardware layer, core, apps).

use std::fmt;

pub use rzr_protocol::{cmd, crc, polling_code, polling_hz, ProtocolError, Report, ARGS_LEN, REPORT_LEN};

pub mod status {
    pub use rzr_protocol::status::{label, BUSY, FAIL, NEW, NOT_SUPPORTED, OK, TIMEOUT};

    /// "BUSY", "OK", … or "0x42".
    pub fn name(s: u8) -> String {
        label(s).map(String::from).unwrap_or_else(|| format!("0x{s:02x}"))
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
    /// A wireless device does not answer: asleep, switched off or out of range of its receiver.
    /// Not a fault — the request can be repeated when it wakes up.
    Asleep,
    /// No reply in time (`cmd`, waited `ms`).
    Timeout { cmd: u16, ms: u64 },
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
            Error::Asleep => write!(f, "The mouse is asleep or out of range of its receiver. Move it to wake it up."),
            Error::Timeout { cmd, ms } => write!(f, "no reply to command 0x{cmd:04x} within {ms} ms"),
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
    /// Stable code for the UI to translate the message: io, failed, conflict, not_found, invalid,
    /// no_access, unsupported, asleep, timeout.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io(_) => "io",
            Error::Failed { .. } => "failed",
            Error::Conflict => "conflict",
            Error::NotFound(_) => "not_found",
            Error::Invalid(_) => "invalid",
            Error::NoAccess(_) => "no_access",
            Error::Unsupported(_) => "unsupported",
            Error::Asleep => "asleep",
            Error::Timeout { .. } => "timeout",
        }
    }

    /// True when the mouse simply does not support/accept this command.
    pub fn is_failed(&self) -> bool {
        matches!(self, Error::Failed { .. })
    }
}

impl From<ProtocolError> for Error {
    fn from(e: ProtocolError) -> Self {
        Error::Invalid(e.to_string())
    }
}
