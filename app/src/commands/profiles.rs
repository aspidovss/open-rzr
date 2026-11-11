//! Commands: profiles (library, on-board slots, linking).

use crate::*;

#[derive(Serialize)]
pub(crate) struct ProfileMeta {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) links: Vec<String>,
    /// on-board slots (1..5) holding this profile on the connected mouse
    pub(crate) slots: Vec<u8>,
    /// None: in no slot; Some(true): every slot holds exactly this version; Some(false): changed since
    pub(crate) written: Option<bool>,
}

#[derive(Serialize)]
pub(crate) struct ProfilesInfo {
    pub(crate) list: Vec<ProfileMeta>,
    pub(crate) current: String,
    /// slot 1..5 -> profile id
    pub(crate) slots: Vec<Option<String>>,
    /// (total, free, used) bytes of the on-board memory, if the mouse reports it
    pub(crate) flash: Option<(u32, u32, u32)>,
    pub(crate) autoswitch: store::AutoSwitch,
    /// the program in front can be followed on this system
    pub(crate) linking: bool,
    /// changes go straight into the slot (Settings)
    pub(crate) autowrite: bool,
}

/// What a slot must hold for a profile to count as written: bindings, DPI stages, macros.
pub(crate) fn fingerprint(p: &Profile) -> String {
    use std::hash::{Hash, Hasher};
    let text = serde_json::to_string(&(&p.buttons, &p.dpi_stages, &p.macros)).unwrap_or_default();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Remember that `slot` now holds this version of `p`.
pub(crate) fn mark_written(st: &AppState, slot: u8, p: Option<&Profile>) {
    if !(1..=5).contains(&slot) {
        return;
    }
    let serial = serial_of(st);
    let mut s = lock(&st.store);
    let v = s.written.entry(serial).or_default();
    v.resize(5, None);
    v[slot as usize - 1] = p.map(fingerprint);
    s.save();
}

pub(crate) fn serial_of(st: &AppState) -> String {
    lock(&st.serial).clone()
}

pub(crate) fn slots_of(st: &AppState, serial: &str) -> Vec<Option<String>> {
    let mut v = lock(&st.store).slots.get(serial).cloned().unwrap_or_default();
    v.resize(5, None);
    v
}

/// Where edits of profile `id` go: its on-board slot, or the temporary layer.
/// Where changes of profile `id` go: its slot when "write changes at once" is on, else the
/// temporary layer (applies at once, kept in RAM) until "Write to mouse".
pub(crate) fn target_slot(st: &AppState, serial: &str, id: &str) -> u8 {
    if !lock(&st.store).autowrite {
        return TEMPORARY;
    }
    slots_of(st, serial).iter().position(|s| s.as_deref() == Some(id)).map(|i| i as u8 + 1).unwrap_or(TEMPORARY)
}

pub(crate) fn profiles_info(st: &AppState) -> ProfilesInfo {
    let serial = serial_of(st);
    let slots = slots_of(st, &serial);
    let profiles = library::list();
    let written = {
        let mut s = lock(&st.store);
        let mut w = s.written.get(&serial).cloned().unwrap_or_default();
        w.resize(5, None);
        // slots filled by earlier versions (which wrote at once) count as written
        let mut changed = false;
        for (i, id) in slots.iter().enumerate() {
            if let (Some(id), None) = (id, &w[i]) {
                if let Some(p) = profiles.iter().find(|p| &p.id == id) {
                    w[i] = Some(fingerprint(p));
                    changed = true;
                }
            }
        }
        if changed {
            s.written.insert(serial.clone(), w.clone());
            s.save();
        }
        w
    };
    let list = profiles
        .into_iter()
        .map(|p| {
            let mine: Vec<u8> =
                slots.iter().enumerate().filter(|(_, s)| s.as_deref() == Some(p.id.as_str())).map(|(i, _)| i as u8 + 1).collect();
            let fp = fingerprint(&p);
            let written = (!mine.is_empty()).then(|| mine.iter().all(|n| written[*n as usize - 1].as_deref() == Some(fp.as_str())));
            ProfileMeta { slots: mine, written, id: p.id, name: p.name, links: p.links }
        })
        .collect();
    let flash = lock(&st.mouse).as_mut().filter(|m| m.has(feat::MACROS)).and_then(|m| m.flash_usage().ok());
    let s = lock(&st.store);
    ProfilesInfo {
        list,
        current: s.current_profile.get(&serial).cloned().unwrap_or_default(),
        slots,
        flash,
        autoswitch: s.autoswitch.clone(),
        linking: foreground::supported(),
        autowrite: s.autowrite,
    }
}

/// Profiles and on-board slots for the connected mouse. On the first connection the mouse's
/// slots are read into the library (like Synapse does), named "<COMPUTER>-Default", "Profile 2", ...
#[tauri::command]
pub(crate) async fn profiles(st: State<'_, AppState>) -> R<ProfilesInfo> {
    let serial = serial_of(&st);
    let known = lock(&st.store).slots.contains_key(&serial);
    let has_buttons = lock(&st.mouse).as_ref().map(|m| m.has(feat::BUTTONS)).unwrap_or(false);
    if !known && has_buttons {
        log_info!("ui", "first connection of {serial}: reading on-board profiles into the library");
        let host = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "PC".into());
        let slots = with_mouse(&st, "read on-board profiles", |m| {
            let mut ids = Vec::new();
            for slot in m.list_profiles().into_iter().filter(|s| (1..=5).contains(s)) {
                let name = if slot == 1 { format!("{host}-Default") } else { format!("Profile {slot}") };
                let p = m.read_profile(slot, &name)?;
                ids.push((slot, p));
            }
            Ok(ids)
        })?;
        let mut map: Vec<Option<String>> = vec![None; 5];
        let mut first = None;
        for (slot, p) in slots {
            let p = library::add(p)?;
            first.get_or_insert(p.id.clone());
            map[slot as usize - 1] = Some(p.id);
        }
        let mut s = lock(&st.store);
        s.slots.insert(serial.clone(), map);
        if let Some(f) = first {
            s.current_profile.insert(serial.clone(), f);
        }
        s.save();
    }
    Ok(profiles_info(&st))
}

