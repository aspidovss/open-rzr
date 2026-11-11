//! Device descriptions from files: new mice (and other devices with the same protocol)
//! without recompiling.
//!
//! Folder: `<config dir>/devices/` — `*.toml` or `*.json`, one device per file
//! (Windows `%APPDATA%\open-rzr\devices`, Linux `~/.config/open-rzr/devices`).
//!
//! A file starts from an existing description and changes what it states:
//! * the OpenRazer database entry with the same `pid` (default when there is one);
//! * `base_pid = 0x008F` — a tested module or any known model (buttons, side plates, defaults);
//! * nothing — the common description of a current-generation mouse.
//!
//! Tested modules are not replaced unless the file says `override = true`.
//!
//! ```toml
//! pid = 0x00AB
//! name = "Razer Example"
//! base_pid = 0x008F            # optional
//! transaction_id = 0x1F
//! wireless = false
//! battery = false
//! max_dpi = 26000
//! features = ["firmware", "serial", "dpi", "dpi_stages", "polling", "lighting"]
//!
//! [[zones]]
//! id = "logo"
//! led = 0x04
//! effects = ["off", "static", "spectrum", "breath"]
//!
//! [[buttons]]
//! id = 0x04
//! name = "Side back"
//! ```

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::conf::{self, Value};
use crate::{feat, fx, openrazer_db, DeviceSpec, Source, Zone, MODULES, OPENRAZER_BASE};

/// What happened to one file.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Loaded {
    pub file: String,
    pub pid: u16,
    pub name: String,
    /// "openrazer" | "0x008f" (a known model) | "generic"
    pub base: String,
    pub error: String,
    pub warnings: Vec<String>,
}

/// Descriptions loaded from files (leaked once per load: they live as long as the program).
static USER: RwLock<Vec<&'static DeviceSpec>> = RwLock::new(Vec::new());
/// pids whose tested module a file replaces (`override = true`)
static OVERRIDES: RwLock<Vec<u16>> = RwLock::new(Vec::new());

/// The device folder under a configuration directory.
pub fn folder(config_dir: &Path) -> PathBuf {
    config_dir.join("devices")
}

/// Description from a file for `pid`, if any.
pub fn find(pid: u16) -> Option<&'static DeviceSpec> {
    USER.read().ok()?.iter().copied().find(|d| d.pid == pid)
}

/// Does a file replace the tested module of `pid`?
pub fn overrides(pid: u16) -> bool {
    OVERRIDES.read().map(|o| o.contains(&pid)).unwrap_or(false)
}

/// All descriptions from files.
pub fn all() -> Vec<&'static DeviceSpec> {
    USER.read().map(|u| u.clone()).unwrap_or_default()
}

/// Read every `*.toml` / `*.json` in `dir` and use them from now on (replaces earlier loads).
/// A missing folder is not an error.
pub fn load_dir(dir: &Path) -> Vec<Loaded> {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("toml" | "json")))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    let mut report = Vec::new();
    let mut specs: Vec<&'static DeviceSpec> = Vec::new();
    let mut over = Vec::new();
    for f in files {
        let mut l = Loaded { file: f.display().to_string(), ..Loaded::default() };
        match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| parse(&t, f.extension().and_then(|e| e.to_str()) == Some("json"), &mut l)) {
            Ok((spec, _)) if specs.iter().any(|s| s.pid == spec.pid) => {
                l.error = format!("product id 0x{:04x} is already described by another file", spec.pid);
            }
            Ok((spec, ov)) => {
                if ov {
                    over.push(spec.pid);
                }
                specs.push(Box::leak(Box::new(spec)));
            }
            Err(e) => l.error = e,
        }
        report.push(l);
    }
    if let Ok(mut u) = USER.write() {
        *u = specs;
    }
    if let Ok(mut o) = OVERRIDES.write() {
        *o = over;
    }
    report
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

const KEYS: &[&str] = &[
    "pid",
    "name",
    "base_pid",
    "override",
    "transaction_id",
    "interface",
    "wireless",
    "bluetooth",
    "battery",
    "min_dpi",
    "max_dpi",
    "dpi_stages",
    "profiles",
    "reply_delay_ms",
    "side_plates",
    "features",
    "openrazer_features",
    "zones",
    "buttons",
];

