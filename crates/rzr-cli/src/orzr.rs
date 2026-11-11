//! orzr-cli — the open-rzr command line with the syntax of razer-cli
//! (https://github.com/LoLei/razer-cli): `-e EFFECT -c COLOR -z ZONE -b BRIGHTNESS --dpi --poll
//! --battery -l/-ll/-ls -d DEVICE -a --restore -man`. Written from scratch for open-rzr (no razer-cli
//! code); it talks to the mouse through openrzr directly, so it needs no OpenRazer daemon and works on
//! Windows too. Buttons, macros and profiles stay in `open-rzr-cli`.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use openrzr::devices::fx;
use openrzr::lighting::ZoneState;
use openrzr::{compat, feat, find_devices, logs, DeviceInfo, RazerMouse};

const MANUAL: &[(&str, &str)] = &[
    ("automatic", "-a, --automatic\n    Set a colour on every device without further arguments. The colour is taken from pywal\n    (~/.cache/wal/colors, colour 1) or from X resources (*color1). With -e the effect is kept,\n    otherwise \"static\" is used.\n\n    orzr-cli -a\n    orzr-cli -a -e breath_single"),
    ("battery", "--battery print | low PERCENT | idle SECONDS\n    print       charge, charging, low power threshold and sleep delay\n    low N       low power mode below N % (5-100)\n    idle N      sleep after N seconds without use (60-900)\n\n    orzr-cli --battery print\n    orzr-cli --battery low 15 idle 300"),
    ("brightness", "-b, --brightness PERCENT | ZONE PERCENT [ZONE PERCENT ...]\n    One number sets every zone, pairs set single zones (0-100).\n\n    orzr-cli -b 50\n    orzr-cli -b logo 100 scroll_wheel 20 generic 0"),
    ("color", "-c, --color COLOR [COLOR ...]\n    A colour is hex (ff0000, #ff0000), three numbers 0-255 (255 0 0), \"random\" or \"x\"\n    (X resources colour). Each zone group takes as many colours as its effect needs:\n    one for static, breath_single and reactive, two for breath_dual.\n\n    orzr-cli -c 00ff80\n    orzr-cli -e breath_dual -c ff0000 0000ff"),
    ("device", "-d, --device NAME|SERIAL [NAME|SERIAL ...]\n    Only change these devices. Use the name or the serial number printed by -ls\n    (product id like 008f works too). Without -d all devices are changed.\n\n    orzr-cli -d \"Razer Naga Pro (wired)\" -c ff0000\n    orzr-cli -d PM2107H16300460 --dpi 1600"),
    ("dpi", "--dpi DPI | X,Y | print\n    Set the sensitivity (both axes, or X,Y) or print it.\n\n    orzr-cli --dpi 1600\n    orzr-cli --dpi 1600,800\n    orzr-cli --dpi print"),
    ("effect", "-e, --effect EFFECT[,PARAMETER] [EFFECT ...]\n    Effects: none, static, spectrum, wave, breath_single, breath_dual, breath_random,\n    reactive[,1-4], multicolor (one colour per LED; Naga Pro side panel: up to 3 colours).\n    One effect per zone group (-z); without -z the effect applies to every zone.\n    Effects of razer-cli the mouse does not have (ripple, starlight_*, blinking, pulsate,\n    breath_triple) are reported and skipped.\n\n    orzr-cli -e spectrum\n    orzr-cli -e static spectrum -z logo scroll_wheel -c 00ff00\n    orzr-cli -e reactive,3 -c ffffff"),
    ("example", "orzr-cli -ls                          devices and their settings\norzr-cli -e static -c ff0000           every zone red\norzr-cli -e static -c 00ff00 -z logo   only the logo\norzr-cli -e multicolor -c ff0000 00ff00 0000ff -z generic   side panel LEDs\norzr-cli -b 30 --dpi 800,800 --poll 500\norzr-cli --battery low 10 idle 600\norzr-cli --restore                     apply the last used settings again"),
    ("list_devices", "-l, --list_devices        devices, settings, zones and their effects\n-ll, --list_devices_long  the same, one effect per line\n-ls, --list_devices_short devices and their settings only"),
    ("poll", "--poll 125|500|1000|print\n    Set the polling rate in Hz or print it.\n\n    orzr-cli --poll 1000"),
    ("restore", "--restore\n    orzr-cli remembers the lighting, brightness, DPI, polling and battery arguments last used\n    for every device (by serial number) and --restore applies them again, e.g. after login."),
    ("zone", "-z, --zone ZONE[,ZONE...] [ZONE[,ZONE...] ...]\n    Zones: generic (the whole device / Naga side panel), logo, scroll_wheel (scroll, wheel),\n    left, right, backlight, all. Each argument is a zone group for the matching -e effect.\n\n    orzr-cli -e static -z logo,scroll_wheel -c ff00ff\n    orzr-cli -e spectrum static -z logo generic -c 0000ff"),
];