#[tauri::command]
pub(crate) async fn profile_get(id: String) -> R<Profile> {
    library::load(&id)
}

/// Make `id` the selected profile. A profile that is not in a slot is written to the temporary layer.
#[tauri::command]
pub(crate) async fn profile_select(st: State<'_, AppState>, id: String) -> R<Profile> {
    let _op = lock(&st.profile_op);
    let p = library::load(&id)?;
    let serial = serial_of(&st);
    log_info!("ui", "profile selected: \"{}\"", p.name);
    {
        let mut s = lock(&st.store);
        s.current_profile.insert(serial.clone(), id.clone());
        s.save();
    }
    if target_slot(&st, &serial, &id) == TEMPORARY {
        write_temporary(&st, &p)?;
    }
    Ok(p)
}

pub(crate) fn write_temporary(st: &AppState, p: &Profile) -> R<()> {
    let prev = lock(&st.temp_layer).clone();
    let r = with_mouse(st, "write profile to the temporary layer", |m| {
        m.write_profile(p, TEMPORARY, prev.as_ref())?;
        m.apply_profile_globals(&globals_of(p))
    });
    // after a failure part of it may be in the mouse: next time write it whole
    *lock(&st.temp_layer) = if r.is_ok() { Some(p.clone()) } else { None };
    r
}

/// Before one change of profile `p` goes to the temporary layer: the layer must hold `p`.
/// (The automatic switch may have put another profile there meanwhile; the profile being
/// edited wins, so the mouse never holds a mix of two profiles.)
pub(crate) fn ensure_temporary(st: &AppState, p: &Profile, slot: u8) -> R<()> {
    if slot != TEMPORARY {
        return Ok(());
    }
    let holds = lock(&st.temp_layer).as_ref().map(|t| t.id == p.id);
    if holds == Some(false) {
        log_info!("app", "temporary layer holds another profile: writing \"{}\" back first", p.name);
        write_temporary(st, p)?;
    }
    Ok(())
}

