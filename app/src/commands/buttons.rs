//! Commands: buttons.

use crate::*;

#[tauri::command]
pub(crate) async fn side_plate(st: State<'_, AppState>) -> R<Option<u8>> {
    with_mouse(&st, "side plate", |m| m.side_plate())
}

#[tauri::command]
pub(crate) async fn set_plate_choice(st: State<'_, AppState>, choice: String) -> R<()> {
    let serial = lock(&st.serial).clone();
    log_info!("ui", "plate filter: {choice}");
    let mut s = lock(&st.store);
    s.plate_choice.insert(serial, choice);
    s.save();
    Ok(())
}

/// `plate`: 12/6/2 = list only that plate's buttons, 0 = no filter.
#[tauri::command]
pub(crate) async fn scan_buttons(st: State<'_, AppState>, profile: u8, hypershift: bool, all: bool, plate: u8) -> R<Vec<ButtonInfo>> {
    log_info!("ui", "scan buttons: profile {profile}, hypershift {hypershift}, plate {plate}");
    let mut list = with_mouse(&st, "scan buttons", |m| m.scan_buttons(if profile == 0 { 1 } else { profile }, hypershift, all))?;
    if profile == 0 {
        // direct/temporary layer: re-read values for profile 0
        with_mouse(&st, "temporary layer", |m| {
            for b in list.iter_mut() {
                if let Ok(a) = m.get_button(b.id, 0, hypershift) {
                    b.action = a;
                }
            }
            Ok(())
        })?;
    }
    if plate > 0 {
        list.retain(|b| devices::button_on_plate(b.id, plate));
    }
    Ok(list)
}

#[tauri::command]
pub(crate) async fn set_button(st: State<'_, AppState>, id: u8, action: Action, profile: u8, hypershift: bool) -> R<Action> {
    log_info!("ui", "bind 0x{id:02x} (profile {profile}{}): {}", if hypershift { ", Hypershift" } else { "" }, action.describe());
    with_mouse(&st, "bind button", |m| {
        m.set_button(id, &action, profile, hypershift)?;
        m.get_button(id, profile, hypershift)
    })
}

/// Names the user gave to buttons of the connected mouse: "0x40" style id (lowercase hex, no prefix) -> name.
#[tauri::command]
pub(crate) async fn button_names(st: State<'_, AppState>) -> R<HashMap<String, String>> {
    let serial = lock(&st.serial).clone();
    Ok(lock(&st.store).button_names.get(&serial).cloned().unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn rename_button(st: State<'_, AppState>, id: u8, name: String) -> R<()> {
    let serial = lock(&st.serial).clone();
    log_info!("ui", "rename 0x{id:02x} -> {name}");
    let mut s = lock(&st.store);
    s.button_names.entry(serial).or_default().insert(format!("{id:02x}"), name);
    s.save();
    Ok(())
}

#[tauri::command]
pub(crate) async fn copy_profile(st: State<'_, AppState>, from: u8, to: u8) -> R<usize> {
    log_info!("ui", "copy profile {from} -> {to}");
    with_mouse(&st, "copy profile", |m| m.copy_profile(from, to))
}

#[tauri::command]
pub(crate) async fn describe_action(action: Action) -> R<String> {
    Ok(action.describe())
}