const HELP: &str = "usage: orzr-cli [-h] [-man [MANUAL ...]] [-v] [-d DEVICE [DEVICE ...]] [-a] [-e EFFECT [EFFECT ...]]
                [-c COLOR [COLOR ...]] [-z ZONES [ZONES ...]] [-b BRIGHTNESS [BRIGHTNESS ...]] [--dpi DPI]
                [--poll POLL] [--battery BATTERY [BATTERY ...]] [-l] [-ll] [-ls] [--sync] [--restore] [--version]

open-rzr command line, compatible with the razer-cli syntax.

options:
  -h, --help            show this help message and exit
  -man [MANUAL ...], --manual [MANUAL ...]
                        print help details for given feature(s) (also: orzr-cli help TOPIC)
  -v, --verbose         increase output verbosity
  -d DEVICE [DEVICE ...], --device DEVICE [DEVICE ...]
                        only affect these devices, same name as output of -l (or serial)
  -a, --automatic       set colours on all devices from pywal / X resources
  -e EFFECT [EFFECT ...], --effect EFFECT [EFFECT ...]
                        set effect
  -c COLOR [COLOR ...], --color COLOR [COLOR ...]
                        choose colour: one argument for hex, or three for base10 rgb
  -z ZONES [ZONES ...], --zone ZONES [ZONES ...]
                        choose zone for colour(s)
  -b BRIGHTNESS [BRIGHTNESS ...], --brightness BRIGHTNESS [BRIGHTNESS ...]
                        set brightness of device
  --dpi DPI             set DPI of device (use print as a value to show it)
  --poll POLL           set polling rate of device (use print as a value to show it)
  --battery BATTERY [BATTERY ...]
                        set low threshold and/or idle delay (use print as a value to show it)
  -l, --list_devices    list available devices, settings, and their supported capabilities/effects
  -ll, --list_devices_long
                        list available devices settings, and list their supported capabilities/effects
  -ls, --list_devices_short
                        list available devices and their settings
  --sync                apply the lighting to all devices at once (always the case in orzr-cli)
  --restore             load last used settings
  --version             print version number

open-rzr extras:
  --mock                use the built-in Naga Pro emulator
  --debug               print the open-rzr log with every USB packet
  --openrazer auto|on|off
                        OpenRazer compatibility (Linux)

Buttons, macros and on-board profiles: open-rzr-cli --help";

#[derive(Default, Debug)]
struct Opts {
    help: bool,
    manual: Option<Vec<String>>,
    verbose: bool,
    devices: Vec<String>,
    automatic: bool,
    effects: Vec<String>,
    colors: Vec<String>,
    zones: Vec<String>,
    brightness: Vec<String>,
    dpi: Option<String>,
    poll: Option<String>,
    battery: Vec<String>,
    /// 1 = -l, 2 = -ll, 3 = -ls
    list: u8,
    sync: bool,
    restore: bool,
    version: bool,
    mock: bool,
    /// print the open-rzr log (USB packets) to the console
    debug: bool,
    openrazer: Option<String>,
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut i = 0;
    // values of a "nargs=+" option: everything up to the next option
    let many = |i: &mut usize, name: &str| -> Result<Vec<String>, String> {
        let mut v = Vec::new();
        while *i + 1 < args.len() && !is_opt(&args[*i + 1]) {
            *i += 1;
            v.push(args[*i].clone());
        }
        if v.is_empty() {
            return Err(format!("argument {name}: expected at least one argument"));
        }
        Ok(v)
    };
    while i < args.len() {
        let a = args[i].as_str();
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (a, None),
        };
        let one = |i: &mut usize| -> Result<String, String> {
            if let Some(v) = &inline {
                return Ok(v.clone());
            }
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| format!("argument {flag}: expected one argument"))
        };
        match flag {
            "-h" | "--help" => o.help = true,
            "-man" | "--manual" => {
                let mut v = Vec::new();
                while i + 1 < args.len() && !is_opt(&args[i + 1]) {
                    i += 1;
                    v.push(args[i].clone());
                }
                o.manual = Some(v);
            }
            "help" if i == 0 => {
                o.manual = Some(args[1..].to_vec());
                break;
            }
            "-v" | "--verbose" => o.verbose = true,
            "-d" | "--device" => o.devices = many(&mut i, "-d/--device")?,
            "-a" | "--automatic" => o.automatic = true,
            "-e" | "--effect" => o.effects = many(&mut i, "-e/--effect")?,
            "-c" | "--color" | "--colour" => o.colors = many(&mut i, "-c/--color")?,
            "-z" | "--zone" => o.zones = many(&mut i, "-z/--zone")?,
            "-b" | "--brightness" => o.brightness = many(&mut i, "-b/--brightness")?,
            "--dpi" => o.dpi = Some(one(&mut i)?),
            "--poll" => o.poll = Some(one(&mut i)?),
            "--battery" => o.battery = many(&mut i, "--battery")?,
            "-l" | "--list_devices" => o.list = 1,
            "-ll" | "--list_devices_long" => o.list = 2,
            "-ls" | "--list_devices_short" => o.list = 3,
            "--sync" => o.sync = true,
            "--restore" => o.restore = true,
            "--version" => o.version = true,
            "--mock" => o.mock = true,
            "--debug" => o.debug = true,
            "--openrazer" => o.openrazer = Some(one(&mut i)?),
            _ => return Err(format!("unrecognized arguments: {a}")),
        }
        i += 1;
    }
    Ok(o)
}