/// Change one binding of a profile: saved in the file and written to the mouse (its slot or the temporary layer).
#[tauri::command]
pub(crate) async fn profile_set_button(st: State<'_, AppState>, id: String, button: u8, hypershift: bool, action: Action) -> R<Action> {
    let _op = lock(&st.profile_op);
    let mut p = library::load(&id)?;
    let slot = target_slot(&st, &serial_of(&st), &id);
    log_info!("ui", "\"{}\" bind 0x{button:02x}{} -> {} (slot {slot})", p.name, if hypershift { " Hypershift" } else { "" }, action.describe());
    let was_modifier = p.button(button).map(|b| b.normal.kind == "hypershift").unwrap_or(false);
    if hypershift && was_modifier {
        return Err("[invalid] this button holds Hypershift; its Hypershift binding cannot change".into());
    }
    ensure_temporary(&st, &p, slot)?;
    let back = with_mouse(&st, "bind button", |m| {
        let back = m.set_profile_button(&mut p, slot, button, hypershift, &action)?;
        if !hypershift && action.kind == "hypershift" {
            // the modifier keeps Hypershift in its own layer too: releasing it ends the layer
            m.set_profile_button(&mut p, slot, button, true, &action)?;
        } else if !hypershift && was_modifier {
            m.set_profile_button(&mut p, slot, button, true, &Action::kind("disabled"))?;
        }
        Ok(back)
    })?;
    if action.kind == "macro" {
        // keep the macro with the profile so it travels with an export
        if let Ok(steps) = with_mouse(&st, "read macro", |m| m.read_macro(action.macro_id)) {
            p.macros.insert(action.macro_id.to_string(), steps);
        }
    }
    library::save(&p)?;
    if slot == TEMPORARY {
        *lock(&st.temp_layer) = Some(p);
    } else {
        mark_written(&st, slot, Some(&p));
    }
    Ok(back)
}

/// Hypershift layer: "copy" (standard → Hypershift) or "clear".
#[tauri::command]
pub(crate) async fn profile_layer(st: State<'_, AppState>, id: String, op: String) -> R<usize> {
    let _op = lock(&st.profile_op);
    let mut p = library::load(&id)?;
    let slot = target_slot(&st, &serial_of(&st), &id);
    log_info!("ui", "\"{}\" Hypershift layer: {op} (slot {slot})", p.name);
    ensure_temporary(&st, &p, slot)?;
    let n = with_mouse(&st, "Hypershift layer", |m| m.apply_layer(&mut p, slot, &op))?;
    library::save(&p)?;
    if slot == TEMPORARY {
        *lock(&st.temp_layer) = Some(p);
    } else {
        mark_written(&st, slot, Some(&p));
    }
    Ok(n)
}

#[tauri::command]
pub(crate) async fn profile_set_dpi(st: State<'_, AppState>, id: String, stages: Vec<(u16, u16)>, active: u8) -> R<(u16, u16)> {
    let _op = lock(&st.profile_op);
    let mut p = library::load(&id)?;
    let slot = target_slot(&st, &serial_of(&st), &id);
    log_info!("ui", "\"{}\" DPI stages {stages:?}, active {active} (slot {slot})", p.name);
    ensure_temporary(&st, &p, slot)?;
    let dpi = with_mouse(&st, "DPI", |m| {
        if slot != TEMPORARY {
            m.set_dpi_stages(&stages, active, slot)?;
        }
        if let Some(&(x, y)) = stages.get(active.saturating_sub(1) as usize) {
            m.set_dpi(x, y)?;
        }
        m.get_dpi()
    })?;
    p.dpi_stages = Some(DpiStages { active, stages });
    library::save(&p)?;
    if slot != TEMPORARY {
        mark_written(&st, slot, Some(&p));
    }
    Ok(dpi)
}

