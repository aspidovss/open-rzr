//! Chroma lighting packets (Naga Pro tested; other zones use the same extended effects).
//!
//! Packet formats are those OpenRazer sends to this model:
//! * scroll wheel (LED 0x01) and logo (LED 0x04): extended effects, 0x0F02 [varstore, led, effect, ...]
//! * side panel (1×3 LED matrix):
//!   - static / own colours: 0x0F02 [00 00 08] (custom mode) + 0x0F03 [00 00 row start stop RGB…]
//!   - spectrum / breathing / reactive: mouse-extended effects 0x030D, LED 0x05
//! * brightness: 0x0F04 / 0x0F84 [varstore, led, value]; the side panel uses LED 0x00.
//! * other mice (OpenRazer database): left 0x11, right 0x10, backlight 0x05 — extended effects.

use crate::protocol::cmd;

pub const VARSTORE: u8 = 0x01;
pub const LED_SCROLL: u8 = 0x01;
pub const LED_LOGO: u8 = 0x04;
pub const LED_BACKLIGHT: u8 = 0x05;
pub const LED_ZERO: u8 = 0x00;
pub const LED_RIGHT: u8 = 0x10;
pub const LED_LEFT: u8 = 0x11;
pub const SIDE_LEDS: usize = 3;

pub const ZONES: &[(&str, &str)] = &[
    ("scroll", "Scroll wheel"),
    ("logo", "Logo"),
    ("side", "Side panel"),
    ("left", "Left side"),
    ("right", "Right side"),
    ("backlight", "Backlight"),
    ("matrix", "Device lighting"),
];

pub const EFFECTS: &[(&str, &str)] = &[
    ("off", "Off"),
    ("static", "Static"),
    ("spectrum", "Spectrum cycling"),
    ("wave", "Wave"),
    ("breath_single", "Breathing: 1 colour"),
    ("breath_dual", "Breathing: 2 colours"),
    ("breath_random", "Breathing: random"),
    ("reactive", "Reactive"),
    ("custom", "Custom colours (3 LEDs)"),
];

pub fn zone_effects(zone: &str) -> &'static [&'static str] {
    match zone {
        "side" => &["off", "static", "custom", "spectrum", "breath_single", "breath_dual", "breath_random", "reactive"],
        _ => &["off", "static", "spectrum", "wave", "breath_single", "breath_dual", "breath_random", "reactive"],
    }
}

pub fn zone_name(z: &str) -> &'static str {
    ZONES.iter().find(|(k, _)| *k == z).map(|(_, n)| *n).unwrap_or("?")
}

pub fn effect_name(e: &str) -> &'static str {
    EFFECTS.iter().find(|(k, _)| *k == e).map(|(_, n)| *n).unwrap_or("?")
}

pub type Rgb = [u8; 3];

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(default))]
pub struct ZoneState {
    pub effect: String,
    pub color1: Rgb,
    pub color2: Rgb,
    /// reactive afterglow 1..=4
    pub speed: u8,
    /// percent
    pub brightness: u8,
    pub leds: Vec<Rgb>,
}

impl Default for ZoneState {
    fn default() -> Self {
        ZoneState {
            effect: "static".into(),
            color1: [0, 255, 128],
            color2: [0, 64, 255],
            speed: 2,
            brightness: 100,
            leds: vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]],
        }
    }
}

