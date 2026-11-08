//! Local settings: one JSON file in the config dir. No network, no accounts.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use openrzr::logs;
use openrzr::macro_doc::{Item, SoftAction};
use openrzr::{MacroStep, ZoneState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedMacro {
    pub id: u16,
    /// compiled on-board steps (what was last written)
    pub steps: Vec<MacroStep>,
    /// editor steps (hardware part); empty in libraries of 0.5.7 and older: made from `steps`
    #[serde(default)]
    pub items: Vec<Item>,
    /// program actions, performed by open-rzr when the mouse presses `trigger`
    #[serde(default)]
    pub software: Vec<SoftAction>,
    /// F13–F24 HID code the macro ends with, when it has program actions
    #[serde(default)]
    pub trigger: Option<u8>,
}

impl SavedMacro {
    pub fn editor_items(&self) -> Vec<Item> {
        if self.items.is_empty() {
            self.steps.iter().map(Item::from_step).collect()
        } else {
            self.items.clone()
        }
    }
}

/// Switching profiles by the program in front.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoSwitch {
    pub enabled: bool,
    /// profile used when no linked program is in front ("" = keep the selected profile)
    pub fallback: String,
    /// where linked profiles are written: 0 = temporary layer (no flash wear), 1..5 = on-board slot
    pub target: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    /// serial -> button id (hex) -> custom name
    pub button_names: HashMap<String, HashMap<String, String>>,
    /// name -> macro
    pub macros: BTreeMap<String, SavedMacro>,
    /// serial -> zone -> last state set from the app (the mouse can't report its effect)
    pub lighting: HashMap<String, HashMap<String, ZoneState>>,
    pub log_level: String,
    /// serial -> side plate choice: "auto" | "12" | "6" | "2" | "all"
    pub plate_choice: HashMap<String, String>,
    /// UI language: "" = not chosen yet (first run asks), "en", "ru"
    pub language: String,
    /// closing the window hides it to the tray instead of quitting
    pub close_to_tray: Option<bool>,
    /// OpenRazer compatibility: "" / "auto", "on", "off"
    pub openrazer_mode: String,
    /// Windows: Razer services open-rzr stopped, with their original start mode, to start them again
    pub razer_saved: Vec<(String, String)>,
    /// serial -> selected profile id
    pub current_profile: HashMap<String, String>,
    /// serial -> profile id in on-board slots 1..5 (index 0 = slot 1)
    pub slots: HashMap<String, Vec<Option<String>>>,
    pub autoswitch: AutoSwitch,
    /// window style: "" / "synapse" = like the original configurator, "classic" = sidebar
    pub ui_style: String,
}

impl Store {
    pub fn close_to_tray(&self) -> bool {
        self.close_to_tray.unwrap_or(true)
    }
}

pub fn path() -> PathBuf {
    logs::config_dir().join("settings.json")
}

impl Store {
    pub fn load() -> Store {
        match std::fs::read_to_string(path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                openrzr::log_warn!("app", "settings.json is damaged ({e}), starting with defaults");
                Store::default()
            }),
            Err(_) => Store::default(),
        }
    }

    pub fn save(&self) {
        let p = path();
        let res = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(p.parent().unwrap())?;
            let tmp = p.with_extension("tmp");
            std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap_or_default())?;
            std::fs::rename(tmp, &p)
        })();
        if let Err(e) = res {
            openrzr::log_error!("app", "cannot save settings: {e}");
        }
    }
}
