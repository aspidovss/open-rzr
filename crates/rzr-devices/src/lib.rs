//! Device modules: what each supported mouse is and what it can do.
//!
//! * `naga_pro.rs`     — hand-written, tested module (buttons, side plates, macros, profiles, lighting).
//! * `openrazer_db.rs` — generated from the OpenRazer project (tools/openrazer-import): every mouse
//!   OpenRazer knows, with the features open-rzr can use directly and through the OpenRazer driver.
//!
//! * `external.rs`     — descriptions from `*.toml` / `*.json` files in the config folder
//!   (new models without recompiling), starting from an OpenRazer entry or a known model.
//!
//! Order: hand-written module → file → OpenRazer database (a file with `override = true` goes
//! before the module). To add a mouse properly: copy `naga_pro.rs`, describe the model and add it
//! to `MODULES`.
//!
//! Also here: `bindings` (button actions as the mouse stores them) and `hidkeys` (key names).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::bindings::Action;

pub mod bindings;
pub mod conf;
pub mod external;
pub mod generic;
pub mod hidkeys;
pub mod naga_pro;
pub mod openrazer_db;

pub use naga_pro::{button_on_plate, plate_from_code, plate_of, side_plate_name, COMMON_BUTTONS, PLATES};

/// USB vendor id of Razer (same as `rzr_hal::RAZER_VID`).
pub const RAZER_VID: u16 = 0x1532;

/// Where a device description comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Own module, tested on real hardware.
    Verified,
    /// Generated from the OpenRazer device database, not tested by open-rzr.
    OpenRazer,
    /// Hand-written from a tested sibling (same protocol, same buttons), not tested itself.
    Family,
    /// From a description file of the user (`external.rs`).
    User,
}

impl Source {
    pub fn code(self) -> &'static str {
        match self {
            Source::Verified => "verified",
            Source::OpenRazer => "openrazer",
            Source::Family => "family",
            Source::User => "user",
        }
    }
}

/// Feature flags (`DeviceSpec::direct`, `DeviceSpec::openrazer`).
pub mod feat {
    pub const FIRMWARE: u32 = 1 << 0;
    pub const SERIAL: u32 = 1 << 1;
    pub const DPI: u32 = 1 << 2;
    pub const DPI_STAGES: u32 = 1 << 3;
    pub const POLLING: u32 = 1 << 4;
    pub const BATTERY: u32 = 1 << 5;
    pub const CHARGING: u32 = 1 << 6;
    pub const IDLE: u32 = 1 << 7;
    pub const LOW_BATTERY: u32 = 1 << 8;
    pub const LIGHTING: u32 = 1 << 9;
    pub const BUTTONS: u32 = 1 << 10;
    pub const MACROS: u32 = 1 << 11;
    pub const PROFILES: u32 = 1 << 12;
    pub const SIDE_PLATE: u32 = 1 << 13;

    pub const ALL: &[(u32, &str)] = &[
        (FIRMWARE, "firmware"),
        (SERIAL, "serial"),
        (DPI, "dpi"),
        (DPI_STAGES, "dpi_stages"),
        (POLLING, "polling"),
        (BATTERY, "battery"),
        (CHARGING, "charging"),
        (IDLE, "idle"),
        (LOW_BATTERY, "low_battery"),
        (LIGHTING, "lighting"),
        (BUTTONS, "buttons"),
        (MACROS, "macros"),
        (PROFILES, "profiles"),
        (SIDE_PLATE, "side_plate"),
    ];

    pub fn names(mask: u32) -> Vec<&'static str> {
        ALL.iter().filter(|(f, _)| mask & f != 0).map(|(_, n)| *n).collect()
    }
}

/// Lighting effect flags (`Zone::effects`).
pub mod fx {
    pub const OFF: u16 = 1 << 0;
    pub const STATIC: u16 = 1 << 1;
    pub const SPECTRUM: u16 = 1 << 2;
    pub const WAVE: u16 = 1 << 3;
    /// breathing: one colour, two colours, random
    pub const BREATH: u16 = 1 << 4;
    pub const REACTIVE: u16 = 1 << 5;
    /// own colour per LED
    pub const CUSTOM: u16 = 1 << 6;

    /// open-rzr effect names covered by a flag
    pub fn names(mask: u16) -> Vec<&'static str> {
        let mut v = Vec::new();
        let mut add = |f: u16, n: &[&'static str]| {
            if mask & f != 0 {
                v.extend_from_slice(n);
            }
        };
        add(OFF, &["off"]);
        add(STATIC, &["static"]);
        add(CUSTOM, &["custom"]);
        add(SPECTRUM, &["spectrum"]);
        add(WAVE, &["wave"]);
        add(BREATH, &["breath_single", "breath_dual", "breath_random"]);
        add(REACTIVE, &["reactive"]);
        v
    }
}

/// A lighting zone of a mouse.
#[derive(Debug, Clone, Copy)]
pub struct Zone {
    /// "scroll" | "logo" | "side" (Naga side panel) | "left" | "right" | "backlight" | "matrix"
    pub id: &'static str,
    /// LED id in Razer lighting commands
    pub led: u8,
    pub effects: u16,
    /// open-rzr can drive it directly
    pub direct: bool,
    /// the OpenRazer driver can drive it
    pub openrazer: bool,
}

