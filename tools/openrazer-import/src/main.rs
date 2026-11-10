//! openrazer-import — builds `crates/rzr-core/src/devices/openrazer_db.rs` from an OpenRazer source tree.
//!
//! Maintainer tool, not part of the program. Usage:
//!
//! ```text
//! git clone --depth 1 https://github.com/openrazer/openrazer
//! cargo run -p openrazer-import -- path/to/openrazer crates/rzr-core/src/devices/openrazer_db.rs
//! ```
//!
//! Only facts are taken (USB product ids, model names, which sysfs attributes and which
//! request types the OpenRazer kernel driver uses for a model, maximum DPI). No OpenRazer code
//! is copied into open-rzr. OpenRazer is GPL-2.0: https://github.com/openrazer/openrazer
//!
//! For every mouse it records:
//! * `openrazer` — features available through the OpenRazer kernel driver (sysfs attributes);
//! * `direct`    — features open-rzr can talk to directly over HID, because the driver uses for
//!   that model exactly the same request open-rzr sends, with the same transaction id.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE: &str = "usage: openrazer-import <openrazer-source-dir> [output.rs]";

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Remove // and /* */ comments, keeping string literals intact.
fn strip_comments(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let start = i;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
                out.push_str(&s[start..i.min(b.len())]);
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    if b[i] == b'\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
                i += 2;
            }
            _ => {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

/// Byte range of the `{ ... }` block that starts at the first '{' at or after `from` (contents only).
fn block_after(s: &str, from: usize) -> Option<(usize, usize)> {
    let open = from + s[from..].find('{')?;
    let mut depth = 0i32;
    for (i, c) in s[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open + 1, open + i));
                }
            }
            _ => {}
        }
    }
    None
}

struct Src {
    c: String,
    bodies: HashMap<String, String>,
}

impl Src {
    fn new(c: String) -> Src {
        let mut bodies = HashMap::new();
        let mut pos = 0;
        while let Some(i) = c[pos..].find("static ssize_t ") {
            let start = pos + i + "static ssize_t ".len();
            let name: String = c[start..].chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
            let after = start + name.len();
            if c[after..].starts_with('(') {
                let sig_end = after + c[after..].find(')').unwrap_or(0);
                // prototype (";") or definition ("{")
                let rest = c[sig_end + 1..].trim_start();
                if rest.starts_with('{') {
                    if let Some((a, b)) = block_after(&c, sig_end) {
                        bodies.insert(name, c[a..b].to_string());
                        pos = b;
                        continue;
                    }
                }
            }
            pos = after;
        }
        Src { c, bodies }
    }
}

#[derive(Default, Debug)]
struct Switch {
    /// case label (constant name or "default") -> statements of its group, fallthrough included
    groups: HashMap<String, String>,
}

/// Top-level `switch (...usb_pid...)` statements of a function body and the text outside them.
fn switches(body: &str) -> (Vec<Switch>, String) {
    let mut out = Vec::new();
    let mut outside = String::new();
    let mut pos = 0;
    loop {
        let Some(i) = find_switch(&body[pos..]) else {
            outside.push_str(&body[pos..]);
            break;
        };
        let at = pos + i;
        outside.push_str(&body[pos..at]);
        let Some((a, b)) = block_after(body, at) else { break };
        out.push(parse_switch(&body[a..b]));
        pos = b + 1;
    }
    (out, outside)
}

fn find_switch(s: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = s[from..].find("switch") {
        let at = from + i;
        let head = &s[at..];
        let paren_end = head.find(')').unwrap_or(0);
        if head[6..].trim_start().starts_with('(') && head[..paren_end].contains("usb_pid") {
            return Some(at);
        }
        from = at + 6;
    }
    None
}

