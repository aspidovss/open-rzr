//! Commands: OpenRazer compatibility.

use crate::*;

#[tauri::command]
pub(crate) async fn openrazer_status(st: State<'_, AppState>) -> R<compat::Status> {
    let mut s = compat::status();
    // the window shows the chosen mode, "" = auto
    s.mode = lock(&st.store).openrazer_mode.clone();
    if s.mode.is_empty() {
        s.mode = "auto".into();
    }
    Ok(s)
}

/// Change the mode; the mouse is disconnected so the next connect uses the new transport.
#[tauri::command]
pub(crate) async fn set_openrazer_mode(st: State<'_, AppState>, mode: String) -> R<()> {
    let m = compat::Mode::parse(&mode).ok_or("[invalid] mode: auto | on | off")?;
    log_info!("ui", "OpenRazer compatibility: {}", m.code());
    compat::set_mode(m);
    {
        let mut s = lock(&st.store);
        s.openrazer_mode = m.code().into();
        s.save();
    }
    *lock(&st.mouse) = None;
    Ok(())
}

/// Polychromatic is a separate program (GPL-3.0): it is only started, never loaded into open-rzr.
#[tauri::command]
pub(crate) async fn open_polychromatic() -> R<()> {
    log_info!("ui", "open Polychromatic");
    compat::open_polychromatic().map_err(|e| err(&e))
}

#[tauri::command]
pub(crate) async fn stop_openrazer_daemon(st: State<'_, AppState>) -> R<()> {
    log_info!("ui", "stop OpenRazer daemon");
    compat::stop_daemon().map_err(|e| err(&e))?;
    *lock(&st.mouse) = None;
    Ok(())
}