/// "-e" is an option, "-5" or "ff0000" are values.
fn is_opt(s: &str) -> bool {
    s.starts_with('-') && s.len() > 1 && !s[1..].starts_with(|c: char| c.is_ascii_digit())
}

// ------------------------------------------------------------------ colours

fn hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn random_color() -> [u8; 3] {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7);
    let n = t ^ N.fetch_add(0x9E37_79B9, Ordering::Relaxed);
    let x = n.wrapping_mul(2_654_435_761).rotate_left(13) ^ (n >> 7);
    [(x >> 16) as u8, (x >> 8) as u8, x as u8]
}

/// Colour 1 of pywal, or *color1 of X resources.
fn x_color() -> Option<[u8; 3]> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)?;
    if let Ok(s) = std::fs::read_to_string(home.join(".cache/wal/colors")) {
        if let Some(c) = s.lines().nth(1).and_then(|l| hex(l.trim())) {
            return Some(c);
        }
    }
    let out = std::process::Command::new("xrdb").arg("-query").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    text.lines().find(|l| l.trim_start().starts_with("*color1:") || l.trim_start().starts_with("*.color1:")).and_then(|l| hex(l.split(':').nth(1)?.trim()))
}

/// razer-cli colour rules: "ff0000" | "random" | "x" | three base-10 numbers ("rng" = random part).
fn parse_colors(args: &[String]) -> Result<Vec<[u8; 3]>, String> {
    let mut v = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].to_lowercase();
        if a.len() > 3 || a == "x" {
            v.push(match a.as_str() {
                "random" => random_color(),
                "x" => x_color().ok_or("no X / pywal colour found (colour \"x\")")?,
                _ => hex(&a).ok_or_else(|| format!("colour {} ({}) looks to have a typo", v.len() + 1, args[i]))?,
            });
            i += 1;
        } else if i + 2 < args.len() {
            let mut rgb = [0u8; 3];
            for (k, part) in args[i..i + 3].iter().enumerate() {
                rgb[k] = if part.eq_ignore_ascii_case("rng") {
                    random_color()[k]
                } else {
                    part.parse::<u8>().map_err(|_| format!("colour {} ({} {} {}) looks to have a typo", v.len() + 1, args[i], args[i + 1], args[i + 2]))?
                };
            }
            v.push(rgb);
            i += 3;
        } else {
            return Err("unexpected arguments for colour (hex, or three numbers 0-255)".into());
        }
    }
    Ok(v)
}

// ------------------------------------------------------------------ zones

/// razer-cli zone name -> open-rzr zone ids of this mouse
fn zone_ids(m: &RazerMouse, name: &str) -> Vec<&'static str> {
    let have = |z: &str| m.zones.iter().any(|x| x.id == z);
    let pick: &[&str] = match name.to_lowercase().as_str() {
        "all" => return m.zones.iter().map(|z| z.id).collect(),
        "generic" => &["side", "matrix"],
        "scroll_wheel" | "scroll" | "wheel" => &["scroll"],
        "logo" => &["logo"],
        "left" => &["left"],
        "right" => &["right"],
        "backlight" => &["backlight"],
        "side" => &["side"],
        "matrix" => &["matrix"],
        _ => &[],
    };
    m.zones.iter().map(|z| z.id).filter(|z| pick.contains(z) && have(z)).collect()
}