fn parse_switch(block: &str) -> Switch {
    let mut sw = Switch::default();
    let mut active: Vec<String> = Vec::new();
    let mut closed = true;
    let mut depth = 0i32;
    let mut prev_if = false;
    for line in block.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if depth == 0 && (t.starts_with("case ") || t.starts_with("default:") || t.starts_with("default :")) {
            if closed {
                active.clear();
                closed = false;
            }
            let label = if t.starts_with("default") {
                "default".to_string()
            } else {
                t["case ".len()..].split(':').next().unwrap_or("").trim().to_string()
            };
            sw.groups.entry(label.clone()).or_default();
            active.push(label);
            // "case X: statement;" on one line
            let rest = t.split_once(':').map(|x| x.1.trim()).unwrap_or("");
            if !rest.is_empty() {
                for l in &active {
                    sw.groups.get_mut(l).unwrap().push_str(rest);
                    sw.groups.get_mut(l).unwrap().push('\n');
                }
            }
            continue;
        }
        for l in &active {
            let g = sw.groups.get_mut(l).unwrap();
            g.push_str(t);
            g.push('\n');
        }
        if depth == 0 {
            if t == "break;" || (t.starts_with("return") && !prev_if) {
                closed = true;
            }
        }
        prev_if = depth == 0 && (t.starts_with("if") || t.starts_with("} else if") || t == "else")
            && !t.ends_with('{')
            && !t.ends_with(';');
        depth += t.matches('{').count() as i32 - t.matches('}').count() as i32;
    }
    sw
}

