//! Commands: app / devices.

use crate::*;

#[derive(Serialize)]
pub(crate) struct AppInfo {
    pub(crate) version: &'static str,
    pub(crate) os: &'static str,
    pub(crate) log_file: String,
    pub(crate) config_dir: String,
    pub(crate) mock: bool,
    pub(crate) log_level: String,
    /// OpenRazer revision the device database was generated from
    pub(crate) openrazer_db: &'static str,
    pub(crate) db_models: usize,
    pub(crate) hotplug: bool,
}

#[tauri::command]
pub(crate) async fn app_info(st: State<'_, AppState>) -> R<AppInfo> {
    Ok(AppInfo {
        version: openrzr::VERSION,
        os: std::env::consts::OS,
        log_file: logs::log_file().display().to_string(),
        config_dir: logs::config_dir().display().to_string(),
        mock: st.mock,
        log_level: logs::level().as_str().into(),
        openrazer_db: devices::openrazer_db::REVISION,
        db_models: devices::all().len(),
        hotplug: st.hotplug.load(Ordering::Relaxed),
    })
}

#[derive(Serialize)]
pub(crate) struct DeviceGroup {
    pub(crate) label: String,
    pub(crate) pid: u16,
    pub(crate) candidates: Vec<DeviceInfo>,
}

#[tauri::command]
pub(crate) async fn list_devices(st: State<'_, AppState>) -> R<Vec<DeviceGroup>> {
    let devs = find_devices(st.mock);
    let mut groups: Vec<DeviceGroup> = Vec::new();
    for d in devs {
        let label = if d.backend == "mock" {
            d.name.clone()
        } else {
            devices::spec_for(d.pid).map(|s| s.name.to_string()).unwrap_or_else(|| d.label())
        };
        match groups.iter_mut().find(|g| g.label == label) {
            Some(g) => g.candidates.push(d),
            None => groups.push(DeviceGroup { label, pid: d.pid, candidates: vec![d] }),
        }
    }
    log_info!("ui", "device search: {} found", groups.len());
    Ok(groups)
}

#[derive(Serialize, Default)]
pub(crate) struct ConnectInfo {
    pub(crate) name: String,
    pub(crate) pid: u16,
    pub(crate) path: String,
    /// own tested module (true) or OpenRazer database entry (false)
    pub(crate) verified: bool,
    /// "verified" | "family" | "openrazer"
    pub(crate) source: &'static str,
    /// features found by probing a model without an own module: work, but not tested on it
    pub(crate) experimental: Vec<&'static str>,
    /// usable features: "dpi", "buttons", ... (openrzr::feat names)
    pub(crate) features: Vec<&'static str>,
    /// sysfs directory when connected through the OpenRazer driver
    pub(crate) openrazer: Option<String>,
    /// zone -> effects it supports
    pub(crate) zone_effects: HashMap<String, Vec<&'static str>>,
    pub(crate) wireless: bool,
    pub(crate) bluetooth: bool,
    pub(crate) has_battery: bool,
    pub(crate) max_dpi: u16,
    pub(crate) max_stages: usize,
    pub(crate) lighting: Vec<String>,
    pub(crate) firmware: String,
    pub(crate) serial: String,
    pub(crate) dpi: Option<(u16, u16)>,
    pub(crate) stages: Option<DpiStages>,
    pub(crate) poll: Option<u32>,
    pub(crate) battery: Option<u8>,
    pub(crate) charging: bool,
    pub(crate) idle: Option<u16>,
    pub(crate) low: Option<u8>,
    pub(crate) profiles: Vec<u8>,
    /// mouse has swappable side plates
    pub(crate) side_plates: bool,
    /// detected plate: 12/6/2, 0 = none attached
    pub(crate) plate: Option<u8>,
    /// saved filter choice for this mouse: "auto" | "12" | "6" | "2" | "all"
    pub(crate) plate_choice: String,
}

#[tauri::command]
pub(crate) async fn connect(st: State<'_, AppState>, candidates: Vec<DeviceInfo>) -> R<ConnectInfo> {
    log_info!("ui", "connect: {} candidates", candidates.len());
    *lock(&st.mouse) = None;
    let bluetooth = candidates.iter().any(|c| c.bluetooth || devices::spec_for(c.pid).is_some_and(|s| s.bluetooth));
    let mut m = RazerMouse::open_first(&candidates).map_err(|e| {
        log_error!("ui", "connect failed: {e}");
        if bluetooth {
            format!("[bluetooth] {e}")
        } else {
            err(&e)
        }
    })?;
    let mut ci = ConnectInfo {
        name: m.name().into(),
        pid: m.info.pid,
        path: m.info.path.clone(),
        verified: m.spec.verified(),
        source: m.spec.source.code(),
        experimental: feat::names(m.experimental),
        features: feat::names(m.features),
        openrazer: m.openrazer.as_ref().map(|d| d.display().to_string()),
        zone_effects: m.zones.iter().map(|z| (z.id.to_string(), m.zone_effects(z.id))).collect(),
        wireless: m.spec.wireless,
        bluetooth: m.info.bluetooth || m.spec.bluetooth,
        has_battery: m.has(feat::BATTERY),
        max_dpi: m.spec.max_dpi,
        max_stages: m.spec.max_dpi_stages,
        lighting: m.zones.iter().map(|z| z.id.to_string()).collect(),
        firmware: m.firmware().unwrap_or_default(),
        serial: m.serial().unwrap_or_default(),
        dpi: m.get_dpi().ok(),
        stages: m.get_dpi_stages(1).ok(),
        poll: m.get_polling_rate().ok(),
        ..Default::default()
    };
    if m.has(feat::BATTERY) {
        ci.battery = m.get_battery().ok();
        ci.charging = m.is_charging().unwrap_or(false);
    }
    ci.idle = m.get_idle_time().ok();
    ci.low = m.get_low_battery_threshold().ok();
    ci.profiles = m.list_profiles();
    ci.side_plates = m.spec.side_plates && m.has(feat::SIDE_PLATE);
    if ci.side_plates {
        ci.plate = m.side_plate().unwrap_or_else(|e| {
            log_warn!("device", "side plate not detected: {e}");
            None
        });
    }
    log_info!("ui", "connected: {} {} serial {} profiles {:?}", ci.name, ci.firmware, ci.serial, ci.profiles);
    let serial = if ci.serial.is_empty() { format!("{:04x}", m.info.pid) } else { ci.serial.clone() };
    ci.plate_choice = lock(&st.store).plate_choice.get(&serial).cloned().unwrap_or_else(|| "auto".into());
    log_info!("ui", "side plate: detected {:?}, choice \"{}\"", ci.plate, ci.plate_choice);
    *lock(&st.serial) = serial;
    *lock(&st.mouse) = Some(m);
    *lock(&st.temp_layer) = None;
    Ok(ci)
}

#[tauri::command]
pub(crate) async fn battery(st: State<'_, AppState>) -> R<(u8, bool)> {
    let (lvl, ch) = with_mouse(&st, "battery", |m| Ok((m.get_battery()?, m.is_charging().unwrap_or(false))))?;
    if let Some(t) = lock(&st.tray).as_ref() {
        let _ = t.icon.set_tooltip(Some(format!("open-rzr · {lvl}%{}", if ch { " ⚡" } else { "" })));
    }
    Ok((lvl, ch))
}
