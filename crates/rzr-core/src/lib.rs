//! open-rzr core: Razer mouse protocol and device access, std-only.
//! No network, no telemetry, no accounts.
//!
//! Layout: `protocol` (report format, `rzr-protocol`) → `transport` (`rzr-hal`) (HID / OpenRazer bridge / emulator) →
//! `device` (a connected mouse) + `features/*` (what it can do) + `devices/*` (model modules).
//! Codecs: `bindings` (button actions), `macros`, `lighting`, `hidkeys`.

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

#[macro_use]
extern crate rzr_hal;

pub use rzr_hal::{log_debug, log_error, log_info, log_warn, logs, transport};
/// Device database (tested modules, OpenRazer, description files) — crate `rzr-devices`.
pub use rzr_devices as devices;
pub use rzr_devices::{bindings, hidkeys};
pub mod caps;
pub mod compat;
pub mod device;
pub mod drivers;
pub mod features;
pub mod foreground;
pub mod hotplug;
pub mod lighting;
pub mod macro_doc;
pub mod macros;
pub mod profile;
pub mod program_actions;
pub mod protocol;
pub mod synapse;
pub mod udev;
pub mod ble;
pub mod wireless;

pub use bindings::Action;
pub use device::{find_devices, Backup, ButtonInfo, DpiStages, RazerMouse};
pub use devices::{feat, DeviceSpec, Source, Zone};
pub use lighting::ZoneState;
pub use macros::MacroStep;
pub use profile::Profile;
pub use protocol::{Error, Result};
pub use transport::DeviceInfo;
