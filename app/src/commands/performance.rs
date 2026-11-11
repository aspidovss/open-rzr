//! Commands: performance / power.

use crate::*;

#[tauri::command]
pub(crate) async fn set_dpi_stages(st: State<'_, AppState>, stages: Vec<(u16, u16)>, active: u8, profile: u8) -> R<(u16, u16)> {
    log_info!("ui", "DPI: stages {stages:?}, active {active}, profile {profile}");
    with_mouse(&st, "DPI", |m| {
        m.set_dpi_stages(&stages, active, profile)?;
        if let Some(&(x, y)) = stages.get(active.saturating_sub(1) as usize) {
            m.set_dpi(x, y)?;
        }
        m.get_dpi()
    })
}

#[tauri::command]
pub(crate) async fn get_dpi_stages(st: State<'_, AppState>, profile: u8) -> R<DpiStages> {
    with_mouse(&st, "read DPI", |m| m.get_dpi_stages(profile))
}

#[tauri::command]
pub(crate) async fn set_polling(st: State<'_, AppState>, hz: u32) -> R<()> {
    log_info!("ui", "polling rate {hz} Hz");
    with_mouse(&st, "polling", |m| m.set_polling_rate(hz))
}

#[tauri::command]
pub(crate) async fn set_power(st: State<'_, AppState>, idle: u16, low: u8) -> R<()> {
    log_info!("ui", "power: sleep {idle} s, low power {low}%");
    // 0 = the mouse has no such setting, leave it alone
    with_mouse(&st, "power", |m| {
        if idle > 0 {
            m.set_idle_time(idle)?;
        }
        if low > 0 {
            m.set_low_battery_threshold(low)?;
        }
        Ok(())
    })
}