/// One description: the spec and whether it may replace a tested module.
pub fn parse(text: &str, json: bool, l: &mut Loaded) -> Result<(DeviceSpec, bool), String> {
    let v = if json { conf::parse_json(text)? } else { conf::parse_toml(text)? };
    if !matches!(v, Value::Table(_)) {
        return Err("the file must describe one device (a table / object)".into());
    }
    for k in v.keys() {
        if !KEYS.contains(&k) {
            l.warnings.push(format!("unknown key “{k}” (ignored)"));
        }
    }
    let int = |k: &str, min: i64, max: i64| -> Result<Option<i64>, String> {
        match v.get(k) {
            None => Ok(None),
            Some(x) => match x.as_int() {
                Some(i) if (min..=max).contains(&i) => Ok(Some(i)),
                Some(i) => Err(format!("“{k}” = {i} is out of range {min}..={max}")),
                None => Err(format!("“{k}” must be a number")),
            },
        }
    };
    let boolean = |k: &str| -> Result<Option<bool>, String> {
        match v.get(k) {
            None => Ok(None),
            Some(x) => x.as_bool().map(Some).ok_or_else(|| format!("“{k}” must be true or false")),
        }
    };
    let pid = int("pid", 1, 0xFFFF)?.ok_or("“pid” (product id) is required")? as u16;
    l.pid = pid;

    // what the file starts from
    let mut s = match int("base_pid", 1, 0xFFFF)? {
        Some(b) => {
            let base = MODULES
                .iter()
                .flat_map(|m| m.iter())
                .chain(openrazer_db::DEVICES.iter())
                .find(|d| d.pid == b as u16)
                .ok_or_else(|| format!("base_pid 0x{b:04x} is not a known model"))?;
            l.base = format!("0x{b:04x}");
            base.clone()
        }
        None => match openrazer_db::DEVICES.iter().find(|d| d.pid == pid) {
            Some(d) => {
                l.base = "openrazer".into();
                d.clone()
            }
            None => {
                l.base = "generic".into();
                DeviceSpec {
                    transaction_id: 0x1F,
                    direct: feat::FIRMWARE | feat::SERIAL | feat::DPI | feat::DPI_STAGES | feat::POLLING,
                    ..OPENRAZER_BASE
                }
            }
        },
    };
    s.pid = pid;
    s.source = Source::User;
    match v.get("name") {
        Some(n) => s.name = leak(n.as_str().ok_or("“name” must be text")?.trim()),
        None if s.name.is_empty() => return Err("“name” is required for a model OpenRazer does not know".into()),
        None => {}
    }
    if s.name.is_empty() {
        return Err("“name” is empty".into());
    }
    l.name = s.name.to_string();
    if let Some(t) = int("transaction_id", 0, 0xFF)? {
        s.transaction_id = t as u8;
    }
    if let Some(i) = int("interface", -1, 15)? {
        s.interface = i as i32;
    }
    if let Some(b) = boolean("wireless")? {
        s.wireless = b;
    }
    if let Some(b) = boolean("bluetooth")? {
        s.bluetooth = b;
    }
    if let Some(b) = boolean("battery")? {
        s.has_battery = b;
    }
    if let Some(d) = int("min_dpi", 50, 1000)? {
        s.min_dpi = d as u16;
    }
    if let Some(d) = int("max_dpi", 400, 65000)? {
        s.max_dpi = d as u16;
    }
    if s.min_dpi >= s.max_dpi {
        return Err(format!("min_dpi {} must be below max_dpi {}", s.min_dpi, s.max_dpi));
    }
    if let Some(n) = int("dpi_stages", 1, 5)? {
        s.max_dpi_stages = n as usize;
    }
    if let Some(n) = int("profiles", 0, 5)? {
        s.profiles = n as u8;
    }
    if let Some(n) = int("reply_delay_ms", 0, 500)? {
        s.reply_delay_ms = n as u64;
    }
    if let Some(b) = boolean("side_plates")? {
        s.side_plates = b;
    }
    if let Some(f) = v.get("features") {
        s.direct = features(f, "features", l)?;
    }
    if let Some(f) = v.get("openrazer_features") {
        s.openrazer = features(f, "openrazer_features", l)?;
    }
    if let Some(z) = v.get("zones") {
        let mut zones = Vec::new();
        for (i, item) in z.as_array().ok_or("“zones” must be a list of [[zones]]")?.iter().enumerate() {
            let id = item.get("id").and_then(|x| x.as_str()).ok_or_else(|| format!("zone {}: “id” is required", i + 1))?;
            let led = item.get("led").and_then(|x| x.as_int()).filter(|x| (0..=0xFF).contains(x)).ok_or_else(|| format!("zone “{id}”: “led” (0..255) is required"))?;
            let mut effects = 0u16;
            for e in item.get("effects").and_then(|x| x.as_array()).unwrap_or(&[]) {
                effects |= match e.as_str().unwrap_or("") {
                    "off" => fx::OFF,
                    "static" => fx::STATIC,
                    "spectrum" => fx::SPECTRUM,
                    "wave" => fx::WAVE,
                    "breath" | "breathing" => fx::BREATH,
                    "reactive" => fx::REACTIVE,
                    "custom" => fx::CUSTOM,
                    other => return Err(format!("zone “{id}”: unknown effect “{other}” (off, static, spectrum, wave, breath, reactive, custom)")),
                };
            }
            if effects == 0 {
                effects = fx::OFF | fx::STATIC;
                l.warnings.push(format!("zone “{id}”: no effects listed, using off and static"));
            }
            zones.push(Zone {
                id: leak(id),
                led: led as u8,
                effects,
                direct: item.get("direct").and_then(|x| x.as_bool()).unwrap_or(true),
                openrazer: item.get("openrazer").and_then(|x| x.as_bool()).unwrap_or(false),
            });
        }
        s.zones = Box::leak(zones.into_boxed_slice());
        if s.zones.iter().any(|z| z.direct) {
            s.direct |= feat::LIGHTING;
        }
    }
    if let Some(b) = v.get("buttons") {
        let mut list: Vec<(u8, &'static str)> = s.buttons.to_vec();
        for (i, item) in b.as_array().ok_or("“buttons” must be a list of [[buttons]]")?.iter().enumerate() {
            let id = item.get("id").and_then(|x| x.as_int()).filter(|x| (1..=0xFF).contains(x)).ok_or_else(|| format!("button {}: “id” (1..255) is required", i + 1))? as u8;
            let name = leak(item.get("name").and_then(|x| x.as_str()).ok_or_else(|| format!("button 0x{id:02x}: “name” is required"))?);
            match list.iter_mut().find(|(b, _)| *b == id) {
                Some(e) => e.1 = name,
                None => list.push((id, name)),
            }
        }
        list.sort_by_key(|(b, _)| *b);
        s.buttons = Box::leak(list.into_boxed_slice());
    }
    if s.direct & (feat::BUTTONS | feat::MACROS | feat::PROFILES) != 0 && s.transaction_id != 0x1F && s.transaction_id != 0x3F {
        l.warnings.push("buttons, macros and profiles are only known to work with transaction id 0x1F / 0x3F".into());
    }
    let ov = boolean("override")?.unwrap_or(false);
    if MODULES.iter().flat_map(|m| m.iter()).any(|d| d.pid == pid && d.source == Source::Verified) && !ov {
        l.warnings.push("a tested module exists for this product id and is used instead; add `override = true` to use this file".into());
    }
    Ok((s, ov))
}

fn features(v: &Value, key: &str, l: &mut Loaded) -> Result<u32, String> {
    let mut m = 0;
    for f in v.as_array().ok_or_else(|| format!("“{key}” must be a list"))? {
        let n = f.as_str().ok_or_else(|| format!("“{key}”: names in quotes"))?;
        match feat::ALL.iter().find(|(_, name)| *name == n) {
            Some((bit, _)) => m |= bit,
            None => l.warnings.push(format!("“{key}”: unknown feature “{n}” (ignored)")),
        }
    }
    Ok(m)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn p(text: &str) -> (Result<(DeviceSpec, bool), String>, Loaded) {
        let mut l = Loaded::default();
        (parse(text, false, &mut l), l)
    }

    #[test]
    fn new_model_from_generic() {
        let (r, l) = p(
            r#"
pid = 0x7FF1
name = "Razer Test Mouse"
max_dpi = 26000
battery = true
wireless = true
features = ["firmware", "dpi", "battery", "bogus"]
[[zones]]
id = "logo"
led = 4
effects = ["off", "static", "breath"]
[[buttons]]
id = 0x60
name = "DPI down"
"#,
        );
        let (s, ov) = r.unwrap();
        assert!(!ov);
        assert_eq!((s.pid, s.name, s.source, s.max_dpi, s.transaction_id), (0x7FF1, "Razer Test Mouse", Source::User, 26000, 0x1F));
        assert!(s.wireless && s.has_battery);
        assert_eq!(s.direct, feat::FIRMWARE | feat::DPI | feat::BATTERY | feat::LIGHTING);
        assert_eq!(s.zones[0].effects, fx::OFF | fx::STATIC | fx::BREATH);
        assert!(s.buttons.iter().any(|b| *b == (0x60, "DPI down")) && s.buttons.iter().any(|b| b.0 == 0x01));
        assert_eq!(l.base, "generic");
        assert!(l.warnings.iter().any(|w| w.contains("bogus")));
    }

    #[test]
    fn starts_from_openrazer_and_known_models() {
        // a database model: only the name changes
        let db = &openrazer_db::DEVICES[openrazer_db::DEVICES.len() / 2];
        let (r, l) = p(&format!("pid = {}\nname = \"Mine\"", db.pid));
        let s = r.unwrap().0;
        assert_eq!((l.base.as_str(), s.max_dpi, s.direct, s.name), ("openrazer", db.max_dpi, db.direct, "Mine"));
        // a sibling of the Naga Pro: buttons, plates and defaults are inherited
        let (r, l) = p("pid = 0x7FF2\nname = \"Naga clone\"\nbase_pid = 0x008F");
        let s = r.unwrap().0;
        assert_eq!(l.base, "0x008f");
        assert!(s.side_plates && s.direct & feat::BUTTONS != 0);
        // the tested module stays first unless overridden
        let (r, l) = p("pid = 0x008F");
        assert!(!r.unwrap().1 && l.warnings.iter().any(|w| w.contains("override")));
    }

    #[test]
    fn errors() {
        assert!(p("name = \"x\"").0.unwrap_err().contains("pid"));
        assert!(p("pid = 0x7FF3").0.unwrap_err().contains("name"));
        assert!(p("pid = 0x7FF3\nname = \"x\"\nmax_dpi = 10").0.unwrap_err().contains("max_dpi"));
        assert!(p("pid = 0x7FF3\nname = \"x\"\nbase_pid = 0x7FFF").0.unwrap_err().contains("base_pid"));
        assert!(p("pid = 0x7FF3\nname = \"x\"\n[[zones]]\nid = \"a\"\nled = 1\neffects = [\"disco\"]").0.unwrap_err().contains("disco"));
        let (r, l) = p("pid = 0x7FF3\nname = \"x\"\ncolour = 1");
        assert!(r.is_ok() && l.warnings[0].contains("colour"));
        // JSON gives the same result
        let mut l = Loaded::default();
        let s = parse(r#"{"pid": "0x7FF4", "name": "J", "max_dpi": 8000}"#, true, &mut l).unwrap().0;
        assert_eq!((s.pid, s.max_dpi), (0x7FF4, 8000));
    }

    #[test]
    fn folder_is_loaded_and_found() {
        let dir = std::env::temp_dir().join(format!("openrzr-devices-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.toml"), "pid = 0x7FF5\nname = \"From file\"").unwrap();
        std::fs::write(dir.join("b.json"), "{\"pid\": 32757, \"name\": \"Twin\"}").unwrap(); // same pid: rejected
        std::fs::write(dir.join("c.toml"), "pid = ").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let r = load_dir(&dir);
        assert_eq!(r.len(), 3);
        assert!(r[0].error.is_empty() && r[1].error.contains("already") && !r[2].error.is_empty());
        assert_eq!(crate::spec_for(0x7FF5).unwrap().name, "From file");
        assert!(crate::all().iter().any(|d| d.pid == 0x7FF5));
        load_dir(&dir.join("missing"));
        assert!(find(0x7FF5).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
