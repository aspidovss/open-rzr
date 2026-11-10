//! On-board macro content encoding.
//!
//! ```text
//! 01 kk        key down (HID keycode)       02 kk        key up
//! 03/04 ss     system control               05/06 cccc   consumer usage (0000 = release)
//! 08 bb        mouse buttons bitmask (01 L, 02 R, 04 M, 08 Back, 10 Fwd), 00 = released
//! 0a ww        wheel, signed                11 dd / 12 dddd  delay ms
//! ```

use crate::hidkeys;

const OP_KEY_DOWN: u8 = 0x01;
const OP_KEY_UP: u8 = 0x02;
const OP_SYS_A: u8 = 0x03;
const OP_SYS_B: u8 = 0x04;
const OP_CONS_A: u8 = 0x05;
const OP_CONS_B: u8 = 0x06;
const OP_MOUSE: u8 = 0x08;
const OP_WHEEL: u8 = 0x0A;
const OP_DELAY1: u8 = 0x11;
const OP_DELAY2: u8 = 0x12;

pub const MOUSE_BITS: &[(u8, &str)] = &[(0x01, "Left"), (0x02, "Right"), (0x04, "Middle"), (0x08, "Back"), (0x10, "Forward")];

/// kind: key_down, key_up, mouse, wheel, delay, consumer, system
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct MacroStep {
    pub kind: String,
    pub value: i64,
}

impl MacroStep {
    pub fn new(kind: &str, value: i64) -> Self {
        MacroStep { kind: kind.into(), value }
    }

    pub fn describe(&self) -> String {
        match self.kind.as_str() {
            "key_down" => format!("↓ {}", hidkeys::key_name(self.value as u8)),
            "key_up" => format!("↑ {}", hidkeys::key_name(self.value as u8)),
            "mouse" => {
                if self.value == 0 {
                    "Mouse: release all".into()
                } else {
                    let names: Vec<&str> = MOUSE_BITS
                        .iter()
                        .filter(|(b, _)| self.value as u8 & b != 0)
                        .map(|(_, n)| *n)
                        .collect();
                    format!("Mouse: {}", names.join("+"))
                }
            }
            "wheel" => format!("Wheel {}{}", if self.value > 0 { "+" } else { "" }, self.value),
            "delay" => format!("Delay {} ms", self.value),
            "consumer" => {
                if self.value == 0 {
                    "Media: release".into()
                } else {
                    format!("Media: {}", hidkeys::consumer_name(self.value as u16))
                }
            }
            k => format!("{k} {}", self.value),
        }
    }
}

pub fn encode(steps: &[MacroStep]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for s in steps {
        match s.kind.as_str() {
            "key_down" => out.extend([OP_KEY_DOWN, s.value as u8]),
            "key_up" => out.extend([OP_KEY_UP, s.value as u8]),
            "mouse" => out.extend([OP_MOUSE, (s.value as u8) & 0x1F]),
            "wheel" => out.extend([OP_WHEEL, (s.value.clamp(-127, 127) as i8) as u8]),
            "consumer" => {
                out.push(OP_CONS_A);
                out.extend((s.value as u16).to_be_bytes());
            }
            "system" => out.extend([OP_SYS_A, s.value as u8]),
            "delay" => {
                let mut ms = s.value.max(0);
                while ms > 0xFFFF {
                    out.push(OP_DELAY2);
                    out.extend(0xFFFFu16.to_be_bytes());
                    ms -= 0xFFFF;
                }
                if ms == 0 {
                    continue;
                }
                if ms < 0x100 {
                    out.extend([OP_DELAY1, ms as u8]);
                } else {
                    out.push(OP_DELAY2);
                    out.extend((ms as u16).to_be_bytes());
                }
            }
            k => return Err(format!("unknown macro step {k}")),
        }
    }
    Ok(out)
}

