//! Commands: diagnostics.

use crate::*;

#[derive(Serialize)]
pub(crate) struct LogChunk {
    pub(crate) seq: u64,
    pub(crate) lines: Vec<String>,
}

#[tauri::command]
pub(crate) async fn logs_since(since: u64) -> R<LogChunk> {
    let (seq, lines) = logs::recent(since);
    Ok(LogChunk { seq, lines })
}

#[tauri::command]
pub(crate) async fn set_log_level(st: State<'_, AppState>, level: String) -> R<()> {
    let l = Level::parse(&level).ok_or("[invalid] level: DEBUG/INFO/WARN/ERROR")?;
    logs::set_level(l);
    log_info!("ui", "log level: {}", l.as_str());
    let mut s = lock(&st.store);
    s.log_level = l.as_str().into();
    s.save();
    Ok(())
}

#[tauri::command]
pub(crate) async fn open_log_folder() -> R<()> {
    logs::open_folder().map_err(|e| e.to_string())
}

/// UI-side events (clicks, page switches) go to the same log.
#[tauri::command]
pub(crate) async fn ui_log(level: String, msg: String) -> R<()> {
    match Level::parse(&level).unwrap_or(Level::Info) {
        Level::Error => log_error!("ui", "{msg}"),
        Level::Warn => log_warn!("ui", "{msg}"),
        Level::Debug => log_debug!("ui", "{msg}"),
        Level::Info => log_info!("ui", "{msg}"),
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn save_report(st: State<'_, AppState>) -> R<String> {
    let mut rep = String::new();
    rep += &format!("open-rzr {} / {} {}\n", openrzr::VERSION, std::env::consts::OS, std::env::consts::ARCH);
    rep += &format!("time (UTC): {}\n\nRazer HID devices:\n", logs::timestamp());
    for d in openrzr::transport::enumerate(st.mock) {
        rep += &format!("  {} {}\n", d.label(), d.path);
    }
    let or = compat::status();
    rep += &format!(
        "\nOpenRazer: driver {}, daemon {}, mode {}, bound {:?}\n",
        if or.driver_loaded { "loaded" } else { "not loaded" },
        if or.daemon_running { "running" } else { "not running" },
        or.mode,
        or.bound
    );
    let dev = with_mouse(&st, "report", |m| {
        let mut s = format!("\nConnected: {} ({})\n", m.name(), m.info.path);
        s += &format!(
            "module: {} | via OpenRazer: {:?} | features: {}\n",
            m.spec.source.code(),
            m.openrazer,
            feat::names(m.features).join(", ")
        );
        s += &format!("firmware: {:?}\n", m.firmware());
        s += &format!("DPI: {:?}\nstages: {:?}\npolling: {:?}\n", m.get_dpi(), m.get_dpi_stages(1), m.get_polling_rate());
        s += &format!("battery: {:?}\nsleep: {:?}\nprofiles: {:?}\nmacros: {:?}\nside plate: {:?}\n", m.get_battery(), m.get_idle_time(), m.list_profiles(), m.macro_ids(), m.side_plate());
        s += "\nButtons (profile 1, all):\n";
        for b in m.scan_buttons(1, false, true).unwrap_or_default() {
            let raw: Vec<String> = b.action.encode().unwrap_or_default().iter().map(|x| format!("{x:02x}")).collect();
            s += &format!("  0x{:02x} {:<24} {:<40} [{}]\n", b.id, b.name, b.action.describe(), raw.join(" "));
        }
        Ok(s)
    });
    rep += &dev.unwrap_or_else(|e| format!("\nMouse: {e}\n"));
    rep += "\nLog (last 800 lines):\n";
    for l in logs::tail(800) {
        rep += &l;
        rep.push('\n');
    }
    let path = logs::log_dir().join(format!("report-{}.txt", logs::timestamp().replace([' ', ':', '.'], "-")));
    std::fs::create_dir_all(logs::log_dir()).map_err(|e| e.to_string())?;
    std::fs::write(&path, rep).map_err(|e| e.to_string())?;
    log_info!("ui", "report saved: {}", path.display());
    Ok(path.display().to_string())
}

#[tauri::command]
pub(crate) async fn raw_command(st: State<'_, AppState>, cmd: String, args: String, size: Option<u8>) -> R<String> {
    let c = u16::from_str_radix(cmd.trim().trim_start_matches("0x"), 16).map_err(|_| "[invalid] command: 4 hex digits")?;
    let hex: String = args.split_whitespace().collect();
    if hex.len() % 2 != 0 {
        return Err("[invalid] arguments: hex bytes".into());
    }
    let bytes = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| "[invalid] arguments: hex bytes")?;
    log_info!("ui", "raw command {c:04x} {args} size {size:?}");
    let a = with_mouse(&st, "raw command", |m| m.req(c, &bytes, size))?;
    let n = size.unwrap_or(16).max(1) as usize;
    Ok(a[..n.min(80)].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "))
}

/// Every HID endpoint the system reports (Diagnostics), Razer first.
#[tauri::command]
pub(crate) async fn all_hid_devices() -> R<Vec<DeviceInfo>> {
    let mut v = openrzr::transport::enumerate_all();
    v.sort_by_key(|d| (d.vid != devices::RAZER_VID, d.vid, d.pid, d.usage_page, d.usage, d.path.clone()));
    log_info!("ui", "all HID devices: {}", v.len());
    for d in v.iter().filter(|d| d.vid == devices::RAZER_VID) {
        log_info!("ui", "  {} page 0x{:04x} usage 0x{:04x}{} {}", d.label(), d.usage_page, d.usage, if d.bluetooth { " BT" } else { "" }, d.path);
    }
    Ok(v)
}

/// Diagnostics → Wireless: HID collections, probes and GATT services of a mouse on Bluetooth.
/// `probe`: "" | "vendor" | "all" (sends the read-only firmware request).
#[tauri::command]
pub(crate) async fn wireless_debug(probe: String) -> R<openrzr::wireless::Report> {
    log_info!("ui", "wireless debug (probe: {})", if probe.is_empty() { "no" } else { probe.as_str() });
    Ok(openrzr::wireless::report(&probe))
}

/// Diagnostics → Wireless → Razer GATT channel: subscribe, write a request ("" = listen only,
/// "fw" = read-only firmware request, or hex), collect notifications.
#[tauri::command]
pub(crate) async fn wireless_gatt(request: String, listen_ms: u32) -> R<openrzr::wireless::Exchange> {
    log_info!("ui", "wireless GATT exchange (request: {})", if request.trim().is_empty() { "listen only" } else { request.trim() });
    Ok(tauri::async_runtime::spawn_blocking(move || openrzr::wireless::gatt_exchange(&request, listen_ms)).await.unwrap_or_default())
}

#[derive(Serialize)]
pub(crate) struct DeviceFiles {
    pub(crate) folder: String,
    pub(crate) files: Vec<openrzr::devices::external::Loaded>,
}

/// Diagnostics → Device files: read the folder again and say what was loaded.
#[tauri::command]
pub(crate) async fn device_files() -> R<DeviceFiles> {
    log_info!("ui", "device files: reload");
    let folder = openrzr::devices::external::folder(&logs::config_dir());
    let _ = std::fs::create_dir_all(&folder);
    Ok(DeviceFiles { folder: folder.display().to_string(), files: openrzr::caps::load_device_files() })
}

/// Drivers serving Razer devices (Diagnostics): standard HID, or Razer packages / filters left by Synapse.
#[tauri::command]
pub(crate) async fn razer_drivers() -> R<Vec<openrzr::drivers::DriverInfo>> {
    let v = openrzr::drivers::list();
    for d in &v {
        log_info!("ui", "driver: {} — {} {} {} [{}]{}", d.device, d.provider, d.version, d.service, d.filters, if d.razer { " RAZER" } else { "" });
    }
    Ok(v)
}
