//! Razer Naga Pro — tested module (firmware v2.1, wired 0x008F and HyperSpeed receiver 0x0090).
//!
//! Swappable side plates: 12 buttons (ids 0x40..0x4B), 6 buttons (0x50..0x55), 2 buttons (0x04/0x05).
//! The mouse reports the attached plate with command 0x00B9.

use super::{feat, fx, DeviceSpec, Source, Zone};
use crate::bindings::Action;

pub const COMMON_BUTTONS: &[(u8, &str)] = &[
    (0x01, "Left button"),
    (0x02, "Right button"),
    (0x03, "Middle button (wheel)"),
    (0x04, "Side back"),
    (0x05, "Side forward"),
    (0x09, "Scroll up"),
    (0x0A, "Scroll down"),
    (0x0B, "DPI up (top)"),
    (0x0C, "DPI down (top)"),
    (0x0E, "Profile button (bottom)"),
    (0x34, "Wheel tilt left"),
    (0x35, "Wheel tilt right"),
    (0x60, "DPI down"),
    (0x6A, "DPI up"),
];

/// Plate sizes of swappable side plates.
pub const PLATES: [u8; 3] = [12, 6, 2];

/// Side keypad factory defaults: keys 1..9, 0, -, =
pub const SIDE_KEYS: [u8; 12] = [0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E];

/// Which side plate a button id belongs to:
/// 0x40..=0x4B — 12-button plate, 0x50..=0x55 — 6-button plate, 0x04/0x05 — 2-button plate.
pub fn plate_of(id: u8) -> Option<u8> {
    match id {
        0x40..=0x4B => Some(12),
        0x50..=0x55 => Some(6),
        0x04 | 0x05 => Some(2),
        _ => None,
    }
}

/// Value of command 0x00B9 -> plate size (0 = no plate attached).
pub fn plate_from_code(code: u8) -> Option<u8> {
    match code {
        0 => Some(0),
        1 => Some(2),
        3 => Some(12),
        4 => Some(6),
        _ => None,
    }
}

/// Should button `id` be listed when `plate` (12/6/2) is attached? Non-plate buttons always are.
pub fn button_on_plate(id: u8, plate: u8) -> bool {
    match plate_of(id) {
        Some(p) => p == plate,
        None => true,
    }
}

pub fn side_plate_name(id: u8) -> Option<String> {
    match id {
        0x40..=0x4B => Some(format!("12-plate: {}", id - 0x40 + 1)),
        0x50..=0x55 => Some(format!("6-plate: {}", id - 0x50 + 1)),
        _ => None,
    }
}

/// Factory binding of a button, for "Reset to default".
pub fn default_action(id: u8) -> Option<Action> {
    let dpi = |code: u16| Action { kind: "dpi".into(), code, ..Default::default() };
    Some(match id {
        0x01..=0x05 => Action::mouse(id),
        0x09 => Action::mouse(0x09),
        0x0A => Action::mouse(0x0A),
        0x0B => dpi(0x01),
        0x0C => dpi(0x02),
        0x0E => Action { kind: "profile".into(), code: 0x04, ..Default::default() },
        0x34 => Action::mouse(0x68),
        0x35 => Action::mouse(0x69),
        0x40..=0x4B => Action::keyboard(0, SIDE_KEYS[(id - 0x40) as usize]),
        0x50..=0x55 => Action::keyboard(0, SIDE_KEYS[(id - 0x50) as usize]),
        _ => return None,
    })
}

const ZONE_FX: u16 = fx::OFF | fx::STATIC | fx::SPECTRUM | fx::WAVE | fx::BREATH | fx::REACTIVE;

const ZONES: &[Zone] = &[
    Zone { id: "scroll", led: 0x01, effects: ZONE_FX, direct: true, openrazer: true },
    Zone { id: "logo", led: 0x04, effects: ZONE_FX, direct: true, openrazer: true },
    // 1×3 LED matrix of the side panel ("matrix_*" attributes in OpenRazer)
    Zone {
        id: "side",
        led: 0x00,
        effects: fx::OFF | fx::STATIC | fx::CUSTOM | fx::SPECTRUM | fx::BREATH | fx::REACTIVE,
        direct: true,
        openrazer: true,
    },
];

const DIRECT: u32 = feat::FIRMWARE
    | feat::SERIAL
    | feat::DPI
    | feat::DPI_STAGES
    | feat::POLLING
    | feat::BATTERY
    | feat::CHARGING
    | feat::IDLE
    | feat::LOW_BATTERY
    | feat::LIGHTING
    | feat::BUTTONS
    | feat::MACROS
    | feat::PROFILES
    | feat::SIDE_PLATE;

/// What the OpenRazer driver offers for this model.
const VIA_OPENRAZER: u32 = feat::FIRMWARE
    | feat::SERIAL
    | feat::DPI
    | feat::DPI_STAGES
    | feat::POLLING
    | feat::BATTERY
    | feat::CHARGING
    | feat::IDLE
    | feat::LOW_BATTERY
    | feat::LIGHTING;

const BASE: DeviceSpec = DeviceSpec {
    pid: 0x008F,
    name: "Razer Naga Pro (wired)",
    source: Source::Verified,
    transaction_id: 0x1F,
    interface: 0,
    wireless: false,
    has_battery: true,
    max_dpi: 20000,
    min_dpi: 100,
    max_dpi_stages: 5,
    profiles: 5,
    zones: ZONES,
    reply_delay_ms: 5,
    buttons: COMMON_BUTTONS,
    side_plates: true,
    direct: DIRECT,
    openrazer: VIA_OPENRAZER,
    defaults: default_action,
};

pub static DEVICES: &[DeviceSpec] = &[
    BASE,
    DeviceSpec { pid: 0x0090, name: "Razer Naga Pro (HyperSpeed receiver)", wireless: true, reply_delay_ms: 35, ..BASE },
];

/// Naga V2 Pro: the successor with the same three swappable side plates and the same button ids.
/// Built from this module, not tested on real hardware (`Source::Family`).
pub static FAMILY: &[DeviceSpec] = &[
    DeviceSpec { pid: 0x00A7, name: "Razer Naga V2 Pro (wired)", source: Source::Family, max_dpi: 30000, ..BASE },
    DeviceSpec {
        pid: 0x00A8,
        name: "Razer Naga V2 Pro (HyperSpeed receiver)",
        source: Source::Family,
        max_dpi: 30000,
        wireless: true,
        reply_delay_ms: 35,
        ..BASE
    },
];