/// "Write to mouse": the profile into every slot that holds it.
#[tauri::command]
pub(crate) async fn profile_write(st: State<'_, AppState>, id: String) -> R<ProfilesInfo> {
    let _op = lock(&st.profile_op);
    let p = library::load(&id)?;
    let serial = serial_of(&st);
    let slots: Vec<u8> =
        slots_of(&st, &serial).iter().enumerate().filter(|(_, s)| s.as_deref() == Some(id.as_str())).map(|(i, _)| i as u8 + 1).collect();
    if slots.is_empty() {
        return Err("[invalid] the profile is in no slot: choose one".into());
    }
    for slot in slots {
        log_info!("ui", "write \"{}\" -> slot {slot}", p.name);
        let n = match with_mouse(&st, "write profile to slot", |m| m.write_profile(&p, slot, None)) {
            Ok(n) => n,
            Err(e) => {
                // part of the profile may be in the slot: show it as not written
                log_warn!("ui", "slot {slot}: write stopped half-way, marked as not written");
                mark_written(&st, slot, None);
                return Err(e);
            }
        };
        log_info!("ui", "slot {slot}: {n} records");
        mark_written(&st, slot, Some(&p));
    }
    Ok(profiles_info(&st))
}

#[tauri::command]
pub(crate) async fn set_autowrite(st: State<'_, AppState>, on: bool) -> R<ProfilesInfo> {
    log_info!("ui", "write changes straight into slots: {on}");
    {
        let mut s = lock(&st.store);
        s.autowrite = on;
        s.save();
    }
    Ok(profiles_info(&st))
}

/// Synapse "Add": `from` = "default" (factory bindings) | "current" (copy of the selected one) | "slot:N" (read from the mouse).
#[tauri::command]
pub(crate) async fn profile_create(st: State<'_, AppState>, name: String, from: String) -> R<String> {
    let _op = lock(&st.profile_op);
    log_info!("ui", "add profile \"{name}\" from {from}");
    let mut p = if let Some(n) = from.strip_prefix("slot:") {
        let slot: u8 = n.parse().map_err(|_| "[invalid] slot")?;
        with_mouse(&st, "read profile", |m| m.read_profile(slot, &name))?
    } else if from == "current" {
        let cur = lock(&st.store).current_profile.get(&serial_of(&st)).cloned().unwrap_or_default();
        library::load(&cur)?
    } else {
        with_mouse(&st, "factory profile", |m| m.factory_profile(&name))?
    };
    p.name = name;
    p.links.clear();
    Ok(library::add(p)?.id)
}

#[tauri::command]
pub(crate) async fn profile_rename(id: String, name: String) -> R<()> {
    let mut p = library::load(&id)?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("[invalid] empty name".into());
    }
    log_info!("ui", "rename profile \"{}\" -> \"{name}\"", p.name);
    p.name = name;
    library::save(&p)
}

#[tauri::command]
pub(crate) async fn profile_duplicate(id: String) -> R<String> {
    let mut p = library::load(&id)?;
    log_info!("ui", "duplicate profile \"{}\"", p.name);
    p.name = format!("{} (copy)", p.name);
    p.links.clear();
    Ok(library::add(p)?.id)
}

#[tauri::command]
pub(crate) async fn profile_delete(st: State<'_, AppState>, id: String) -> R<()> {
    let _op = lock(&st.profile_op);
    if library::list().len() <= 1 {
        return Err("[invalid] the last profile cannot be deleted".into());
    }
    log_info!("ui", "delete profile {id}");
    library::delete(&id)?;
    let mut s = lock(&st.store);
    for v in s.slots.values_mut() {
        for slot in v.iter_mut() {
            if slot.as_deref() == Some(id.as_str()) {
                *slot = None;
            }
        }
    }
    s.current_profile.retain(|_, v| *v != id);
    if s.autoswitch.fallback == id {
        s.autoswitch.fallback.clear();
    }
    s.save();
    Ok(())
}

/// Synapse "Reset": factory bindings for every button, written to the mouse.
#[tauri::command]
pub(crate) async fn profile_reset(st: State<'_, AppState>, id: String) -> R<Profile> {
    let _op = lock(&st.profile_op);
    let mut p = library::load(&id)?;
    let slot = target_slot(&st, &serial_of(&st), &id);
    log_info!("ui", "reset profile \"{}\" (slot {slot})", p.name);
    with_mouse(&st, "reset profile", |m| {
        p.reset_to(m.spec.defaults);
        m.write_profile(&p, slot, None).map(|_| ())
    })?;
    library::save(&p)?;
    if slot != TEMPORARY {
        mark_written(&st, slot, Some(&p));
    }
    Ok(p)
}

