//! Button actions <-> 7-byte on-board binding records: [function_class, data_len, data0..data4].

use crate::hidkeys;

pub const DISABLED: u8 = 0x00;
pub const MOUSE: u8 = 0x01;
pub const KEYBOARD: u8 = 0x02;
pub const MACRO_FIXED: u8 = 0x03;
pub const MACRO_HOLD: u8 = 0x04;
pub const MACRO_TOGGLE: u8 = 0x05;
pub const DPI: u8 = 0x06;
pub const PROFILE: u8 = 0x07;
pub const SYSTEM: u8 = 0x09;
pub const CONSUMER: u8 = 0x0A;
pub const DOUBLE_CLICK: u8 = 0x0B;
pub const HYPERSHIFT: u8 = 0x0C;
pub const KEYBOARD_TURBO: u8 = 0x0D;
pub const MOUSE_TURBO: u8 = 0x0E;
pub const MACRO_SEQUENCE: u8 = 0x0F;
pub const SCROLL_MODE: u8 = 0x12;

pub const DPI_FIXED: u8 = 0x03;
pub const DPI_AIM: u8 = 0x05;
pub const PROFILE_FIXED: u8 = 0x03;

pub const KINDS: &[(&str, &str)] = &[
    ("keyboard", "Key / shortcut"),
    ("keyboard_turbo", "Turbo key"),
    ("mouse", "Mouse button"),
    ("double_click", "Double click"),
    ("mouse_turbo", "Turbo click"),
    ("macro", "Macro"),
    ("dpi", "DPI switch"),
    ("profile", "Profile switch"),
    ("consumer", "Media key"),
    ("hypershift", "Hypershift (hold)"),
    ("system", "PC power"),
    ("scroll_mode", "Scroll mode"),
    ("disabled", "Disabled"),
    ("raw", "Raw bytes"),
];

pub const DPI_OPS: &[(u8, &str)] = &[
    (0x01, "DPI: stage up"),
    (0x02, "DPI: stage down"),
    (0x06, "DPI: cycle up"),
    (0x07, "DPI: cycle down"),
    (DPI_FIXED, "DPI: specific stage"),
    (DPI_AIM, "Sniper (temporary DPI)"),
];
pub const PROFILE_OPS: &[(u8, &str)] = &[
    (0x01, "Profile: next"),
    (0x02, "Profile: previous"),
    (0x04, "Profile: cycle up"),
    (0x05, "Profile: cycle down"),
    (PROFILE_FIXED, "Specific profile"),
];
pub const SYSTEM_OPS: &[(u8, &str)] = &[(0x01, "Shut down"), (0x02, "Sleep"), (0x04, "Wake up")];
pub const MACRO_MODES: &[(&str, &str)] = &[
    ("once", "N times"),
    ("hold", "while held"),
    ("toggle", "toggle"),
    ("sequence", "step by step"),
];

fn lookup<K: PartialEq + Copy>(t: &[(K, &str)], k: K) -> Option<String> {
    t.iter().find(|(a, _)| *a == k).map(|(_, n)| n.to_string())
}

fn macro_mode_code(mode: &str) -> Option<u8> {
    match mode {
        "once" => Some(MACRO_FIXED),
        "hold" => Some(MACRO_HOLD),
        "toggle" => Some(MACRO_TOGGLE),
        "sequence" => Some(MACRO_SEQUENCE),
        _ => None,
    }
}

/// A button action. Flat struct (like a form) so it maps 1:1 to the UI and JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(default))]
pub struct Action {
    pub kind: String,
    pub button: u8,
    pub modifiers: u8,
    pub key: u8,
    pub interval: u16,
    /// consumer usage / system op / dpi op / profile op
    pub code: u16,
    pub macro_id: u16,
    pub macro_mode: String,
    pub repeat: u8,
    pub stage: u8,
    pub dpi_x: u16,
    pub dpi_y: u16,
    pub profile: u8,
    pub raw: Vec<u8>,
}

impl Default for Action {
    fn default() -> Self {
        Action {
            kind: "disabled".into(),
            button: 1,
            modifiers: 0,
            key: 0,
            interval: 100,
            code: 0,
            macro_id: 0,
            macro_mode: "once".into(),
            repeat: 1,
            stage: 1,
            dpi_x: 400,
            dpi_y: 400,
            profile: 1,
            raw: Vec::new(),
        }
    }
}

