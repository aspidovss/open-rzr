//! Commands: settings.

use crate::*;

#[derive(Serialize)]
pub(crate) struct Settings {
    /// "" until the user confirmed a language on first run
    pub(crate) language: String,
    pub(crate) autostart: bool,
    pub(crate) start_minimized: bool,
    pub(crate) close_to_tray: bool,
    pub(crate) log_level: String,
    /// "synapse" | "classic"
    pub(crate) ui_style: String,
}

#[tauri::command]
pub(crate) async fn get_settings(st: State<'_, AppState>) -> R<Settings> {
    let a = autostart::get();
    let s = lock(&st.store);
    Ok(Settings {
        language: s.language.clone(),
        autostart: a.enabled,
        start_minimized: a.minimized,
        close_to_tray: s.close_to_tray(),
        log_level: logs::level().as_str().into(),
        ui_style: if s.ui_style.is_empty() { "synapse".into() } else { s.ui_style.clone() },
    })
}

#[tauri::command]
pub(crate) async fn set_ui_style(st: State<'_, AppState>, style: String) -> R<()> {
    if !["synapse", "classic"].contains(&style.as_str()) {
        return Err("[invalid] style: synapse | classic".into());
    }
    log_info!("ui", "window style: {style}");
    let mut s = lock(&st.store);
    s.ui_style = style;
    s.save();
    Ok(())
}

#[tauri::command]
pub(crate) async fn set_language(st: State<'_, AppState>, language: String) -> R<()> {
    if !["en", "ru"].contains(&language.as_str()) {
        return Err("[invalid] language: en | ru".into());
    }
    log_info!("ui", "language: {language}");
    {
        let mut s = lock(&st.store);
        s.language = language.clone();
        s.save();
    }
    if let Some(t) = lock(&st.tray).as_ref() {
        let (show, quit) = tray_labels(&language);
        let _ = t.show.set_text(show);
        let _ = t.quit.set_text(quit);
        if let Some(pm) = &t.poly {
            let _ = pm.set_text(poly_label(&language));
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn set_autostart(enabled: bool, minimized: bool) -> R<autostart::State> {
    log_info!("ui", "autostart: {enabled}, minimized: {minimized}");
    autostart::set(enabled, minimized).map_err(|e| {
        log_error!("app", "autostart: {e}");
        format!("[io] {e}")
    })?;
    Ok(autostart::get())
}

#[tauri::command]
pub(crate) async fn set_close_to_tray(st: State<'_, AppState>, enabled: bool) -> R<()> {
    log_info!("ui", "close to tray: {enabled}");
    let mut s = lock(&st.store);
    s.close_to_tray = Some(enabled);
    s.save();
    Ok(())
}

#[tauri::command]
pub(crate) async fn default_action(st: State<'_, AppState>, id: u8) -> R<Option<Action>> {
    let g = lock(&st.mouse);
    Ok(match g.as_ref() {
        Some(m) => m.default_action(id),
        None => devices::default_action(id),
    })
}