pub fn parse_color(text: &str) -> Result<Rgb, String> {
    let t = text.trim().trim_start_matches('#').to_ascii_lowercase();
    let named = match t.as_str() {
        "red" => "ff0000",
        "green" => "00ff00",
        "blue" => "0000ff",
        "white" => "ffffff",
        "black" => "000000",
        "yellow" => "ffff00",
        "cyan" => "00ffff",
        "magenta" => "ff00ff",
        "orange" => "ff8000",
        "purple" => "8000ff",
        other => other,
    }
    .to_string();
    let full = if named.len() == 3 { named.chars().flat_map(|c| [c, c]).collect() } else { named };
    if full.len() != 6 {
        return Err(format!("colour {text}: expected RRGGBB"));
    }
    let v = u32::from_str_radix(&full, 16).map_err(|_| format!("colour {text}: expected RRGGBB"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

pub fn hex_color(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// One packet: (command, args, data_size)
pub type Packet = (u16, Vec<u8>, u8);

fn extended(led: u8, st: &ZoneState) -> Result<Packet, String> {
    let h = [VARSTORE, led];
    let c1 = st.color1;
    let c2 = st.color2;
    let mut a = h.to_vec();
    let size = match st.effect.as_str() {
        "off" => { a.push(0x00); 0x06 }
        "static" => { a.extend([0x01, 0, 0, 0x01]); a.extend(c1); 0x09 }
        "spectrum" => { a.push(0x03); 0x06 }
        "wave" => { a.extend([0x04, 0x01, 0x28]); 0x06 }
        "breath_random" => { a.push(0x02); 0x06 }
        "breath_single" => { a.extend([0x02, 0x01, 0, 0x01]); a.extend(c1); 0x09 }
        "breath_dual" => { a.extend([0x02, 0x02, 0, 0x02]); a.extend(c1); a.extend(c2); 0x0C }
        "reactive" => { a.extend([0x05, 0, st.speed.clamp(1, 4), 0x01]); a.extend(c1); 0x09 }
        e => return Err(format!("effect {e} is not supported by this zone")),
    };
    Ok((cmd::SET_EFFECT, a, size))
}

fn side_animated(st: &ZoneState) -> Result<Packet, String> {
    let mut a = vec![VARSTORE, LED_BACKLIGHT];
    let size = match st.effect.as_str() {
        "spectrum" => { a.push(0x04); 0x03 }
        "breath_random" => { a.extend([0x03, 0x03]); 0x0A }
        "breath_single" => { a.extend([0x03, 0x01]); a.extend(st.color1); 0x0A }
        "breath_dual" => { a.extend([0x03, 0x02]); a.extend(st.color1); a.extend(st.color2); 0x0A }
        "reactive" => { a.extend([0x02, st.speed.clamp(1, 4)]); a.extend(st.color1); 0x07 }
        e => return Err(format!("effect {e} is not supported by the side panel")),
    };
    Ok((cmd::SET_MOUSE_EFFECT, a, size))
}

fn side_frame(colors: &[Rgb]) -> Vec<Packet> {
    let n = colors.len().min(SIDE_LEDS).max(1);
    let mut frame = vec![0x00, 0x00, 0x00, 0x00, (n - 1) as u8];
    for c in &colors[..n] {
        frame.extend(c);
    }
    let size = frame.len() as u8;
    vec![
        (cmd::SET_EFFECT, vec![0x00, 0x00, 0x08, 0x00, 0x00, 0x00], 0x06),
        (cmd::SET_FRAME, frame, size),
    ]
}

/// Default LED id of a zone (models may override it in their `Zone`).
pub fn zone_led(zone: &str) -> u8 {
    match zone {
        "scroll" => LED_SCROLL,
        "logo" => LED_LOGO,
        "backlight" => LED_BACKLIGHT,
        "right" => LED_RIGHT,
        "left" => LED_LEFT,
        _ => LED_ZERO,
    }
}

pub fn zone_commands(zone: &str, st: &ZoneState) -> Result<Vec<Packet>, String> {
    zone_commands_led(zone, zone_led(zone), st)
}

/// Packets for a zone with LED id `led`. "side" is the Naga Pro side panel (1×3 matrix).
pub fn zone_commands_led(zone: &str, led: u8, st: &ZoneState) -> Result<Vec<Packet>, String> {
    match zone {
        "scroll" | "logo" | "left" | "right" | "backlight" | "matrix" => Ok(vec![extended(led, st)?]),
        "side" => match st.effect.as_str() {
            "off" => Ok(side_frame(&[[0, 0, 0]; SIDE_LEDS])),
            "static" => Ok(side_frame(&[st.color1; SIDE_LEDS])),
            "custom" => {
                let mut leds = st.leds.clone();
                while leds.len() < SIDE_LEDS {
                    leds.push(*leds.last().unwrap_or(&st.color1));
                }
                Ok(side_frame(&leds))
            }
            _ => Ok(vec![side_animated(st)?]),
        },
        z => Err(format!("unknown zone {z}")),
    }
}

pub fn brightness_led(zone: &str) -> u8 {
    brightness_led_of(zone, zone_led(zone))
}

/// LED id used for brightness: the side panel and the whole matrix use LED 0.
pub fn brightness_led_of(zone: &str, led: u8) -> u8 {
    match zone {
        "side" | "matrix" => LED_ZERO,
        _ => led,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(v: &[u8]) -> String {
        v.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn packets_match_openrazer() {
        // openrazer doc: 0f02 "010501000001ff0000" = static red on LED 05
        let (c, a, s) = extended(0x05, &ZoneState { effect: "static".into(), color1: [255, 0, 0], ..Default::default() }).unwrap();
        assert_eq!((c, hex(&a).as_str(), s), (0x0F02, "010501000001ff0000", 9));
        let f = zone_commands("side", &ZoneState { effect: "static".into(), color1: [1, 2, 3], ..Default::default() }).unwrap();
        assert_eq!(hex(&f[0].1), "000008000000");
        assert_eq!(hex(&f[1].1), format!("0000000002{}", "010203".repeat(3)));
        assert_eq!(f[1].2, 0x0E);
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#00ff80").unwrap(), [0, 255, 128]);
        assert_eq!(parse_color("fff").unwrap(), [255, 255, 255]);
        assert_eq!(parse_color("red").unwrap(), [255, 0, 0]);
        assert!(parse_color("xyz").is_err());
        assert!(zone_commands("side", &ZoneState { effect: "wave".into(), ..Default::default() }).is_err());
    }
}
