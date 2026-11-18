//! open-rzr hardware layer: how 90-byte reports reach a Razer device.
//!
//! * `transport` — hidraw (Linux), hidapi (Windows), the emulator, the OpenRazer bridge;
//!   `Channel` sends a request and waits for its reply with a strict deadline, retries while the
//!   device is BUSY and reports a sleeping wireless device as [`Error::Asleep`];
//! * `protocol` — the report format (`rzr-protocol`) and the error type of the whole stack;
//! * `logs` — the local log file (never sent anywhere).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version as shown: four numbers, "0.8.1.1". Cargo keeps the fourth one as build metadata
/// ("0.8.1+1"): major (big changes) . minor (new features) . patch (fixes) . optional update
/// (intermediate, device support).
pub fn version() -> String {
    match VERSION.split_once('+') {
        Some((v, b)) => format!("{v}.{b}"),
        None => format!("{VERSION}.0"),
    }
}

/// USB vendor id of Razer.
pub const RAZER_VID: u16 = 0x1532;

#[macro_use]
pub mod logs;
pub mod protocol;
pub mod transport;

pub use protocol::{Error, Result};
pub use transport::{Channel, DeviceInfo, Transport};
