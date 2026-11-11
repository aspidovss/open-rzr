//! Commands: tables for the UI.

use crate::*;

#[derive(Serialize)]
pub(crate) struct Tables {
    pub(crate) kinds: Vec<(&'static str, &'static str)>,
    pub(crate) dpi_ops: Vec<(u8, &'static str)>,
    pub(crate) profile_ops: Vec<(u8, &'static str)>,
    pub(crate) system_ops: Vec<(u8, &'static str)>,
    pub(crate) macro_modes: Vec<(&'static str, &'static str)>,
    pub(crate) mouse_fn: Vec<(u8, &'static str)>,
    pub(crate) consumer: Vec<(u16, &'static str)>,
    pub(crate) keys: Vec<(u8, String)>,
    pub(crate) zones: Vec<(&'static str, &'static str)>,
    pub(crate) effects: Vec<(&'static str, &'static str)>,
    pub(crate) zone_effects: HashMap<&'static str, Vec<&'static str>>,
}

#[tauri::command]
pub(crate) async fn tables() -> R<Tables> {
    use openrzr::bindings as b;
    Ok(Tables {
        kinds: b::KINDS.to_vec(),
        dpi_ops: b::DPI_OPS.to_vec(),
        profile_ops: b::PROFILE_OPS.to_vec(),
        system_ops: b::SYSTEM_OPS.to_vec(),
        macro_modes: b::MACRO_MODES.to_vec(),
        mouse_fn: hidkeys::MOUSE_FN.to_vec(),
        consumer: hidkeys::CONSUMER.to_vec(),
        keys: hidkeys::all_keys(),
        zones: lighting::ZONES.to_vec(),
        effects: lighting::EFFECTS.to_vec(),
        zone_effects: lighting::ZONES.iter().map(|(z, _)| (*z, lighting::zone_effects(z).to_vec())).collect(),
    })
}
