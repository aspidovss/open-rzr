//! Tauri commands of the window, one module per page / subject. They see everything of the crate
//! root (`use crate::*`), and the root sees their items (`use commands::x::*`).

pub(crate) mod device;
pub(crate) mod settings;
pub(crate) mod openrazer_cmds;
pub(crate) mod razer;
pub(crate) mod performance;
pub(crate) mod buttons;
pub(crate) mod macro_cmds;
pub(crate) mod lighting_cmds;
pub(crate) mod profiles;
pub(crate) mod backup;
pub(crate) mod diagnostics;
pub(crate) mod tables;