/// A record the program does not understand ("raw") is written back as it was read, so it must at
/// least be well-formed: a known type with the length that type has. A turbo record with a wrong
/// length (e.g. `0e 01 09 00 14`) once stopped the wheel tilt of a Naga Pro until it was re-plugged.
pub fn check_record(r: &[u8; 7]) -> Result<(), String> {
    let lens: &[u8] = match r[0] {
        DISABLED => &[0],
        MOUSE | DOUBLE_CLICK | SYSTEM | HYPERSHIFT | SCROLL_MODE => &[1],
        KEYBOARD | CONSUMER | MACRO_HOLD | MACRO_TOGGLE | MACRO_SEQUENCE => &[2],
        MOUSE_TURBO | MACRO_FIXED => &[3],
        KEYBOARD_TURBO => &[4],
        DPI => &[1, 2, 5],
        PROFILE => &[1, 2],
        // a type this program does not know: only the length can be checked
        _ => &[0, 1, 2, 3, 4, 5],
    };
    if lens.contains(&r[1]) {
        Ok(())
    } else {
        Err(format!(
            "malformed record {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}: type 0x{:02x} with length {}",
            r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[0], r[1]
        ))
    }
}

impl Action {
    pub fn kind(kind: &str) -> Action {
        Action { kind: kind.into(), ..Default::default() }
    }
    pub fn keyboard(modifiers: u8, key: u8) -> Action {
        Action { kind: "keyboard".into(), modifiers, key, ..Default::default() }
    }
    pub fn mouse(button: u8) -> Action {
        Action { kind: "mouse".into(), button, ..Default::default() }
    }

    pub fn encode(&self) -> Result<[u8; 7], String> {
        let (fn_, data): (u8, Vec<u8>) = match self.kind.as_str() {
            "disabled" => (DISABLED, vec![]),
            "mouse" => (MOUSE, vec![self.button]),
            "double_click" => (DOUBLE_CLICK, vec![self.button]),
            "mouse_turbo" => {
                let i = self.interval.to_be_bytes();
                (MOUSE_TURBO, vec![self.button, i[0], i[1]])
            }
            "keyboard" => (KEYBOARD, vec![self.modifiers, self.key]),
            "keyboard_turbo" => {
                let i = self.interval.to_be_bytes();
                (KEYBOARD_TURBO, vec![self.modifiers, self.key, i[0], i[1]])
            }
            "consumer" => (CONSUMER, self.code.to_be_bytes().to_vec()),
            "system" => (SYSTEM, vec![self.code as u8]),
            "macro" => {
                let f = macro_mode_code(&self.macro_mode)
                    .ok_or_else(|| format!("unknown macro mode {}", self.macro_mode))?;
                let id = self.macro_id.to_be_bytes();
                if f == MACRO_FIXED {
                    (f, vec![id[0], id[1], self.repeat])
                } else {
                    (f, vec![id[0], id[1]])
                }
            }
            "dpi" => {
                let op = self.code as u8;
                if op == DPI_FIXED {
                    (DPI, vec![DPI_FIXED, self.stage])
                } else if op == DPI_AIM {
                    let x = self.dpi_x.to_be_bytes();
                    let y = self.dpi_y.to_be_bytes();
                    (DPI, vec![DPI_AIM, x[0], x[1], y[0], y[1]])
                } else {
                    (DPI, vec![if op == 0 { 0x06 } else { op }])
                }
            }
            "profile" => {
                let op = self.code as u8;
                if op == PROFILE_FIXED {
                    (PROFILE, vec![PROFILE_FIXED, self.profile])
                } else {
                    (PROFILE, vec![if op == 0 { 0x04 } else { op }])
                }
            }
            "hypershift" => (HYPERSHIFT, vec![0x01]),
            "scroll_mode" => (SCROLL_MODE, vec![0x01]),
            "raw" => {
                let mut r = [0u8; 7];
                let n = self.raw.len().min(7);
                r[..n].copy_from_slice(&self.raw[..n]);
                return Ok(r);
            }
            k => return Err(format!("unknown action kind {k}")),
        };
        if data.len() > 5 {
            return Err("binding too long".into());
        }
        let mut r = [0u8; 7];
        r[0] = fn_;
        r[1] = data.len() as u8;
        r[2..2 + data.len()].copy_from_slice(&data);
        Ok(r)
    }