/// "Undo" after a reset or a layer change: put the earlier version of the profile back
/// (only what differs is written to the mouse).
#[tauri::command]
pub(crate) async fn profile_restore(st: State<'_, AppState>, profile: Profile) -> R<Profile> {
    let _op = lock(&st.profile_op);
    let cur = library::load(&profile.id)?;
    let mut p = profile;
    p.id = cur.id.clone();
    let slot = target_slot(&st, &serial_of(&st), &p.id);
    log_info!("ui", "undo: restore profile \"{}\" (slot {slot})", p.name);
    ensure_temporary(&st, &cur, slot)?;
    let n = with_mouse(&st, "undo", |m| m.write_profile(&p, slot, Some(&cur)))?;
    log_info!("ui", "undo: {n} records written back");
    library::save(&p)?;
    if slot == TEMPORARY {
        *lock(&st.temp_layer) = Some(p.clone());
    } else {
        mark_written(&st, slot, Some(&p));
    }
    Ok(p)
}

/// Read the profile back from its on-board slot (when it was changed elsewhere).
#[tauri::command]
pub(crate) async fn profile_reload(st: State<'_, AppState>, id: String) -> R<Profile> {
    let _op = lock(&st.profile_op);
    let old = library::load(&id)?;
    // its on-board slot even when changes go to the temporary layer first
    let slot = slots_of(&st, &serial_of(&st)).iter().position(|s| s.as_deref() == Some(id.as_str())).map(|i| i as u8 + 1).unwrap_or(TEMPORARY);
    let mut p = with_mouse(&st, "read profile", |m| m.read_profile(if slot == TEMPORARY { 0 } else { slot }, &old.name))?;
    p.id = old.id;
    p.links = old.links;
    p.lighting = old.lighting;
    if p.dpi_stages.is_none() {
        p.dpi_stages = old.dpi_stages;
    }
    library::save(&p)?;
    if slot != TEMPORARY {
        mark_written(&st, slot, Some(&p));
    }
    Ok(p)
}

#[tauri::command]
pub(crate) async fn profile_export(id: String) -> R<String> {
    let p = library::load(&id)?;
    library::export(&p).map(|f| f.display().to_string())
}

/// Import the text of an exported profile (the window reads the file).
#[tauri::command]
pub(crate) async fn profile_import(text: String) -> R<String> {
    log_info!("ui", "import profile ({} bytes)", text.len());
    Ok(library::import(&text)?.id)
}

#[tauri::command]
pub(crate) async fn open_folder_of(path: String) -> R<()> {
    let p = std::path::PathBuf::from(&path);
    open_path(p.parent().unwrap_or(&p))
}

/// Put profile `id` into on-board slot 1..5 (None = forget the assignment; the mouse keeps its content).
#[tauri::command]
pub(crate) async fn slot_assign(st: State<'_, AppState>, slot: u8, id: Option<String>) -> R<ProfilesInfo> {
    let _op = lock(&st.profile_op);
    if !(1..=5).contains(&slot) {
        return Err("[invalid] slot 1..5".into());
    }
    let serial = serial_of(&st);
    if let Some(id) = &id {
        let p = library::load(id)?;
        log_info!("ui", "slot {slot} <- \"{}\"", p.name);
        with_mouse(&st, "write profile to slot", |m| m.write_profile(&p, slot, None).map(|_| ()))?;
        mark_written(&st, slot, Some(&p));
    } else {
        mark_written(&st, slot, None);
    }
    {
        let mut s = lock(&st.store);
        let v = s.slots.entry(serial).or_default();
        v.resize(5, None);
        v[slot as usize - 1] = id;
        s.save();
    }
    Ok(profiles_info(&st))
}

/// First id of the service macros open-rzr writes itself (turbo for media keys).
pub(crate) const SERVICE_MACRO: u16 = 0xF000;