#[derive(Debug, Clone)]
pub struct DeviceSpec {
    pub pid: u16,
    pub name: &'static str,
    pub source: Source,
    pub transaction_id: u8,
    /// HID interface that accepts control reports.
    pub interface: i32,
    pub wireless: bool,
    pub has_battery: bool,
    pub max_dpi: u16,
    pub min_dpi: u16,
    pub max_dpi_stages: usize,
    /// On-board profiles 1..=N (0 = direct/temporary).
    pub profiles: u8,
    pub zones: &'static [Zone],
    /// Milliseconds to wait before reading a reply.
    pub reply_delay_ms: u64,
    /// Known button ids with names; others are discovered by scanning.
    pub buttons: &'static [(u8, &'static str)],
    /// Swappable side plates (Naga Pro): the mouse reports which one is attached.
    pub side_plates: bool,
    /// Features open-rzr sends itself over HID (`feat::*`).
    pub direct: u32,
    /// Features available through the OpenRazer kernel driver on Linux (`feat::*`).
    pub openrazer: u32,
    /// Factory binding of a button, for "Reset to default".
    pub defaults: fn(u8) -> Option<Action>,
    /// The mouse's Bluetooth identity (its HID collections are tried, vendor-defined ones first).
    pub bluetooth: bool,
}

/// Common values of generated OpenRazer entries.
pub const OPENRAZER_BASE: DeviceSpec = DeviceSpec {
    pid: 0,
    name: "",
    source: Source::OpenRazer,
    transaction_id: 0xFF,
    interface: 0,
    wireless: false,
    has_battery: false,
    max_dpi: 16000,
    min_dpi: 100,
    max_dpi_stages: 5,
    profiles: 0,
    zones: &[],
    reply_delay_ms: 5,
    buttons: generic::BUTTONS,
    side_plates: false,
    direct: 0,
    openrazer: 0,
    defaults: generic::default_action,
    bluetooth: false,
};

/// Hand-written device modules, checked before the OpenRazer database.
pub static MODULES: &[&[DeviceSpec]] = &[naga_pro::DEVICES, naga_pro::BLUETOOTH, naga_pro::FAMILY];

impl DeviceSpec {
    pub fn zone(&self, id: &str) -> Option<&'static Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    pub fn verified(&self) -> bool {
        self.source == Source::Verified
    }
}

/// Description of a product id: own module, description file, OpenRazer database.
pub fn spec_for(pid: u16) -> Option<&'static DeviceSpec> {
    if external::overrides(pid) {
        return external::find(pid);
    }
    MODULES
        .iter()
        .flat_map(|m| m.iter())
        .find(|d| d.pid == pid)
        .or_else(|| external::find(pid))
        .or_else(|| openrazer_db::DEVICES.iter().find(|d| d.pid == pid))
}

/// Every known device, one entry per product id (in the order of `spec_for`).
pub fn all() -> Vec<&'static DeviceSpec> {
    let mut v: Vec<&'static DeviceSpec> = external::all().into_iter().filter(|d| external::overrides(d.pid)).collect();
    for d in MODULES.iter().flat_map(|m| m.iter()).chain(external::all()) {
        if !v.iter().any(|x| x.pid == d.pid) {
            v.push(d);
        }
    }
    for d in openrazer_db::DEVICES {
        if !v.iter().any(|x| x.pid == d.pid) {
            v.push(d);
        }
    }
    v.sort_by_key(|d| d.pid);
    v
}

pub fn button_name(spec: &DeviceSpec, id: u8) -> Option<String> {
    spec.buttons
        .iter()
        .find(|(b, _)| *b == id)
        .map(|(_, n)| n.to_string())
        .or_else(|| if spec.side_plates { side_plate_name(id) } else { None })
        .or_else(|| generic::family_button_name(spec.name, id))
}

/// Factory binding of a Naga Pro button (kept for older callers; prefer `spec.defaults`).
pub fn default_action(id: u8) -> Option<Action> {
    naga_pro::default_action(id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn own_module_wins_and_db_is_sane() {
        let n = spec_for(0x008F).unwrap();
        assert_eq!(n.source, Source::Verified);
        assert!(n.direct & feat::BUTTONS != 0);
        assert!(openrazer_db::DEVICES.len() > 50);
        for d in openrazer_db::DEVICES {
            assert!(d.max_dpi >= 400, "{:04x}", d.pid);
            assert!(!d.name.is_empty());
            // buttons, macros and profiles of database models are found by probing (generic.rs)
            assert_eq!(d.direct & (feat::BUTTONS | feat::MACROS | feat::PROFILES), 0);
            for z in d.zones {
                assert!(fx::names(z.effects).len() > 0);
            }
        }
        // the generated entry for the Naga Pro agrees with the tested module
        let g = openrazer_db::DEVICES.iter().find(|d| d.pid == 0x008F).unwrap();
        assert_eq!(g.transaction_id, n.transaction_id);
        assert!(g.direct & feat::DPI_STAGES != 0);
        let mut pids: Vec<u16> = all().iter().map(|d| d.pid).collect();
        let len = pids.len();
        pids.dedup();
        assert_eq!(len, pids.len());
    }
}