/// open-rzr zone id -> razer-cli name
fn cli_zone(id: &str) -> &'static str {
    match id {
        "side" | "matrix" => "generic",
        "scroll" => "scroll_wheel",
        "logo" => "logo",
        "left" => "left",
        "right" => "right",
        "backlight" => "backlight",
        _ => "generic",
    }
}

fn zone_groups(m: &RazerMouse, z: &[String]) -> Vec<Vec<&'static str>> {
    if z.is_empty() {
        return vec![m.zones.iter().map(|z| z.id).collect()];
    }
    z.iter()
        .map(|g| {
            let mut v: Vec<&'static str> = Vec::new();
            for n in g.split(',').filter(|s| !s.is_empty()) {
                for id in zone_ids(m, n) {
                    if !v.contains(&id) {
                        v.push(id);
                    }
                }
            }
            v
        })
        .collect()
}

// ------------------------------------------------------------------ devices

struct Dev {
    m: RazerMouse,
    serial: String,
}

fn open_devices(o: &Opts) -> Vec<Dev> {
    let mut groups: Vec<(u16, Vec<DeviceInfo>)> = Vec::new();
    for d in find_devices(o.mock) {
        if o.mock != (d.backend == "mock") {
            continue;
        }
        match groups.iter_mut().find(|(p, _)| *p == d.pid) {
            Some((_, v)) => v.push(d),
            None => groups.push((d.pid, vec![d])),
        }
    }
    let mut out = Vec::new();
    for (_, cands) in groups {
        match RazerMouse::open_first(&cands) {
            Ok(mut m) => {
                let serial = m.serial().unwrap_or_default();
                out.push(Dev { m, serial });
            }
            Err(e) => eprintln!("Warning: {e}"),
        }
    }
    out
}

fn selected(o: &Opts, d: &Dev) -> bool {
    if o.devices.is_empty() {
        return true;
    }
    let pid = format!("{:04x}", d.m.info.pid);
    o.devices.iter().any(|s| {
        s.eq_ignore_ascii_case(d.m.name()) || (!d.serial.is_empty() && s == &d.serial) || s.trim_start_matches("0x").eq_ignore_ascii_case(&pid)
    })
}

// ------------------------------------------------------------------ actions

fn set_effects(o: &Opts, d: &mut Dev) -> Result<(), String> {
    let mut colors = if !o.colors.is_empty() {
        parse_colors(&o.colors)?
    } else if o.automatic {
        vec![x_color().ok_or("-a: no pywal (~/.cache/wal/colors) or X resources colour found")?]
    } else {
        Vec::new()
    };
    let mut groups = zone_groups(&d.m, &o.zones);
    let mut effects: Vec<String> = if o.effects.is_empty() { vec!["static".into()] } else { o.effects.clone() };
    // one effect for many groups, or many effects for one group (razer-cli rules)
    if effects.len() == 1 && groups.len() > 1 {
        effects = vec![effects[0].clone(); groups.len()];
    } else if groups.len() == 1 && effects.len() > 1 {
        groups = vec![groups[0].clone(); effects.len()];
    }
    let n = groups.len().min(effects.len());
    if o.verbose && effects.len() < groups.len() {
        println!("   Warning: only {} effects provided for {} zones", effects.len(), groups.len());
    }
    let mut used_colors = 0usize;
    for (g, spec) in groups.iter().take(n).zip(effects.iter()) {
        let mut parts = spec.split(',');
        let effect = parts.next().unwrap_or("").to_lowercase();
        let param = parts.next().map(str::to_string);
        let (ours, need) = match effect.as_str() {
            "none" | "off" => ("off", 0),
            "static" => ("static", 1),
            "spectrum" => ("spectrum", 0),
            "wave" => ("wave", 0),
            "breath_single" => ("breath_single", 1),
            "breath_dual" => ("breath_dual", 2),
            "breath_random" => ("breath_random", 0),
            "reactive" => ("reactive", 1),
            "multicolor" | "custom" => ("custom", param.as_deref().and_then(|p| p.parse().ok()).unwrap_or(3)),
            "brightness" => {
                // razer-cli: an effect "brightness[,N]" sets the brightness of the zones
                let pct: u8 = param.as_deref().and_then(|p| p.parse().ok()).unwrap_or(100);
                for z in g {
                    d.m.set_brightness(z, pct).map_err(|e| e.to_string())?;
                }
                continue;
            }
            "ripple" | "ripple_random" | "starlight_single" | "starlight_dual" | "starlight_random" | "blinking" | "pulsate"
            | "breath_triple" | "active" => {
                println!("   {}: {effect} is not supported by this mouse [skipping]", d.m.name());
                continue;
            }
            _ => {
                println!("   Warning: {effect} is not a known effect.");
                continue;
            }
        };
        while colors.len() < used_colors + need {
            // razer-cli fills missing colours with random ones
            colors.push(random_color());
        }
        let cs = &colors[used_colors..used_colors + need];
        let mut st = ZoneState { effect: ours.into(), ..Default::default() };
        if let Some(c) = cs.first() {
            st.color1 = *c;
        }
        if let Some(c) = cs.get(1) {
            st.color2 = *c;
        }
        if ours == "custom" {
            st.leds = (0..3).map(|k| cs.get(k % cs.len().max(1)).copied().unwrap_or_else(random_color)).collect();
        }
        if ours == "reactive" {
            st.speed = param.as_deref().and_then(|p| p.parse().ok()).unwrap_or(2).clamp(1, 4);
        }
        if g.is_empty() && o.verbose {
            println!("   {}: none of the zones {:?} exists on this mouse", d.m.name(), o.zones);
        }
        for z in g {
            if !d.m.zone_effects(z).contains(&ours) {
                if o.verbose {
                    println!("   {} zone: does not support {effect}", cli_zone(z));
                }
                continue;
            }
            d.m.set_lighting(z, &st).map_err(|e| e.to_string())?;
            if o.verbose {
                println!("   {} zone: {effect} {}", cli_zone(z), cs.iter().map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect::<Vec<_>>().join(" "));
            }
        }
        used_colors += need;
    }
    Ok(())
}