/// Turbo for a media key: the mouse has no turbo for them, so a small macro "press, release,
/// pause" is played while the button is held. Reuses an identical one already in the mouse.
#[tauri::command]
pub(crate) async fn turbo_media(st: State<'_, AppState>, code: u16, interval: u16) -> R<u16> {
    let interval = interval.clamp(20, 5000) as i64;
    let steps = vec![
        MacroStep::new("consumer", code as i64),
        MacroStep::new("delay", 10),
        MacroStep::new("consumer", 0),
        MacroStep::new("delay", (interval - 10).max(10)),
    ];
    let id = with_mouse(&st, "turbo macro", |m| {
        let ids = m.macro_ids()?;
        for id in ids.iter().copied().filter(|i| *i >= SERVICE_MACRO) {
            if m.read_macro(id).map(|s| s == steps).unwrap_or(false) {
                return Ok(id);
            }
        }
        let id = (SERVICE_MACRO..u16::MAX)
            .find(|i| !ids.contains(i))
            .ok_or_else(|| openrzr::Error::Invalid("no free service macro id".into()))?;
        m.write_macro(id, &steps)?;
        Ok(id)
    })?;
    log_info!("ui", "turbo media 0x{code:04x} / {interval} ms -> service macro #{id}");
    Ok(id)
}

#[tauri::command]
pub(crate) async fn profile_links(id: String, links: Vec<String>) -> R<()> {
    let mut p = library::load(&id)?;
    log_info!("ui", "\"{}\" linked programs: {links:?}", p.name);
    p.links = links;
    library::save(&p)
}

#[tauri::command]
pub(crate) async fn running_programs() -> R<Vec<foreground::Program>> {
    Ok(foreground::running_programs())
}

#[tauri::command]
pub(crate) async fn set_autoswitch(st: State<'_, AppState>, enabled: bool, fallback: String, target: u8) -> R<()> {
    log_info!("ui", "auto switch: {enabled}, fallback \"{fallback}\", target {target}");
    let mut s = lock(&st.store);
    s.autoswitch = store::AutoSwitch { enabled, fallback, target: target.min(5) };
    s.save();
    Ok(())
}

#[derive(Serialize, Clone)]
pub(crate) struct Switched {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) program: String,
}

/// Follow the program in front and switch to its linked profile (runs while open-rzr runs).
pub(crate) fn autoswitch_loop(app: AppHandle) {
    let mut last_exe = String::new();
    let mut last_id = String::new();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let st = app.state::<AppState>();
        let cfg = lock(&st.store).autoswitch.clone();
        if !cfg.enabled || !foreground::supported() || lock(&st.mouse).is_none() {
            last_id.clear();
            continue;
        }
        let Some(exe) = foreground::foreground_exe() else { continue };
        let name = openrzr::profile::file_name(&exe).to_lowercase();
        if exe == last_exe || name.starts_with("open-rzr") {
            continue;
        }
        last_exe = exe.clone();
        let all = library::list();
        let serial = serial_of(&st);
        let current = lock(&st.store).current_profile.get(&serial).cloned().unwrap_or_default();
        let fallback = if cfg.fallback.is_empty() { current } else { cfg.fallback.clone() };
        let target = all.iter().find(|p| p.linked_to(&exe)).or_else(|| all.iter().find(|p| p.id == fallback));
        let Some(p) = target else { continue };
        if p.id == last_id {
            continue;
        }
        log_info!("app", "auto switch: {name} -> \"{}\"", p.name);
        let _op = lock(&st.profile_op);
        let r = if cfg.target == TEMPORARY {
            write_temporary(&st, p)
        } else {
            with_mouse(&st, "auto switch", |m| {
                m.write_profile(p, cfg.target, None)?;
                m.apply_profile_globals(&globals_of(p))
            })
        };
        match r {
            Ok(()) => {
                last_id = p.id.clone();
                let _ = app.emit("profile-switched", Switched { id: p.id.clone(), name: p.name.clone(), program: name });
            }
            Err(e) => {
                log_warn!("app", "auto switch failed: {e}");
                if cfg.target != TEMPORARY {
                    mark_written(&st, cfg.target, None);
                }
            }
        }
    }
}