fn requests(text: &str) -> BTreeSet<String> {
    let mut v = BTreeSet::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("request = ") {
        let at = from + i + "request = ".len();
        let name: String = text[at..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !name.is_empty() && text[at + name.len()..].starts_with('(') {
            v.insert(name);
        }
        from = at;
    }
    v
}

fn tids(text: &str) -> BTreeSet<u8> {
    let mut v = BTreeSet::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("transaction_id.id = ") {
        let at = from + i + "transaction_id.id = ".len();
        let tok: String = text[at..].chars().take_while(|c| c.is_alphanumeric()).collect();
        if let Some(h) = tok.strip_prefix("0x").or_else(|| tok.strip_prefix("0X")) {
            if let Ok(x) = u8::from_str_radix(h, 16) {
                v.insert(x);
            }
        }
        from = at;
    }
    v
}

#[derive(Debug, Clone)]
enum Res {
    /// the driver does not support this for the model
    Unsupported,
    /// handled by model-specific code in the driver (different packet layout)
    Custom,
    Ok { reqs: BTreeSet<String>, tids: BTreeSet<u8>, led: Option<String> },
}

impl Res {
    fn compatible(&self, allowed: &[&str], tid: u8) -> bool {
        match self {
            Res::Ok { reqs, tids, .. } => {
                !reqs.is_empty()
                    && reqs.iter().all(|r| allowed.contains(&r.as_str()))
                    && tids.iter().all(|t| *t == tid)
            }
            _ => false,
        }
    }
}

fn analyze(src: &Src, func: &str, pid: &str) -> Res {
    analyze_led(src, func, pid, None)
}

fn analyze_led(src: &Src, func: &str, pid: &str, led: Option<String>) -> Res {
    let Some(body) = src.bodies.get(func) else { return Res::Unsupported };
    let (sws, outside) = switches(body);
    // thin wrapper: return common(dev, attr, buf[, count], LED);
    if sws.is_empty() {
        let t = outside.trim();
        if let Some(rest) = t.strip_prefix("return ") {
            if let Some((callee, args)) = rest.split_once('(') {
                let args: Vec<&str> = args.trim_end_matches(';').trim_end_matches(')').split(',').map(str::trim).collect();
                if args.first() == Some(&"dev") && src.bodies.contains_key(callee) {
                    let led = args.last().filter(|a| a.ends_with("_LED")).map(|a| a.to_string());
                    return analyze_led(src, callee, pid, led);
                }
            }
        }
    }
    let mut reqs = requests(&outside);
    let mut ts = tids(&outside);
    for sw in &sws {
        let listed = sw.groups.get(pid);
        let g = match listed.or_else(|| sw.groups.get("default")) {
            Some(g) => g,
            None => continue,
        };
        if listed.is_none() && g.contains("-EINVAL") {
            return Res::Unsupported;
        }
        if g.contains("return count") || g.contains("return sysfs_emit") {
            return Res::Custom;
        }
        reqs.extend(requests(g));
        ts.extend(tids(g));
    }
    Res::Ok { reqs, tids: ts, led }
}

// ------------------------------------------------------------------ features (match crates/rzr-core/src/devices.rs)
const FEATURES: &[(&str, &str, &[(&str, &[&str])])] = &[
    // (flag, sysfs attribute, [(driver function, requests open-rzr sends the same way)])
    ("FIRMWARE", "firmware_version", &[("razer_attr_read_firmware_version", &["razer_chroma_standard_get_firmware_version"])]),
    ("SERIAL", "device_serial", &[("razer_attr_read_device_serial", &["razer_chroma_standard_get_serial"])]),
    (
        "DPI",
        "dpi",
        &[
            ("razer_attr_write_dpi", &["razer_chroma_misc_set_dpi_xy"]),
            ("razer_attr_read_dpi", &["razer_chroma_misc_get_dpi_xy"]),
        ],
    ),
    (
        "DPI_STAGES",
        "dpi_stages",
        &[
            ("razer_attr_write_dpi_stages", &["razer_chroma_misc_set_dpi_stages"]),
            ("razer_attr_read_dpi_stages", &["razer_chroma_misc_get_dpi_stages"]),
        ],
    ),
    (
        "POLLING",
        "poll_rate",
        &[
            ("razer_attr_write_poll_rate", &["razer_chroma_misc_set_polling_rate"]),
            ("razer_attr_read_poll_rate", &["razer_chroma_misc_get_polling_rate"]),
        ],
    ),
    ("BATTERY", "charge_level", &[("razer_attr_read_charge_level", &["razer_chroma_misc_get_battery_level"])]),
    ("CHARGING", "charge_status", &[("razer_attr_read_charge_status", &["razer_chroma_misc_get_charging_status"])]),
    (
        "IDLE",
        "device_idle_time",
        &[
            ("razer_attr_write_device_idle_time", &["razer_chroma_misc_set_idle_time"]),
            ("razer_attr_read_device_idle_time", &["razer_chroma_misc_get_idle_time"]),
        ],
    ),
    (
        "LOW_BATTERY",
        "charge_low_threshold",
        &[
            ("razer_attr_write_charge_low_threshold", &["razer_chroma_misc_set_low_battery_threshold"]),
            ("razer_attr_read_charge_low_threshold", &["razer_chroma_misc_get_low_battery_threshold"]),
        ],
    ),
];

/// (sysfs prefix, open-rzr zone id)
const ZONES: &[(&str, &str)] =
    &[("scroll_", "scroll"), ("logo_", "logo"), ("left_", "left"), ("right_", "right"), ("backlight_", "backlight"), ("", "matrix")];

/// (sysfs effect suffix, fx flag)
const EFFECTS: &[(&str, &str)] = &[
    ("none", "OFF"),
    ("static", "STATIC"),
    ("spectrum", "SPECTRUM"),
    ("wave", "WAVE"),
    ("breath", "BREATH"),
    ("reactive", "REACTIVE"),
    ("custom", "CUSTOM"),
];

struct Mouse {
    pid: u16,
    konst: String,
    name: String,
    attrs: BTreeSet<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).expect(USAGE));
    let out = args.get(2).map(PathBuf::from).unwrap_or_else(|| "crates/rzr-core/src/devices/openrazer_db.rs".into());

    let h = read(&root.join("driver/razermouse_driver.h"));
    let common_h = read(&root.join("driver/razercommon.h"));
    let src = Src::new(strip_comments(&read(&root.join("driver/razermouse_driver.c"))));
    let py = read(&root.join("daemon/openrazer_daemon/hardware/mouse.py"));
    let rev = Command::new("git")
        .args(["-C", &root.to_string_lossy(), "log", "-1", "--format=%h %cs"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown revision".into());

    // #define USB_DEVICE_ID_RAZER_X 0xNNNN
    let mut pids: HashMap<String, u16> = HashMap::new();
    for l in h.lines() {
        let p: Vec<&str> = l.split_whitespace().collect();
        if p.len() >= 3 && p[0] == "#define" && p[1].starts_with("USB_DEVICE_ID_RAZER_") {
            if let Some(x) = p[2].strip_prefix("0x").or_else(|| p[2].strip_prefix("0X")) {
                if let Ok(v) = u16::from_str_radix(x, 16) {
                    pids.insert(p[1].to_string(), v);
                }
            }
        }
    }
    let mut leds: HashMap<String, u8> = HashMap::new();
    for l in common_h.lines() {
        let p: Vec<&str> = l.split_whitespace().collect();
        if p.len() >= 3 && p[0] == "#define" && p[1].ends_with("_LED") {
            if let Some(x) = p[2].strip_prefix("0x") {
                if let Ok(v) = u8::from_str_radix(x, 16) {
                    leds.insert(p[1].to_string(), v);
                }
            }
        }
    }

    // model names: razer_attr_read_device_type
    let mut names: HashMap<String, String> = HashMap::new();
    let (sws, _) = switches(&src.bodies["razer_attr_read_device_type"]);
    for sw in &sws {
        for (label, g) in &sw.groups {
            if let Some(i) = g.find("device_type = \"") {
                let rest = &g[i + 15..];
                names.insert(label.clone(), rest[..rest.find('"').unwrap()].to_string());
            }
        }
    }

    // attributes per model: the switch in razer_mouse_probe for the mouse interface
    let probe_at = src.c.find("static int razer_mouse_probe(").expect("razer_mouse_probe");
    let (pa, pb) = block_after(&src.c, probe_at).unwrap();
    let probe = &src.c[pa..pb];
    let mouse_if = probe.find("USB_INTERFACE_PROTOCOL_MOUSE").expect("mouse interface branch");
    let sw_at = mouse_if + find_switch(&probe[mouse_if..]).expect("probe switch");
    let (sa, sb) = block_after(probe, sw_at).unwrap();
    let probe_sw = parse_switch(&probe[sa..sb]);
    let mut common_attrs = BTreeSet::new();
    for a in ["firmware_version", "device_type", "device_serial"] {
        common_attrs.insert(a.to_string());
    }

    let mut mice: Vec<Mouse> = Vec::new();
    for (label, g) in &probe_sw.groups {
        let Some(&pid) = pids.get(label) else { continue };
        let mut attrs = common_attrs.clone();
        let mut from = 0;
        while let Some(i) = g[from..].find("&dev_attr_") {
            let at = from + i + 10;
            let a: String = g[at..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            attrs.insert(a);
            from = at;
        }
        let name = names.get(label).cloned().unwrap_or_else(|| label.trim_start_matches("USB_DEVICE_ID_").replace('_', " "));
        mice.push(Mouse { pid, konst: label.clone(), name, attrs });
    }
    mice.sort_by_key(|m| m.pid);

    // maximum DPI from the daemon (with class inheritance)
    let dpi_max = python_dpi_max(&py);

    let mut code = String::new();
    writeln!(
        code,
        "//! Razer mice known to OpenRazer — GENERATED by tools/openrazer-import, do not edit by hand.\n\
         //!\n\
         //! Source: OpenRazer (https://github.com/openrazer/openrazer, GPL-2.0), revision {rev}:\n\
         //! driver/razermouse_driver.{{c,h}} and daemon/openrazer_daemon/hardware/mouse.py.\n\
         //! Only facts are recorded (product ids, names, supported features, maximum DPI);\n\
         //! no OpenRazer code is included. Thank you, OpenRazer community!\n\
         //!\n\
         //! `openrazer` — features available through the OpenRazer kernel driver (Linux, compatibility mode).\n\
         //! `direct`    — features open-rzr sends itself: the driver uses the same request and\n\
         //!               transaction id for this model. Not tested on real hardware unless\n\
         //!               the model has its own module (see naga_pro.rs).\n\n\
         use super::{{feat, fx, DeviceSpec, Source, Zone, OPENRAZER_BASE}};\n\n\
         pub const REVISION: &str = \"{rev}\";\n\n\
         pub static DEVICES: &[DeviceSpec] = &["
    )
    .unwrap();

    let mut stats: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unknown_reqs: BTreeSet<String> = BTreeSet::new();
    for m in &mice {
        // transaction id: the one the driver uses to read the firmware version, else the most common
        let mut tid_votes: BTreeMap<u8, usize> = BTreeMap::new();
        let mut results: Vec<(&str, &str, Vec<(Res, &[&str])>)> = Vec::new();
        for (flag, attr, funcs) in FEATURES {
            let rs: Vec<(Res, &[&str])> = funcs.iter().map(|(f, allowed)| (analyze(&src, f, &m.konst), *allowed)).collect();
            for (r, allowed) in &rs {
                if let Res::Ok { tids, reqs, .. } = r {
                    for t in tids {
                        *tid_votes.entry(*t).or_default() += 1;
                    }
                    for q in reqs {
                        if !allowed.contains(&q.as_str()) {
                            unknown_reqs.insert(format!("{q} ({flag})"));
                        }
                    }
                }
            }
            results.push((flag, attr, rs));
        }
        let fw = &results[0].2[0].0;
        let tid = match fw {
            Res::Ok { tids, .. } if tids.len() == 1 => *tids.iter().next().unwrap(),
            _ => tid_votes.iter().max_by_key(|(_, n)| **n).map(|(t, _)| *t).unwrap_or(0xFF),
        };
        let fw_direct = fw.compatible(&["razer_chroma_standard_get_firmware_version"], tid);

        let mut or_flags = Vec::new();
        let mut direct_flags = Vec::new();
        for (flag, attr, rs) in &results {
            if m.attrs.contains(*attr) {
                or_flags.push(*flag);
                if fw_direct && rs.iter().all(|(r, allowed)| r.compatible(allowed, tid)) {
                    direct_flags.push(*flag);
                }
            }
        }

        // lighting zones
        let mut zones = Vec::new();
        for (prefix, zone) in ZONES {
            let mut fx_flags = Vec::new();
            for (suffix, f) in EFFECTS {
                let attr = format!("{prefix}matrix_effect_{suffix}");
                if m.attrs.contains(&attr) && (*suffix != "custom" || m.attrs.contains("matrix_custom_frame")) {
                    fx_flags.push(*f);
                }
            }
            if fx_flags.is_empty() {
                continue;
            }
            let bright_attr = if prefix.is_empty() { "matrix_brightness".to_string() } else { format!("{prefix}led_brightness") };
            let has_bright = m.attrs.contains(&bright_attr);
            let st = analyze(&src, &format!("razer_attr_write_{prefix}matrix_effect_static"), &m.konst);
            let br = if has_bright {
                analyze(&src, &format!("razer_attr_write_{bright_attr}"), &m.konst)
            } else {
                Res::Unsupported
            };
            let led = match &st {
                Res::Ok { led: Some(l), .. } => leds.get(l).copied(),
                _ => None,
            };
            let direct = fw_direct
                && led.is_some_and(|l| l != 0)
                && st.compatible(&["razer_chroma_extended_matrix_effect_static"], tid)
                && br.compatible(&["razer_chroma_extended_matrix_brightness"], tid)
                && m.attrs.contains(&format!("{prefix}matrix_effect_static"));
            let led = led.unwrap_or(0);
            zones.push(format!(
                "Zone {{ id: \"{zone}\", led: 0x{led:02X}, effects: {}, direct: {direct}, openrazer: true }}",
                fx_flags.iter().map(|f| format!("fx::{f}")).collect::<Vec<_>>().join(" | ")
            ));
        }
        if !zones.is_empty() {
            or_flags.push("LIGHTING");
            if zones.iter().any(|z| z.contains("direct: true")) {
                direct_flags.push("LIGHTING");
            }
        }

        let battery = m.attrs.contains("charge_level");
        let wireless = !m.name.contains("Wired")
            && ["Wireless", "Receiver", "HyperSpeed", "Bluetooth"].iter().any(|w| m.name.contains(w));
        let max_dpi = dpi_max.get(&m.pid).copied().unwrap_or(0);
        let flags = |v: &[&str]| {
            if v.is_empty() {
                "0".to_string()
            } else {
                v.iter().map(|f| format!("feat::{f}")).collect::<Vec<_>>().join(" | ")
            }
        };
        *stats.entry("models").or_default() += 1;
        if !direct_flags.is_empty() {
            *stats.entry("with direct features").or_default() += 1;
        }
        writeln!(
            code,
            "    DeviceSpec {{\n        pid: 0x{:04X},\n        name: \"{}\",\n        source: Source::OpenRazer,\n        \
             transaction_id: 0x{tid:02X},\n        wireless: {wireless},\n        has_battery: {battery},\n        \
             max_dpi: {},\n        reply_delay_ms: {},\n        zones: &[{}],\n        direct: {},\n        openrazer: {},\n        \
             ..OPENRAZER_BASE\n    }},",
            m.pid,
            m.name.replace('"', "'"),
            if max_dpi == 0 { 16000 } else { max_dpi },
            if wireless { 35 } else { 5 },
            zones.join(", "),
            flags(&direct_flags),
            flags(&or_flags),
        )
        .unwrap();
    }
    code.push_str("];\n");
    fs::write(&out, code).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    eprintln!("OpenRazer {rev}: {stats:?} -> {}", out.display());
    if !unknown_reqs.is_empty() {
        eprintln!("requests open-rzr does not send (features stay OpenRazer-only):");
        for q in unknown_reqs {
            eprintln!("  {q}");
        }
    }
}

/// pid -> DPI_MAX from the daemon's mouse classes, following `class A(B)` inheritance.
fn python_dpi_max(py: &str) -> HashMap<u16, u16> {
    struct Class {
        parent: String,
        pids: Vec<u16>,
        dpi: Option<u16>,
    }
    let mut classes: HashMap<String, Class> = HashMap::new();
    let mut cur: Option<String> = None;
    for l in py.lines() {
        if let Some(rest) = l.strip_prefix("class ") {
            let name = rest.split(['(', ':']).next().unwrap_or("").trim().to_string();
            let parent = rest.split_once('(').map(|x| x.1.split(')').next().unwrap_or("").trim().to_string()).unwrap_or_default();
            classes.insert(name.clone(), Class { parent, pids: Vec::new(), dpi: None });
            cur = Some(name);
            continue;
        }
        let Some(c) = cur.as_ref().and_then(|n| classes.get_mut(n)) else { continue };
        let t = l.trim();
        if let Some(v) = t.strip_prefix("USB_PID") {
            let v = v.trim_start().trim_start_matches('=').trim();
            if let Some(h) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
                if let Ok(x) = u16::from_str_radix(h, 16) {
                    c.pids.push(x);
                }
            }
        } else if let Some(v) = t.strip_prefix("DPI_MAX") {
            if let Ok(x) = v.trim_start().trim_start_matches('=').trim().parse() {
                c.dpi = Some(x);
            }
        }
    }
    let mut out = HashMap::new();
    for c in classes.values() {
        let mut dpi = c.dpi;
        let mut parent = c.parent.clone();
        let mut guard = 0;
        while dpi.is_none() && guard < 10 {
            let Some(p) = classes.get(&parent) else { break };
            dpi = p.dpi;
            parent = p.parent.clone();
            guard += 1;
        }
        if let Some(d) = dpi {
            for pid in &c.pids {
                out.insert(*pid, d);
            }
        }
    }
    out
}