fn set_brightness(o: &Opts, d: &mut Dev) -> Result<(), String> {
    let b = &o.brightness;
    let mut pairs: Vec<(String, u8)> = Vec::new();
    if b.len() == 1 {
        let v: u8 = b[0].parse().map_err(|_| "invalid brightness input, see `orzr-cli --manual brightness`")?;
        pairs.push(("all".into(), v.min(100)));
    } else if b.len() % 2 == 0 {
        for c in b.chunks(2) {
            match c[1].parse::<u8>() {
                Ok(v) => pairs.push((c[0].clone(), v.min(100))),
                Err(_) => println!("Warning: {} is not a number for {} [Skipping]", c[1], c[0]),
            }
        }
    } else {
        return Err("invalid brightness input, see `orzr-cli --manual brightness`".into());
    }
    for (zone, v) in pairs {
        let ids = zone_ids(&d.m, &zone);
        if ids.is_empty() && o.verbose {
            println!("   Device does not support lighting_{zone}_brightness");
        }
        for z in ids {
            d.m.set_brightness(z, v).map_err(|e| e.to_string())?;
            if o.verbose {
                println!("   Setting {} brightness to {v}", cli_zone(z));
            }
        }
    }
    Ok(())
}

fn dpi(o: &Opts, d: &mut Dev, v: &str) -> Result<(), String> {
    if !d.m.has(feat::DPI) {
        if o.verbose {
            println!("Device {} does not have a DPI setter", d.m.name());
        }
        return Ok(());
    }
    if v == "print" {
        let (x, y) = d.m.get_dpi().map_err(|e| e.to_string())?;
        let label = if o.poll.as_deref() == Some("print") { "DPI: " } else { "" };
        if x == y {
            println!("{label}{x}");
        } else {
            println!("{label}{x} {y}");
        }
        return Ok(());
    }
    let mut it = v.split(',').map(|s| s.trim().parse::<u16>());
    let x = it.next().ok_or("--dpi: value")?.map_err(|_| format!("--dpi: {v} is not a number"))?;
    let y = match it.next() {
        Some(r) => r.map_err(|_| format!("--dpi: {v} is not a number"))?,
        None => x,
    };
    if o.verbose {
        println!("Setting DPI of device {} to {x},{y}", d.m.name());
    }
    d.m.set_dpi(x, y).map_err(|e| e.to_string())
}

fn poll(o: &Opts, d: &mut Dev, v: &str) -> Result<(), String> {
    if !d.m.has(feat::POLLING) {
        println!("Device does not support setting the polling rate");
        return Ok(());
    }
    if v == "print" {
        let hz = d.m.get_polling_rate().map_err(|e| e.to_string())?;
        if o.dpi.as_deref() == Some("print") || o.battery.len() == 1 {
            println!("poll_rate: {hz}");
        } else {
            println!("{hz}");
        }
        return Ok(());
    }
    let hz: u32 = v.parse().map_err(|_| format!("--poll: {v} is not a number"))?;
    if o.verbose {
        println!("Setting polling rate of device {} to {hz}", d.m.name());
    }
    d.m.set_polling_rate(hz).map_err(|e| e.to_string())
}