    pub fn decode(rec: &[u8]) -> Action {
        let mut r = [0u8; 7];
        let n = rec.len().min(7);
        r[..n].copy_from_slice(&rec[..n]);
        let (f, len) = (r[0], r[1] as usize);
        let d = &r[2..2 + len.min(5)];
        let be = |a: u8, b: u8| u16::from_be_bytes([a, b]);
        let a = match f {
            DISABLED => Some(Action::kind("disabled")),
            MOUSE if len >= 1 => Some(Action::mouse(d[0])),
            DOUBLE_CLICK if len >= 1 => Some(Action { kind: "double_click".into(), button: d[0], ..Default::default() }),
            MOUSE_TURBO if len >= 3 => Some(Action {
                kind: "mouse_turbo".into(), button: d[0], interval: be(d[1], d[2]), ..Default::default()
            }),
            KEYBOARD if len >= 2 => Some(Action::keyboard(d[0], d[1])),
            KEYBOARD_TURBO if len >= 4 => Some(Action {
                kind: "keyboard_turbo".into(), modifiers: d[0], key: d[1], interval: be(d[2], d[3]), ..Default::default()
            }),
            // turbo records with a wrong length byte (seen from older Synapse profiles, e.g.
            // "0e 01 09 00 14"): the data is complete, only the length is wrong. Read them as turbo,
            // so they are written back in the correct form.
            MOUSE_TURBO if len >= 1 && be(r[3], r[4]) != 0 => Some(Action {
                kind: "mouse_turbo".into(), button: r[2], interval: be(r[3], r[4]), ..Default::default()
            }),
            KEYBOARD_TURBO if len >= 1 && be(r[4], r[5]) != 0 => Some(Action {
                kind: "keyboard_turbo".into(), modifiers: r[2], key: r[3], interval: be(r[4], r[5]), ..Default::default()
            }),
            CONSUMER if len >= 2 => Some(Action { kind: "consumer".into(), code: be(d[0], d[1]), ..Default::default() }),
            SYSTEM if len >= 1 => Some(Action { kind: "system".into(), code: d[0] as u16, ..Default::default() }),
            MACRO_FIXED | MACRO_HOLD | MACRO_TOGGLE | MACRO_SEQUENCE if len >= 2 => {
                let mode = match f {
                    MACRO_FIXED => "once",
                    MACRO_HOLD => "hold",
                    MACRO_TOGGLE => "toggle",
                    _ => "sequence",
                };
                Some(Action {
                    kind: "macro".into(),
                    macro_id: be(d[0], d[1]),
                    macro_mode: mode.into(),
                    repeat: if f == MACRO_FIXED && len >= 3 { d[2] } else { 1 },
                    ..Default::default()
                })
            }
            DPI if len >= 1 => {
                if len >= 5 && d[0] == DPI_AIM {
                    Some(Action { kind: "dpi".into(), code: DPI_AIM as u16, dpi_x: be(d[1], d[2]), dpi_y: be(d[3], d[4]), ..Default::default() })
                } else if len >= 2 && d[0] == DPI_FIXED {
                    Some(Action { kind: "dpi".into(), code: DPI_FIXED as u16, stage: d[1], ..Default::default() })
                } else {
                    Some(Action { kind: "dpi".into(), code: d[0] as u16, ..Default::default() })
                }
            }
            PROFILE if len >= 1 => {
                if len >= 2 && d[0] == PROFILE_FIXED {
                    Some(Action { kind: "profile".into(), code: PROFILE_FIXED as u16, profile: d[1], ..Default::default() })
                } else {
                    Some(Action { kind: "profile".into(), code: d[0] as u16, ..Default::default() })
                }
            }
            HYPERSHIFT => Some(Action::kind("hypershift")),
            SCROLL_MODE => Some(Action::kind("scroll_mode")),
            _ => None,
        };
        a.unwrap_or_else(|| Action { kind: "raw".into(), raw: r.to_vec(), ..Default::default() })
    }

