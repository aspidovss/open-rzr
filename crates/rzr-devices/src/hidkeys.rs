//! USB HID usage tables (Keyboard page 0x07, Consumer page 0x0C) with readable names.

pub fn key_name(code: u8) -> String {
    match code {
        0x04..=0x1D => ((b'A' + (code - 0x04)) as char).to_string(),
        0x1E..=0x26 => ((b'1' + (code - 0x1E)) as char).to_string(),
        0x27 => "0".into(),
        0x3A..=0x45 => format!("F{}", code - 0x3A + 1),
        0x68..=0x73 => format!("F{}", code - 0x68 + 13),
        0x59..=0x61 => format!("KP {}", code - 0x59 + 1),
        0x62 => "KP 0".into(),
        _ => NAMED
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| format!("0x{code:02x}")),
    }
}

const NAMED: &[(u8, &str)] = &[
    (0x28, "Enter"), (0x29, "Esc"), (0x2A, "Backspace"), (0x2B, "Tab"), (0x2C, "Space"),
    (0x2D, "-"), (0x2E, "="), (0x2F, "["), (0x30, "]"), (0x31, "\\"), (0x33, ";"), (0x34, "'"),
    (0x35, "`"), (0x36, ","), (0x37, "."), (0x38, "/"), (0x39, "CapsLock"),
    (0x46, "PrintScreen"), (0x47, "ScrollLock"), (0x48, "Pause"), (0x49, "Insert"),
    (0x4A, "Home"), (0x4B, "PageUp"), (0x4C, "Delete"), (0x4D, "End"), (0x4E, "PageDown"),
    (0x4F, "Right"), (0x50, "Left"), (0x51, "Down"), (0x52, "Up"), (0x53, "NumLock"),
    (0x54, "KP /"), (0x55, "KP *"), (0x56, "KP -"), (0x57, "KP +"), (0x58, "KP Enter"),
    (0x63, "KP ."), (0x65, "Menu"),
    (0xE0, "LCtrl"), (0xE1, "LShift"), (0xE2, "LAlt"), (0xE3, "LWin"),
    (0xE4, "RCtrl"), (0xE5, "RShift"), (0xE6, "RAlt"), (0xE7, "RWin"),
];

/// Every key code that has a name (for pickers).
pub fn all_keys() -> Vec<(u8, String)> {
    let mut v: Vec<(u8, String)> = Vec::new();
    for c in 0x04u8..=0x73 {
        let n = key_name(c);
        if !n.starts_with("0x") {
            v.push((c, n));
        }
    }
    for c in 0xE0u8..=0xE7 {
        v.push((c, key_name(c)));
    }
    v
}

pub fn key_by_name(name: &str) -> Option<u8> {
    let n = name.trim().to_ascii_lowercase();
    let alias = match n.as_str() {
        "ctrl" => "lctrl",
        "shift" => "lshift",
        "alt" => "lalt",
        "win" | "super" | "meta" => "lwin",
        "altgr" => "ralt",
        "esc" | "escape" => "esc",
        "space" => "space",
        other => other,
    };
    if let Some(h) = alias.strip_prefix("0x") {
        return u8::from_str_radix(h, 16).ok();
    }
    all_keys().into_iter().find(|(_, k)| k.to_ascii_lowercase() == alias).map(|(c, _)| c)
}

pub const MODIFIERS: &[(u8, &str)] = &[
    (0x01, "LCtrl"), (0x02, "LShift"), (0x04, "LAlt"), (0x08, "LWin"),
    (0x10, "RCtrl"), (0x20, "RShift"), (0x40, "RAlt"), (0x80, "RWin"),
];

/// Modifier key code (0xE0..0xE7) -> modifier bit.
pub fn modifier_bit(code: u8) -> Option<u8> {
    if (0xE0..=0xE7).contains(&code) {
        Some(1 << (code - 0xE0))
    } else {
        None
    }
}

pub fn modifiers_name(mask: u8) -> String {
    MODIFIERS
        .iter()
        .filter(|(b, _)| mask & b != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("+")
}

pub fn combo_name(mods: u8, key: u8) -> String {
    let m = modifiers_name(mods);
    if m.is_empty() {
        key_name(key)
    } else if key == 0 {
        m
    } else {
        format!("{m}+{}", key_name(key))
    }
}

/// "Ctrl+Shift+C" -> (modifier mask, key code)
pub fn parse_combo(text: &str) -> Result<(u8, u8), String> {
    let (mut mods, mut key) = (0u8, 0u8);
    for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        let code = key_by_name(part).ok_or_else(|| format!("unknown key: {part}"))?;
        match modifier_bit(code) {
            Some(b) => mods |= b,
            None => key = code,
        }
    }
    Ok((mods, key))
}

pub const CONSUMER: &[(u16, &str)] = &[
    (0x00B5, "Next track"), (0x00B6, "Previous track"), (0x00B7, "Stop"),
    (0x00CD, "Play/Pause"), (0x00E2, "Mute"), (0x00E9, "Volume up"),
    (0x00EA, "Volume down"), (0x006F, "Brightness up"), (0x0070, "Brightness down"),
    (0x0183, "Media player"), (0x018A, "Mail"), (0x0192, "Calculator"),
    (0x0194, "File explorer"), (0x0221, "Search"), (0x0223, "Browser: home"),
    (0x0224, "Browser: back"), (0x0225, "Browser: forward"), (0x0227, "Browser: refresh"),
];

pub fn consumer_name(code: u16) -> String {
    CONSUMER
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| format!("0x{code:04x}"))
}

/// Values used for mouse-button bindings.
pub const MOUSE_FN: &[(u8, &str)] = &[
    (0x01, "Left"), (0x02, "Right"), (0x03, "Middle"), (0x04, "Back"), (0x05, "Forward"),
    (0x09, "Scroll up"), (0x0A, "Scroll down"), (0x68, "Scroll left"), (0x69, "Scroll right"),
];

pub fn mouse_name(b: u8) -> String {
    MOUSE_FN
        .iter()
        .find(|(c, _)| *c == b)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| format!("0x{b:02x}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn names_and_combo() {
        assert_eq!(key_name(0x04), "A");
        assert_eq!(key_name(0x27), "0");
        assert_eq!(key_name(0x3A), "F1");
        assert_eq!(key_name(0x2E), "=");
        assert_eq!(parse_combo("ctrl+shift+c").unwrap(), (0x03, 0x06));
        assert_eq!(parse_combo("F5").unwrap(), (0, 0x3E));
        assert_eq!(combo_name(0x01, 0x06), "LCtrl+C");
        assert!(parse_combo("nope").is_err());
    }
}