fn battery(o: &Opts, d: &mut Dev) -> Result<(), String> {
    if !d.m.has(feat::BATTERY) {
        println!("Does {} have a battery?", d.m.name());
        return Ok(());
    }
    if o.battery.len() < 2 {
        print_battery(&mut d.m, "");
        return Ok(());
    }
    let b = &o.battery;
    let mut i = 0;
    while i + 1 < b.len() {
        let v: u32 = b[i + 1].parse().map_err(|_| format!("--battery: {} is not a number", b[i + 1]))?;
        match b[i].as_str() {
            "low" => {
                d.m.set_low_battery_threshold(v.min(100) as u8).map_err(|e| e.to_string())?;
                if o.verbose {
                    println!("{} low battery = {} %", d.m.name(), d.m.get_low_battery_threshold().unwrap_or(0));
                }
                i += 2;
            }
            "idle" => {
                d.m.set_idle_time(v.min(u16::MAX as u32) as u16).map_err(|e| e.to_string())?;
                if o.verbose {
                    println!("{} idle delay = {} seconds", d.m.name(), d.m.get_idle_time().unwrap_or(0));
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    Ok(())
}

fn print_battery(m: &mut RazerMouse, pad: &str) {
    if pad.is_empty() {
        println!("{} battery:", m.name());
    } else {
        println!("{pad}battery:");
    }
    if let Ok(c) = m.get_battery() {
        println!("{pad}   charge: {c}");
    }
    println!("{pad}   charging: {}", if m.is_charging().unwrap_or(false) { "True" } else { "False" });
    if let Ok(l) = m.get_low_battery_threshold() {
        println!("{pad}   low threshold: {l} %");
    }
    if let Ok(t) = m.get_idle_time() {
        println!("{pad}   idle delay {t} seconds");
    }
}

fn list(o: &Opts, d: &mut Dev) {
    let m = &mut d.m;
    println!("{}:", m.name());
    println!("   type: mouse");
    if !d.serial.is_empty() {
        println!("   serial: {}", d.serial);
    }
    if let Ok(fw) = m.firmware() {
        println!("   firmware: {fw}");
    }
    if o.verbose {
        println!("   module: {}", if m.spec.verified() { "tested" } else { "OpenRazer database" });
        if let Some(dir) = &m.openrazer {
            println!("   via OpenRazer driver: {}", dir.display());
        }
    }
    if m.has(feat::DPI) {
        if let Ok((x, y)) = m.get_dpi() {
            println!("   DPI: ({x}, {y})");
        }
        println!("   max DPI: {}", m.spec.max_dpi);
    }
    if m.has(feat::DPI_STAGES) {
        if let Ok(st) = m.get_dpi_stages(1) {
            println!("   DPI stages: ({}, {:?})", st.active, st.stages);
        }
    }
    if m.has(feat::POLLING) {
        if let Ok(hz) = m.get_polling_rate() {
            println!("   polling rate: {hz}");
        }
    }
    if m.has(feat::BATTERY) {
        print_battery(m, "   ");
    }
    if o.list == 3 {
        return;
    }
    let zones: Vec<&'static str> = m.zones.iter().map(|z| z.id).collect();
    for z in zones {
        println!("   {} zone:", cli_zone(z));
        if let Ok(b) = m.get_brightness(z) {
            println!("      brightness: {b}");
        }
        let effects: Vec<String> = m
            .zone_effects(z)
            .iter()
            .map(|e| match *e {
                "off" => "none".to_string(),
                "custom" => "multicolor".to_string(),
                e => e.to_string(),
            })
            .collect();
        if o.list == 2 {
            println!("      effects:\n         {}", effects.join("\n         "));
        } else {
            println!("      effects: {}", effects.join(", "));
        }
    }
    if o.list == 2 {
        let f = m.spec.zones.iter().map(|z| fx::names(z.effects).len()).sum::<usize>();
        println!("   capabilities:\n      {}", feat::names(m.features).join("\n      "));
        if o.verbose {
            println!("   ({f} zone effects in the device description)");
        }
    }
}

// ------------------------------------------------------------------ --restore

fn settings_file() -> PathBuf {
    logs::config_dir().join("orzr-cli-settings.txt")
}

/// The setting arguments of a run, kept per device serial: "serial<TAB>arg<US>arg…".
/// "print" queries and everything else (-l, -d, -v …) are left out.
fn setting_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut block: Vec<String> = Vec::new();
    let mut keep = false;
    let flush = |block: &mut Vec<String>, keep: bool, out: &mut Vec<String>| {
        let has_value = block.len() > 1 || block.first().is_some_and(|f| f.contains('='));
        let print = block.iter().any(|a| a == "print" || a.ends_with("=print"));
        if keep && has_value && !print {
            out.append(block);
        }
        block.clear();
    };
    for a in args {
        if is_opt(a) {
            flush(&mut block, keep, &mut out);
            let f = a.split('=').next().unwrap_or(a);
            keep = matches!(f, "-e" | "--effect" | "-c" | "--color" | "--colour" | "-z" | "--zone" | "-b" | "--brightness" | "--dpi" | "--poll" | "--battery");
        }
        block.push(a.clone());
    }
    flush(&mut block, keep, &mut out);
    out
}

fn load_saved() -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    if let Ok(s) = std::fs::read_to_string(settings_file()) {
        for line in s.lines() {
            if let Some((serial, args)) = line.split_once('\t') {
                m.insert(serial.to_string(), args.split('\u{1f}').map(str::to_string).collect());
            }
        }
    }
    m
}

fn save(serial: &str, args: &[String]) {
    if serial.is_empty() || args.is_empty() {
        return;
    }
    let mut all = load_saved();
    // merge: later options of the same kind replace earlier ones
    let old = all.remove(serial).unwrap_or_default();
    let merged = merge_args(&old, args);
    all.insert(serial.to_string(), merged);
    let text: String = all.iter().map(|(k, v)| format!("{k}\t{}\n", v.join("\u{1f}"))).collect();
    let _ = std::fs::create_dir_all(logs::config_dir());
    let _ = std::fs::write(settings_file(), text);
}

/// Split into option blocks ("-e static", "-c ff0000", ...) and let `new` blocks replace `old` ones of the same option.
fn merge_args(old: &[String], new: &[String]) -> Vec<String> {
    fn blocks(v: &[String]) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for a in v {
            if is_opt(a) {
                let key = match a.split('=').next().unwrap_or(a) {
                    "--effect" => "-e",
                    "--color" | "--colour" => "-c",
                    "--zone" => "-z",
                    "--brightness" => "-b",
                    f => f,
                }
                .to_string();
                out.push((key, vec![a.clone()]));
            } else if let Some(last) = out.last_mut() {
                last.1.push(a.clone());
            }
        }
        out
    }
    let nb = blocks(new);
    // lighting options belong together: a new -e/-c/-z replaces all three
    let lighting = nb.iter().any(|(k, _)| ["-e", "-c", "-z"].contains(&k.as_str()));
    let mut res: Vec<(String, Vec<String>)> = blocks(old)
        .into_iter()
        .filter(|(k, _)| !nb.iter().any(|(n, _)| n == k) && !(lighting && ["-e", "-c", "-z"].contains(&k.as_str())))
        .collect();
    res.extend(nb);
    res.into_iter().flat_map(|(_, v)| v).collect()
}

// ------------------------------------------------------------------ main

fn run_settings(o: &Opts, d: &mut Dev) -> Result<(), String> {
    if o.automatic || !o.effects.is_empty() || !o.colors.is_empty() {
        set_effects(o, d)?;
    }
    if let Some(v) = &o.dpi {
        dpi(o, d, v)?;
    }
    if let Some(v) = &o.poll {
        poll(o, d, v)?;
    }
    if !o.battery.is_empty() {
        battery(o, d)?;
    }
    if !o.brightness.is_empty() {
        set_brightness(o, d)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        println!("{HELP}");
        return ExitCode::from(1);
    }
    let o = match parse(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("usage: orzr-cli [-h] ...\norzr-cli: error: {e}");
            return ExitCode::from(2);
        }
    };
    if o.help {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if let Some(topics) = &o.manual {
        let names: Vec<&str> = MANUAL.iter().map(|(n, _)| *n).collect();
        if topics.is_empty() {
            println!("Manual entries exist for: {}", names.join(", "));
        }
        for t in topics {
            match MANUAL.iter().find(|(n, _)| n == t) {
                Some((n, text)) => println!("Manual Entry for --{n}:\n{text}\n"),
                None => println!("No manual entries exist for {t} try:\n   {}", names.join(", ")),
            }
        }
        return ExitCode::SUCCESS;
    }
    // the log file always records the run; --debug also prints it (to stderr)
    logs::init(if o.verbose || o.debug { logs::Level::Debug } else { logs::Level::Info }, o.debug, true);
    logs::startup_banner("orzr-cli");
    if let Some(m) = &o.openrazer {
        match compat::Mode::parse(m) {
            Some(m) => compat::set_mode(m),
            None => {
                eprintln!("orzr-cli: error: --openrazer auto|on|off");
                return ExitCode::from(2);
            }
        }
    }
    let needs_device = o.automatic
        || !o.effects.is_empty()
        || !o.colors.is_empty()
        || !o.brightness.is_empty()
        || o.dpi.is_some()
        || o.poll.is_some()
        || !o.battery.is_empty()
        || o.list > 0
        || o.restore;
    let mut code = ExitCode::SUCCESS;
    if needs_device {
        let mut devs = open_devices(&o);
        if o.list > 0 && (o.verbose || o.devices.is_empty()) {
            println!("Found {} Razer device(s)", devs.len());
        }
        if devs.is_empty() && o.list == 0 {
            eprintln!("orzr-cli: no Razer device found (close Razer Synapse / check access: open-rzr-cli install-udev)");
            return ExitCode::from(1);
        }
        let saved = if o.restore { load_saved() } else { HashMap::new() };
        if !o.devices.is_empty() && !devs.iter().any(|d| selected(&o, d)) {
            eprintln!("orzr-cli: no device matches {:?} (see orzr-cli -ls)", o.devices);
            code = ExitCode::from(1);
        }
        let to_save = setting_args(&args);
        for d in devs.iter_mut().filter(|d| selected(&o, d)) {
            if o.verbose && (o.automatic || !o.effects.is_empty() || !o.colors.is_empty()) {
                println!("Setting effects for {}:", d.m.name());
            }
            let mut r = run_settings(&o, d);
            if r.is_ok() && o.restore {
                match saved.get(&d.serial) {
                    Some(a) => {
                        if o.verbose {
                            println!("{}: restoring {}", d.m.name(), a.join(" "));
                        }
                        r = parse(a).and_then(|ro| run_settings(&Opts { verbose: o.verbose, ..ro }, d));
                    }
                    None => println!("{}: no saved settings", d.m.name()),
                }
            }
            match r {
                Ok(()) => {
                    if !o.restore {
                        save(&d.serial, &to_save);
                    }
                }
                Err(e) => {
                    eprintln!("{}: {e}", d.m.name());
                    code = ExitCode::from(1);
                }
            }
            if o.list > 0 {
                list(&o, d);
            }
        }
    }
    if o.version {
        println!("orzr-cli (open-rzr) {}", openrzr::VERSION);
        println!("Syntax compatible with razer-cli (https://github.com/LoLei/razer-cli)");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_like_razer_cli() {
        let o = parse(&s(&["-e", "static", "spectrum", "-z", "logo", "scroll_wheel,generic", "-c", "ff0000", "--dpi", "800,400", "-ll", "-d", "Razer Naga Pro (wired)"])).unwrap();
        assert_eq!(o.effects, s(&["static", "spectrum"]));
        assert_eq!(o.zones, s(&["logo", "scroll_wheel,generic"]));
        assert_eq!(o.dpi.as_deref(), Some("800,400"));
        assert_eq!(o.list, 2);
        assert_eq!(o.devices, s(&["Razer Naga Pro (wired)"]));
        assert_eq!(parse(&s(&["help", "dpi", "zone"])).unwrap().manual, Some(s(&["dpi", "zone"])));
        assert_eq!(parse(&s(&["--poll=500"])).unwrap().poll.as_deref(), Some("500"));
        assert!(parse(&s(&["-e"])).is_err());
        assert!(parse(&s(&["--bogus"])).is_err());
    }

    #[test]
    fn colours() {
        assert_eq!(parse_colors(&s(&["ff0000", "0", "255", "0", "#0000ff"])).unwrap(), vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]]);
        assert!(parse_colors(&s(&["ff00"])).is_err());
        assert!(parse_colors(&s(&["12", "13"])).is_err());
        assert_eq!(parse_colors(&s(&["random"])).unwrap().len(), 1);
    }

    #[test]
    fn restore_args_merge() {
        let a = setting_args(&s(&["-e", "static", "-c", "ff0000", "-ls", "--dpi", "800", "-v"]));
        assert_eq!(a, s(&["-e", "static", "-c", "ff0000", "--dpi", "800"]));
        let m = merge_args(&a, &s(&["--dpi", "1600", "-b", "50"]));
        assert_eq!(m, s(&["-e", "static", "-c", "ff0000", "--dpi", "1600", "-b", "50"]));
        let m2 = merge_args(&m, &s(&["-e", "spectrum"]));
        assert_eq!(m2, s(&["--dpi", "1600", "-b", "50", "-e", "spectrum"]));
        assert!(setting_args(&s(&["--dpi", "print"])).iter().all(|x| x != "print"));
    }
}
