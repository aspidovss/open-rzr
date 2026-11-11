//! Razer HID control protocol — the report format lives in the `rzr-protocol` crate (no_std);
//! this module adds the error type of the whole stack (hardware layer, core, apps).

pub use rzr_protocol::{cmd, crc, polling_code, polling_hz, ProtocolError, Report, ARGS_LEN, REPORT_LEN};

pub mod status {
    pub use rzr_protocol::status::{label, BUSY, FAIL, NEW, NOT_SUPPORTED, OK, TIMEOUT};

    /// "BUSY", "OK", … or "0x42".
    pub fn name(s: u8) -> String {
        label(s).map(String::from).unwrap_or_else(|| format!("0x{s:02x}"))
    }
}

/// Errors of the whole stack. The messages are shown to the user; `code()` gives the UI a stable key.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// I/O with the device or the OS failed.
    #[error("{0}")]
    Io(String),
    /// The mouse answered with a non-OK status.
    #[error("command 0x{cmd:04x} failed: {}", status_name(.status))]
    Failed { cmd: u16, status: u8 },
    /// The reply belonged to another command (another program talks to the mouse).
    #[error(
        "The reply belongs to another command: another program is talking to the mouse \
         (Razer Synapse / OpenRazer). Close it."
    )]
    Conflict,
    /// No device / nothing to open.
    #[error("{0}")]
    NotFound(String),
    /// Bad argument from the caller.
    #[error("{0}")]
    Invalid(String),
    /// The OS denied access to the device (Linux: udev rule missing).
    #[error("{0}")]
    NoAccess(String),
    /// the mouse (or the way it is connected) has no such feature
    #[error("{0}")]
    Unsupported(String),
    /// A wireless device does not answer: asleep, switched off or out of range of its receiver.
    /// Not a fault — the request can be repeated when it wakes up.
    #[error("The mouse is asleep or out of range of its receiver. Move it to wake it up.")]
    Asleep,
    /// No reply in time (`cmd`, waited `ms`).
    #[error("no reply to command 0x{cmd:04x} within {ms} ms")]
    Timeout { cmd: u16, ms: u64 },
}

fn status_name(s: &u8) -> String {
    status::name(*s)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_and_codes() {
        assert_eq!(Error::Failed { cmd: 0x0081, status: status::NOT_SUPPORTED }.to_string(), "command 0x0081 failed: NOT_SUPPORTED");
        assert_eq!(Error::Failed { cmd: 0x020c, status: 0x42 }.to_string(), "command 0x020c failed: 0x42");
        assert_eq!(Error::Timeout { cmd: 0x0780, ms: 2000 }.to_string(), "no reply to command 0x0780 within 2000 ms");
        assert_eq!(Error::Invalid("bad".into()).to_string(), "bad");
        assert!(Error::Conflict.to_string().starts_with("The reply belongs to another command: another program is talking to the mouse (Razer"));
        assert!(Error::Asleep.to_string().contains("asleep"));
        assert_eq!((Error::Asleep.code(), Error::Conflict.code()), ("asleep", "conflict"));
        let e: Error = std::io::Error::other("gone").into();
        assert_eq!((e.code(), e.to_string()), ("io", "gone".to_string()));
        let e: Box<dyn std::error::Error> = Box::new(Error::Asleep);
        assert!(!e.to_string().is_empty());
    }
}