    pub fn describe(&self) -> String {
        match self.kind.as_str() {
            "disabled" => "— disabled —".into(),
            "mouse" => format!("Mouse: {}", hidkeys::mouse_name(self.button)),
            "double_click" => format!("Double click: {}", hidkeys::mouse_name(self.button)),
            "mouse_turbo" => format!("Turbo {} / {} ms", hidkeys::mouse_name(self.button), self.interval),
            "keyboard" => format!("Key: {}", hidkeys::combo_name(self.modifiers, self.key)),
            "keyboard_turbo" => format!("Turbo: {} / {} ms", hidkeys::combo_name(self.modifiers, self.key), self.interval),
            "consumer" => format!("Media: {}", hidkeys::consumer_name(self.code)),
            "system" => format!("Power: {}", lookup(SYSTEM_OPS, self.code as u8).unwrap_or_else(|| format!("0x{:02x}", self.code))),
            "macro" => {
                let mode = lookup(MACRO_MODES, self.macro_mode.as_str()).unwrap_or_default();
                let extra = if self.macro_mode == "once" { format!(" ×{}", self.repeat) } else { String::new() };
                format!("Macro #{} ({mode}{extra})", self.macro_id)
            }
            "dpi" => match self.code as u8 {
                DPI_FIXED => format!("DPI: stage {}", self.stage),
                DPI_AIM => format!("Sniper: {} DPI (while held)", self.dpi_x),
                op => lookup(DPI_OPS, op).unwrap_or_else(|| format!("DPI op 0x{op:02x}")),
            },
            "profile" => match self.code as u8 {
                PROFILE_FIXED => format!("Profile {}", self.profile),
                op => lookup(PROFILE_OPS, op).unwrap_or_else(|| format!("Profile op 0x{op:02x}")),
            },
            "hypershift" => "Hypershift".into(),
            "scroll_mode" => "Scroll mode toggle".into(),
            _ => format!(
                "Raw bytes: {}",
                self.raw.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn hex(b: [u8; 7]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn reference_examples() {
        // razerqdhid docs: Ctrl+C = "02 02 01 06 00 00 00", mouse turbo wheel-up 100 ms = "0e 03 09 00 64 00 00"
        assert_eq!(hex(Action::keyboard(1, 0x06).encode().unwrap()), "02020106000000");
        // a turbo record with a wrong length byte is repaired
        let fixed = Action::decode(&[0x0e, 0x01, 0x09, 0x00, 0x14, 0x00, 0x00]);
        assert_eq!((fixed.kind.as_str(), fixed.button, fixed.interval), ("mouse_turbo", 9, 20));
        assert_eq!(fixed.encode().unwrap(), [0x0e, 0x03, 0x09, 0x00, 0x14, 0x00, 0x00]);
        let k = Action::decode(&[0x0d, 0x02, 0x00, 0x04, 0x00, 0x32, 0x00]);
        assert_eq!((k.kind.as_str(), k.key, k.interval), ("keyboard_turbo", 4, 50));
        assert_eq!(Action::decode(&[0x0e, 0x01, 0x09, 0x00, 0x00, 0x00, 0x00]).kind, "raw");
        // malformed records are recognised, normal ones pass
        assert!(check_record(&[0x0e, 0x01, 0x09, 0x00, 0x00, 0x00, 0x00]).is_err());
        assert!(check_record(&[0x0e, 0x03, 0x09, 0x00, 0x32, 0x00, 0x00]).is_ok());
        assert!(check_record(&[0x06, 0x05, 0x05, 0x01, 0x90, 0x01, 0x90]).is_ok());
        assert!(check_record(&[0x42, 0x02, 0x01, 0x02, 0x00, 0x00, 0x00]).is_ok());
        assert!(check_record(&[0x42, 0x07, 0x01, 0x02, 0x00, 0x00, 0x00]).is_err());
        let t = Action { kind: "mouse_turbo".into(), button: 9, interval: 100, ..Default::default() };
        assert_eq!(hex(t.encode().unwrap()), "0e030900640000");
    }

    #[test]
    fn roundtrip_all() {
        let acts = vec![
            Action::kind("disabled"),
            Action::mouse(4),
            Action { kind: "double_click".into(), button: 1, ..Default::default() },
            Action::keyboard(3, 0x10),
            Action { kind: "keyboard_turbo".into(), key: 4, interval: 50, ..Default::default() },
            Action { kind: "consumer".into(), code: 0xE9, ..Default::default() },
            Action { kind: "system".into(), code: 2, ..Default::default() },
            Action { kind: "macro".into(), macro_id: 7, macro_mode: "once".into(), repeat: 3, ..Default::default() },
            Action { kind: "macro".into(), macro_id: 7, macro_mode: "toggle".into(), ..Default::default() },
            Action { kind: "dpi".into(), code: 6, ..Default::default() },
            Action { kind: "dpi".into(), code: DPI_FIXED as u16, stage: 2, ..Default::default() },
            Action { kind: "dpi".into(), code: DPI_AIM as u16, dpi_x: 400, dpi_y: 400, ..Default::default() },
            Action { kind: "profile".into(), code: 4, ..Default::default() },
            Action { kind: "profile".into(), code: PROFILE_FIXED as u16, profile: 3, ..Default::default() },
            Action::kind("hypershift"),
        ];
        for a in acts {
            let e = a.encode().unwrap();
            let b = Action::decode(&e);
            assert_eq!(b.encode().unwrap(), e, "{a:?}");
            assert_eq!(b.kind, a.kind);
            assert!(!b.describe().is_empty());
        }
    }

    #[test]
    fn unknown_is_raw() {
        let a = Action::decode(&[0x7f, 0x02, 0xaa, 0xbb, 0, 0, 0]);
        assert_eq!(a.kind, "raw");
        assert_eq!(hex(a.encode().unwrap()), "7f02aabb000000");
    }
}