fn op_size(op: u8) -> Option<usize> {
    match op {
        OP_KEY_DOWN | OP_KEY_UP | OP_SYS_A | OP_SYS_B | OP_MOUSE | OP_WHEEL | OP_DELAY1 => Some(1),
        OP_CONS_A | OP_CONS_B | OP_DELAY2 => Some(2),
        _ => None,
    }
}

pub fn decode(data: &[u8]) -> Vec<MacroStep> {
    let mut steps = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let op = data[i];
        let Some(size) = op_size(op) else { break };
        if i + 1 + size > data.len() {
            break;
        }
        let v = &data[i + 1..i + 1 + size];
        let step = match op {
            OP_KEY_DOWN => MacroStep::new("key_down", v[0] as i64),
            OP_KEY_UP => MacroStep::new("key_up", v[0] as i64),
            OP_MOUSE => MacroStep::new("mouse", v[0] as i64),
            OP_WHEEL => MacroStep::new("wheel", v[0] as i8 as i64),
            OP_CONS_A | OP_CONS_B => MacroStep::new("consumer", u16::from_be_bytes([v[0], v[1]]) as i64),
            OP_DELAY1 => MacroStep::new("delay", v[0] as i64),
            OP_DELAY2 => MacroStep::new("delay", u16::from_be_bytes([v[0], v[1]]) as i64),
            _ => MacroStep::new("system", v[0] as i64),
        };
        steps.push(step);
        i += 1 + size;
    }
    steps
}

/// Tiny DSL for the CLI: "ctrl+c, 50ms, click:left, wheel:-1, down:a, up:a".
pub fn from_text(text: &str) -> Result<Vec<MacroStep>, String> {
    let mut steps = Vec::new();
    let key = |n: &str| hidkeys::key_by_name(n).ok_or_else(|| format!("unknown key: {n}"));
    for tok in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let low = tok.to_ascii_lowercase();
        if let Some(ms) = low.strip_suffix("ms").and_then(|n| n.trim().parse::<i64>().ok()) {
            steps.push(MacroStep::new("delay", ms));
        } else if let Some(k) = low.strip_prefix("down:") {
            steps.push(MacroStep::new("key_down", key(k)? as i64));
        } else if let Some(k) = low.strip_prefix("up:") {
            steps.push(MacroStep::new("key_up", key(k)? as i64));
        } else if let Some(b) = low.strip_prefix("click:") {
            let bit = match b.trim() {
                "left" => 0x01,
                "right" => 0x02,
                "middle" => 0x04,
                "back" => 0x08,
                "forward" => 0x10,
                x => return Err(format!("unknown button {x}")),
            };
            steps.push(MacroStep::new("mouse", bit));
            steps.push(MacroStep::new("delay", 20));
            steps.push(MacroStep::new("mouse", 0));
        } else if let Some(w) = low.strip_prefix("wheel:") {
            steps.push(MacroStep::new("wheel", w.trim().parse().map_err(|_| format!("wheel: {w}"))?));
        } else {
            let keys: Vec<u8> = tok.split('+').map(|p| key(p)).collect::<Result<_, _>>()?;
            steps.extend(keys.iter().map(|k| MacroStep::new("key_down", *k as i64)));
            steps.push(MacroStep::new("delay", 20));
            steps.extend(keys.iter().rev().map(|k| MacroStep::new("key_up", *k as i64)));
        }
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec() {
        let steps = from_text("ctrl+c, 300ms, click:left, wheel:-1").unwrap();
        let data = encode(&steps).unwrap();
        assert_eq!(&data[..4], &[0x01, 0xE0, 0x01, 0x06]);
        assert!(data.windows(3).any(|w| w == [0x12, 0x01, 0x2C]));
        assert!(data.ends_with(&[0x0A, 0xFF]));
        assert_eq!(encode(&decode(&data)).unwrap(), data);
    }

    #[test]
    fn long_delay_split() {
        let data = encode(&[MacroStep::new("delay", 70000)]).unwrap();
        assert_eq!(decode(&data).iter().map(|s| s.value).sum::<i64>(), 70000);
    }
}
