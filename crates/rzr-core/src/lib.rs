//! open-rzr core: Razer mouse protocol and device access, std-only.
//! No network, no telemetry, no accounts.
//!
//! Layout: `protocol` (report format, `rzr-protocol`) → `transport` (`rzr-hal`) (HID / OpenRazer bridge / emulator) →
//! `device` (a connected mouse) + `features/*` (what it can do) + `devices/*` (model modules).
//! Codecs: `bindings` (button actions), `macros`, `lighting`, `hidkeys`.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[macro_use]
extern crate rzr_hal;

pub use rzr_hal::{log_debug, log_error, log_info, log_warn, logs, transport};
pub mod bindings;
pub mod compat;
pub mod device;
pub mod drivers;
pub mod devices;
pub mod features;
pub mod foreground;
pub mod hidkeys;
pub mod hotplug;
pub mod lighting;
pub mod macro_doc;
pub mod macros;
pub mod profile;
pub mod program_actions;
pub mod protocol;
pub mod synapse;
pub mod udev;
pub mod wireless;

pub use bindings::Action;
pub use device::{find_devices, Backup, ButtonInfo, DpiStages, RazerMouse};
pub use devices::{feat, DeviceSpec, Source, Zone};
pub use lighting::ZoneState;
pub use macros::MacroStep;
pub use profile::Profile;
pub use protocol::{Error, Result};
pub use transport::DeviceInfo;
