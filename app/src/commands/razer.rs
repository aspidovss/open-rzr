//! Commands: Razer software (Windows).

use crate::*;

#[derive(Serialize)]
pub(crate) struct RazerInfo {
    #[serde(flatten)]
    pub(crate) software: synapse::RazerSoftware,
    /// open-rzr stopped services earlier and can start them again
    pub(crate) can_restore: bool,
}

#[tauri::command]
pub(crate) async fn razer_software(st: State<'_, AppState>) -> R<RazerInfo> {
    let software = synapse::scan();
    let can_restore = !lock(&st.store).razer_saved.is_empty();
    Ok(RazerInfo { software, can_restore })
}

/// Stop Razer services and programs (UAC prompt); `autostart` = also keep them from starting with Windows.
#[tauri::command]
pub(crate) async fn stop_razer_software(st: State<'_, AppState>, autostart: bool) -> R<RazerInfo> {
    log_info!("ui", "stop Razer software (keep from autostart: {autostart})");
    let saved = synapse::stop(autostart).map_err(|e| err(&e))?;
    {
        let mut s = lock(&st.store);
        for (name, start) in saved {
            // keep the first recorded start mode: that is the original one
            if !s.razer_saved.iter().any(|(n, _)| *n == name) {
                s.razer_saved.push((name, start));
            } else if !start.is_empty() {
                if let Some(e) = s.razer_saved.iter_mut().find(|(n, st)| *n == name && st.is_empty()) {
                    e.1 = start;
                }
            }
        }
        s.save();
    }
    *lock(&st.mouse) = None;
    razer_software(st).await
}

/// Start the services open-rzr stopped and restore how they started with Windows.
#[tauri::command]
pub(crate) async fn start_razer_software(st: State<'_, AppState>) -> R<RazerInfo> {
    let saved = lock(&st.store).razer_saved.clone();
    log_info!("ui", "start Razer software again: {saved:?}");
    synapse::start_again(&saved).map_err(|e| err(&e))?;
    {
        let mut s = lock(&st.store);
        s.razer_saved.clear();
        s.save();
    }
    *lock(&st.mouse) = None;
    razer_software(st).await
}

#[derive(Serialize)]
pub(crate) struct KnownDevice {
    pub(crate) pid: u16,
    pub(crate) name: &'static str,
    pub(crate) verified: bool,
    pub(crate) source: &'static str,
    /// may answer button/macro/profile commands (checked at connection)
    pub(crate) probe: bool,
    pub(crate) direct: Vec<&'static str>,
    pub(crate) openrazer: Vec<&'static str>,
}

/// Every supported model (own modules + OpenRazer database).
#[tauri::command]
pub(crate) async fn supported_devices() -> R<Vec<KnownDevice>> {
    Ok(devices::all()
        .into_iter()
        .map(|d| KnownDevice {
            pid: d.pid,
            name: d.name,
            verified: d.verified(),
            source: d.source.code(),
            probe: d.source == devices::Source::OpenRazer && devices::generic::may_have_onboard(d.transaction_id),
            direct: feat::names(d.direct),
            openrazer: feat::names(d.openrazer & !d.direct),
        })
        .collect())
}

#[tauri::command]
pub(crate) async fn quit_app(app: AppHandle) -> R<()> {
    log_info!("ui", "quit");
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub(crate) async fn install_udev() -> R<()> {
    log_info!("ui", "install udev rule");
    udev::install(true)
}
