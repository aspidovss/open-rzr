//! A mouse profile kept on the computer: everything that makes up one on-board profile,
//! plus a name and the programs it is linked to. Stored as JSON (one file per profile),
//! exported/imported as the same JSON ("open-rzr-profile", version 1).

use std::collections::BTreeMap;

use crate::bindings::Action;
use crate::features::dpi::DpiStages;
use crate::lighting::ZoneState;
use crate::macros::MacroStep;

pub const FORMAT: &str = "open-rzr-profile";
pub const VERSION: u32 = 1;

/// Binding of one button in both layers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(default))]
pub struct ButtonPair {
    pub normal: Action,
    pub hypershift: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(default))]
pub struct Profile {
    pub format: String,
    pub version: u32,
    /// file name without extension, unique in the library
    pub id: String,
    pub name: String,
    /// model it was made on (a profile can still be written to another mouse of the same family)
    pub device: String,
    pub pid: u16,
    /// button id as "0x40" -> bindings
    pub buttons: BTreeMap<String, ButtonPair>,
    pub dpi_stages: Option<DpiStages>,
    pub polling: Option<u32>,
    /// zone -> lighting (lighting is not stored per on-board profile: applied when the profile is activated)
    pub lighting: BTreeMap<String, ZoneState>,
    /// macro id as string -> steps, for the macros the buttons use
    pub macros: BTreeMap<String, Vec<MacroStep>>,
    /// programs that switch to this profile (file names like "game.exe", or full paths)
    pub links: Vec<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            format: FORMAT.into(),
            version: VERSION,
            id: String::new(),
            name: String::new(),
            device: String::new(),
            pid: 0,
            buttons: BTreeMap::new(),
            dpi_stages: None,
            polling: None,
            lighting: BTreeMap::new(),
            macros: BTreeMap::new(),
            links: Vec::new(),
        }
    }
}

pub fn key(id: u8) -> String {
    format!("0x{id:02x}")
}

pub fn parse_key(k: &str) -> Option<u8> {
    u8::from_str_radix(k.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
}

impl Profile {
    pub fn button(&self, id: u8) -> Option<&ButtonPair> {
        self.buttons.get(&key(id))
    }

    pub fn set(&mut self, id: u8, hypershift: bool, a: Action) {
        let e = self.buttons.entry(key(id)).or_default();
        if hypershift {
            e.hypershift = Some(a);
        } else {
            e.normal = a;
        }
    }

    /// Macro ids used by the bindings.
    pub fn used_macros(&self) -> Vec<u16> {
        let mut v: Vec<u16> = Vec::new();
        for p in self.buttons.values() {
            for a in std::iter::once(&p.normal).chain(p.hypershift.iter()) {
                if a.kind == "macro" && !v.contains(&a.macro_id) {
                    v.push(a.macro_id);
                }
            }
        }
        v
    }

    /// Does a program (full path or file name) belong to this profile?
    pub fn linked_to(&self, exe: &str) -> bool {
        let name = file_name(exe);
        self.links.iter().any(|l| l.eq_ignore_ascii_case(exe) || file_name(l).eq_ignore_ascii_case(name))
    }

    /// Factory bindings for every button the profile knows (`defaults` from the device module);
    /// buttons without a known default keep their binding.
    pub fn reset_to(&mut self, defaults: fn(u8) -> Option<Action>) {
        for (k, p) in self.buttons.iter_mut() {
            if let Some(a) = parse_key(k).and_then(defaults) {
                p.normal = a.clone();
                p.hypershift = Some(a);
            }
        }
        self.macros.clear();
    }

    /// Buttons that hold Hypershift (their standard binding is "hypershift").
    pub fn hypershift_buttons(&self) -> Vec<u8> {
        self.buttons.iter().filter(|(_, p)| p.normal.kind == "hypershift").filter_map(|(k, _)| parse_key(k)).collect()
    }

    /// Hypershift layer operation -> the bindings to write into the Hypershift layer.
    /// `copy`: standard → Hypershift; `clear`: every Hypershift binding off.
    /// Hypershift buttons keep "hypershift" in both layers (releasing them must end the layer).
    pub fn layer_plan(&self, op: &str) -> Result<Vec<(u8, Action)>, String> {
        let mut v = Vec::new();
        for (k, p) in &self.buttons {
            let Some(id) = parse_key(k) else { continue };
            let a = if p.normal.kind == "hypershift" {
                p.normal.clone()
            } else {
                match op {
                    "copy" => p.normal.clone(),
                    "clear" => Action::kind("disabled"),
                    _ => return Err(format!("unknown layer operation {op}")),
                }
            };
            if p.hypershift.as_ref() != Some(&a) {
                v.push((id, a));
            }
        }
        Ok(v)
    }

    /// Check an imported profile.
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!("not an open-rzr profile (format \"{}\")", self.format));
        }
        if self.version > VERSION {
            return Err(format!("profile version {} is newer than this program supports ({VERSION})", self.version));
        }
        for (k, p) in &self.buttons {
            parse_key(k).ok_or_else(|| format!("bad button id {k}"))?;
            p.normal.encode()?;
            if let Some(h) = &p.hypershift {
                h.encode()?;
            }
        }
        for (k, steps) in &self.macros {
            k.parse::<u16>().map_err(|_| format!("bad macro id {k}"))?;
            crate::macros::encode(steps)?;
        }
        Ok(())
    }
}

pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// File-name-safe id from a name: "My Game!" -> "my-game".
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            s.extend(c.to_lowercase());
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-').to_string();
    if s.is_empty() {
        "profile".into()
    } else {
        s.chars().take(40).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_slug() {
        let p = Profile { links: vec!["C:\\Games\\Witcher\\witcher3.exe".into(), "code".into()], ..Default::default() };
        assert!(p.linked_to("D:\\Other\\WITCHER3.EXE"));
        assert!(p.linked_to("/usr/bin/code"));
        assert!(!p.linked_to("chrome.exe"));
        assert_eq!(slug("WIN-R7CFI1KMABK-Default"), "win-r7cfi1kmabk-default");
        assert_eq!(slug("Мой профиль!"), "мой-профиль");
        assert_eq!(slug("!!"), "profile");
    }

    #[test]
    fn reset_and_macros() {
        let mut p = Profile::default();
        p.set(0x40, false, Action { kind: "macro".into(), macro_id: 7, ..Action::keyboard(0, 0x04) });
        p.set(0x7e, false, Action::keyboard(0, 0x05));
        assert_eq!(p.used_macros(), vec![7]);
        p.reset_to(crate::devices::naga_pro::default_action);
        assert_eq!(p.button(0x40).unwrap().normal.describe(), "Key: 1");
        assert_eq!(p.button(0x7e).unwrap().normal.describe(), "Key: B");
        assert!(p.used_macros().is_empty());
        assert!(p.validate().is_ok());
        let bad = Profile { format: "x".into(), ..Default::default() };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn hypershift_layer() {
        let mut p = Profile::default();
        p.set(0x40, false, Action::keyboard(0, 0x04));
        p.set(0x41, false, Action::kind("hypershift"));
        p.set(0x41, true, Action::kind("hypershift"));
        p.set(0x42, false, Action::keyboard(0, 0x05));
        p.set(0x42, true, Action::keyboard(0, 0x05));
        assert_eq!(p.hypershift_buttons(), vec![0x41]);
        let copy = p.layer_plan("copy").unwrap();
        assert_eq!(copy, vec![(0x40, Action::keyboard(0, 0x04))]);
        let clear = p.layer_plan("clear").unwrap();
        assert_eq!(clear.iter().map(|(id, a)| (*id, a.kind.as_str())).collect::<Vec<_>>(), [(0x40, "disabled"), (0x42, "disabled")]);
    }
}
