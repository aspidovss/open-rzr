//! open-rzr — Tauri shell around the openrzr core. All device I/O happens here, in Rust;
//! the web UI only calls these commands. Nothing leaves the computer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod library;
mod store;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use openrzr::device::{find_devices, DpiStages, RazerMouse};
use openrzr::lighting::{self, ZoneState};
use openrzr::logs::{self, Level};
use openrzr::macro_doc::{self, Compiled, Item, SoftAction};
use openrzr::program_actions;
use openrzr::features::profiles::TEMPORARY;
use openrzr::{compat, devices, feat, foreground, hidkeys, hotplug, macros, synapse, udev, Action, Backup, ButtonInfo, DeviceInfo, MacroStep, Profile};
use openrzr::{log_debug, log_error, log_info, log_warn};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent, Wry};

use store::{SavedMacro, Store};

struct TrayHandles {
    icon: TrayIcon<Wry>,
    show: MenuItem<Wry>,
    quit: MenuItem<Wry>,
    /// "Open Polychromatic" (Linux, when it is installed)
    poly: Option<MenuItem<Wry>>,
}

struct AppState {
    tray: Mutex<Option<TrayHandles>>,
    mouse: Mutex<Option<RazerMouse>>,
    store: Mutex<Store>,
    serial: Mutex<String>,
    mock: bool,
    /// USB plug/unplug notifications work (otherwise the window polls)
    hotplug: AtomicBool,
    /// what was last written to the temporary layer (to write only differences)
    temp_layer: Mutex<Option<Profile>>,
    /// program actions of macros: trigger key -> (macro, actions)
    actions: program_actions::Table,
    /// the close button was pressed and the window was asked (it may have unsaved changes)
    closing: Mutex<Option<std::time::Instant>>,
    /// where the window was before it was closed to the tray (logical x, y, width, height, maximized)
    geometry: Mutex<Option<(f64, f64, f64, f64, bool)>>,
    /// page and device the window showed, kept while it is closed to the tray
    ui_memory: Mutex<String>,
}

type R<T> = Result<T, String>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Run `f` with the connected mouse; logs and stringifies errors.
fn with_mouse<T>(st: &AppState, what: &str, f: impl FnOnce(&mut RazerMouse) -> openrzr::Result<T>) -> R<T> {
    let mut g = lock(&st.mouse);
    let m = g.as_mut().ok_or_else(|| "[disconnected] Mouse is not connected".to_string())?;
    match f(m) {
        Ok(v) => Ok(v),
        Err(e) => {
            log_error!("ui", "{what}: {e}");
            if e.code() == "io" {
                // the OS handle broke: the mouse was unplugged or went to sleep
                *g = None;
                log_warn!("app", "mouse disconnected");
                return Err(format!("[disconnected] {e}"));
            }
            Err(err(&e))
        }
    }
}

/// "[code] message" — the UI translates by code and falls back to the English message.
fn err(e: &openrzr::Error) -> String {
    format!("[{}] {e}", e.code())
}

/// Show the main window; it is created anew if it was closed to the tray.
fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    if let Err(e) = create_main(app) {
        log_error!("app", "cannot open the window: {e}");
    }
}

/// The main window. Closing it to the tray destroys it together with its WebView2 / WebKit
/// processes (~120 MB); only the Rust part stays in the background.
fn create_main(app: &AppHandle) -> tauri::Result<()> {
    let geometry = *lock(&app.state::<AppState>().geometry);
    let mut b = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title("open-rzr")
        .inner_size(1140.0, 780.0)
        .min_inner_size(920.0, 620.0)
        .disable_drag_drop_handler()
        .visible(true);
    match geometry {
        Some((x, y, w, h, max)) => b = b.position(x, y).inner_size(w, h).maximized(max),
        None => b = b.center(),
    }
    let w = b.build()?;
    let _ = w.set_focus();
    log_debug!("app", "window created");
    Ok(())
}

/// Remember where the window is and destroy it (the app keeps running in the tray).
fn close_to_tray(window: &tauri::Window) {
    let st = window.state::<AppState>();
    *lock(&st.closing) = None;
    if let (Ok(sf), Ok(pos), Ok(size)) = (window.scale_factor(), window.outer_position(), window.inner_size()) {
        let max = window.is_maximized().unwrap_or(false);
        let (p, z) = (pos.to_logical::<f64>(sf), size.to_logical::<f64>(sf));
        if !max && z.width > 100.0 {
            *lock(&st.geometry) = Some((p.x, p.y, z.width, z.height, false));
        } else if let Some(g) = lock(&st.geometry).as_mut() {
            g.4 = max;
        }
    }
    match window.destroy() {
        Ok(()) => log_info!("app", "window closed to the tray (WebView released)"),
        Err(e) => {
            log_warn!("app", "cannot destroy the window, hiding it: {e}");
            let _ = window.hide();
        }
    }
}

/// The window agreed to close (no unsaved changes, or the user discarded them).
#[tauri::command]
async fn hide_to_tray(window: tauri::Window) -> R<()> {
    close_to_tray(&window);
    Ok(())
}

/// The user kept the window open (unsaved changes).
#[tauri::command]
async fn close_cancelled(st: State<'_, AppState>) -> R<()> {
    *lock(&st.closing) = None;
    Ok(())
}

#[tauri::command]
async fn ui_state_get(st: State<'_, AppState>) -> R<String> {
    Ok(lock(&st.ui_memory).clone())
}

#[tauri::command]
async fn ui_state_set(st: State<'_, AppState>, state: String) -> R<()> {
    *lock(&st.ui_memory) = state;
    Ok(())
}

fn poly_label(lang: &str) -> &'static str {
    match lang {
        "ru" => "Открыть Polychromatic",
        _ => "Open Polychromatic",
    }
}

/// Polychromatic manages the lighting while it runs: profile lighting is left to it.
fn globals_of(p: &Profile) -> Profile {
    let mut g = p.clone();
    if compat::polychromatic_running() {
        log_debug!("app", "Polychromatic is running: profile lighting not applied");
        g.lighting.clear();
    }
    g
}

fn tray_labels(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "ru" => ("Открыть open-rzr", "Выход"),
        _ => ("Open open-rzr", "Quit"),
    }
}

// ------------------------------------------------------------------ app / devices

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    os: &'static str,
    log_file: String,
    config_dir: String,
    mock: bool,
    log_level: String,
    /// OpenRazer revision the device database was generated from
    openrazer_db: &'static str,
    db_models: usize,
    hotplug: bool,
}

#[tauri::command]
async fn app_info(st: State<'_, AppState>) -> R<AppInfo> {
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
struct DeviceGroup {
    label: String,
    pid: u16,
    candidates: Vec<DeviceInfo>,
}

#[tauri::command]
async fn list_devices(st: State<'_, AppState>) -> R<Vec<DeviceGroup>> {
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
struct ConnectInfo {
    name: String,
    pid: u16,
    path: String,
    /// own tested module (true) or OpenRazer database entry (false)
    verified: bool,
    /// "verified" | "family" | "openrazer"
    source: &'static str,
    /// features found by probing a model without an own module: work, but not tested on it
    experimental: Vec<&'static str>,
    /// usable features: "dpi", "buttons", ... (openrzr::feat names)
    features: Vec<&'static str>,
    /// sysfs directory when connected through the OpenRazer driver
    openrazer: Option<String>,
    /// zone -> effects it supports
    zone_effects: HashMap<String, Vec<&'static str>>,
    wireless: bool,
    bluetooth: bool,
    has_battery: bool,
    max_dpi: u16,
    max_stages: usize,
    lighting: Vec<String>,
    firmware: String,
    serial: String,
    dpi: Option<(u16, u16)>,
    stages: Option<DpiStages>,
    poll: Option<u32>,
    battery: Option<u8>,
    charging: bool,
    idle: Option<u16>,
    low: Option<u8>,
    profiles: Vec<u8>,
    /// mouse has swappable side plates
    side_plates: bool,
    /// detected plate: 12/6/2, 0 = none attached
    plate: Option<u8>,
    /// saved filter choice for this mouse: "auto" | "12" | "6" | "2" | "all"
    plate_choice: String,
}

#[tauri::command]
async fn connect(st: State<'_, AppState>, candidates: Vec<DeviceInfo>) -> R<ConnectInfo> {
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
async fn battery(st: State<'_, AppState>) -> R<(u8, bool)> {
    let (lvl, ch) = with_mouse(&st, "battery", |m| Ok((m.get_battery()?, m.is_charging().unwrap_or(false))))?;
    if let Some(t) = lock(&st.tray).as_ref() {
        let _ = t.icon.set_tooltip(Some(format!("open-rzr · {lvl}%{}", if ch { " ⚡" } else { "" })));
    }
    Ok((lvl, ch))
}

// ------------------------------------------------------------------ settings

#[derive(Serialize)]
struct Settings {
    /// "" until the user confirmed a language on first run
    language: String,
    autostart: bool,
    start_minimized: bool,
    close_to_tray: bool,
    log_level: String,
    /// "synapse" | "classic"
    ui_style: String,
}

#[tauri::command]
async fn get_settings(st: State<'_, AppState>) -> R<Settings> {
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
async fn set_ui_style(st: State<'_, AppState>, style: String) -> R<()> {
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
async fn set_language(st: State<'_, AppState>, language: String) -> R<()> {
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
async fn set_autostart(enabled: bool, minimized: bool) -> R<autostart::State> {
    log_info!("ui", "autostart: {enabled}, minimized: {minimized}");
    autostart::set(enabled, minimized).map_err(|e| {
        log_error!("app", "autostart: {e}");
        format!("[io] {e}")
    })?;
    Ok(autostart::get())
}

#[tauri::command]
async fn set_close_to_tray(st: State<'_, AppState>, enabled: bool) -> R<()> {
    log_info!("ui", "close to tray: {enabled}");
    let mut s = lock(&st.store);
    s.close_to_tray = Some(enabled);
    s.save();
    Ok(())
}

#[tauri::command]
async fn default_action(st: State<'_, AppState>, id: u8) -> R<Option<Action>> {
    let g = lock(&st.mouse);
    Ok(match g.as_ref() {
        Some(m) => m.default_action(id),
        None => devices::default_action(id),
    })
}

// ------------------------------------------------------------------ OpenRazer compatibility

#[tauri::command]
async fn openrazer_status(st: State<'_, AppState>) -> R<compat::Status> {
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
async fn set_openrazer_mode(st: State<'_, AppState>, mode: String) -> R<()> {
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
async fn open_polychromatic() -> R<()> {
    log_info!("ui", "open Polychromatic");
    compat::open_polychromatic().map_err(|e| err(&e))
}

#[tauri::command]
async fn stop_openrazer_daemon(st: State<'_, AppState>) -> R<()> {
    log_info!("ui", "stop OpenRazer daemon");
    compat::stop_daemon().map_err(|e| err(&e))?;
    *lock(&st.mouse) = None;
    Ok(())
}

// ------------------------------------------------------------------ Razer software (Windows)

#[derive(Serialize)]
struct RazerInfo {
    #[serde(flatten)]
    software: synapse::RazerSoftware,
    /// open-rzr stopped services earlier and can start them again
    can_restore: bool,
}

#[tauri::command]
async fn razer_software(st: State<'_, AppState>) -> R<RazerInfo> {
    let software = synapse::scan();
    let can_restore = !lock(&st.store).razer_saved.is_empty();
    Ok(RazerInfo { software, can_restore })
}

/// Stop Razer services and programs (UAC prompt); `autostart` = also keep them from starting with Windows.
#[tauri::command]
async fn stop_razer_software(st: State<'_, AppState>, autostart: bool) -> R<RazerInfo> {
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
async fn start_razer_software(st: State<'_, AppState>) -> R<RazerInfo> {
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
struct KnownDevice {
    pid: u16,
    name: &'static str,
    verified: bool,
    source: &'static str,
    /// may answer button/macro/profile commands (checked at connection)
    probe: bool,
    direct: Vec<&'static str>,
    openrazer: Vec<&'static str>,
}

/// Every supported model (own modules + OpenRazer database).
#[tauri::command]
async fn supported_devices() -> R<Vec<KnownDevice>> {
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
async fn quit_app(app: AppHandle) -> R<()> {
    log_info!("ui", "quit");
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn install_udev() -> R<()> {
    log_info!("ui", "install udev rule");
    udev::install(true)
}

// ------------------------------------------------------------------ performance / power

#[tauri::command]
async fn set_dpi_stages(st: State<'_, AppState>, stages: Vec<(u16, u16)>, active: u8, profile: u8) -> R<(u16, u16)> {
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
async fn get_dpi_stages(st: State<'_, AppState>, profile: u8) -> R<DpiStages> {
    with_mouse(&st, "read DPI", |m| m.get_dpi_stages(profile))
}

#[tauri::command]
async fn set_polling(st: State<'_, AppState>, hz: u32) -> R<()> {
    log_info!("ui", "polling rate {hz} Hz");
    with_mouse(&st, "polling", |m| m.set_polling_rate(hz))
}

#[tauri::command]
async fn set_power(st: State<'_, AppState>, idle: u16, low: u8) -> R<()> {
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

// ------------------------------------------------------------------ buttons

#[tauri::command]
async fn side_plate(st: State<'_, AppState>) -> R<Option<u8>> {
    with_mouse(&st, "side plate", |m| m.side_plate())
}

#[tauri::command]
async fn set_plate_choice(st: State<'_, AppState>, choice: String) -> R<()> {
    let serial = lock(&st.serial).clone();
    log_info!("ui", "plate filter: {choice}");
    let mut s = lock(&st.store);
    s.plate_choice.insert(serial, choice);
    s.save();
    Ok(())
}

/// `plate`: 12/6/2 = list only that plate's buttons, 0 = no filter.
#[tauri::command]
async fn scan_buttons(st: State<'_, AppState>, profile: u8, hypershift: bool, all: bool, plate: u8) -> R<Vec<ButtonInfo>> {
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
async fn set_button(st: State<'_, AppState>, id: u8, action: Action, profile: u8, hypershift: bool) -> R<Action> {
    log_info!("ui", "bind 0x{id:02x} (profile {profile}{}): {}", if hypershift { ", Hypershift" } else { "" }, action.describe());
    with_mouse(&st, "bind button", |m| {
        m.set_button(id, &action, profile, hypershift)?;
        m.get_button(id, profile, hypershift)
    })
}

/// Names the user gave to buttons of the connected mouse: "0x40" style id (lowercase hex, no prefix) -> name.
#[tauri::command]
async fn button_names(st: State<'_, AppState>) -> R<HashMap<String, String>> {
    let serial = lock(&st.serial).clone();
    Ok(lock(&st.store).button_names.get(&serial).cloned().unwrap_or_default())
}

#[tauri::command]
async fn rename_button(st: State<'_, AppState>, id: u8, name: String) -> R<()> {
    let serial = lock(&st.serial).clone();
    log_info!("ui", "rename 0x{id:02x} -> {name}");
    let mut s = lock(&st.store);
    s.button_names.entry(serial).or_default().insert(format!("{id:02x}"), name);
    s.save();
    Ok(())
}

#[tauri::command]
async fn copy_profile(st: State<'_, AppState>, from: u8, to: u8) -> R<usize> {
    log_info!("ui", "copy profile {from} -> {to}");
    with_mouse(&st, "copy profile", |m| m.copy_profile(from, to))
}

#[tauri::command]
async fn describe_action(action: Action) -> R<String> {
    Ok(action.describe())
}

// ------------------------------------------------------------------ macros

#[derive(Serialize)]
struct MouseMacro {
    id: u16,
    steps: Vec<MacroStep>,
    text: Vec<String>,
}

#[derive(Serialize)]
struct MacrosOnMouse {
    macros: Vec<MouseMacro>,
    flash: Option<(u32, u32, u32)>,
}

#[tauri::command]
async fn macros_on_mouse(st: State<'_, AppState>) -> R<MacrosOnMouse> {
    with_mouse(&st, "read macros", |m| {
        let mut out = Vec::new();
        for id in m.macro_ids()? {
            let steps = m.read_macro(id)?;
            let text = steps.iter().map(|s| s.describe()).collect();
            out.push(MouseMacro { id, steps, text });
        }
        Ok(MacrosOnMouse { macros: out, flash: m.flash_usage().ok() })
    })
}

/// Build the program-actions table from the library (trigger key -> actions).
fn program_table(store: &Store) -> HashMap<u8, (String, Vec<SoftAction>)> {
    store
        .macros
        .iter()
        .filter_map(|(n, m)| {
            let acts: Vec<SoftAction> = m.software.iter().filter(|a| a.on).cloned().collect();
            match (m.trigger, acts.is_empty()) {
                (Some(k), false) => Some((k, (n.clone(), acts))),
                _ => None,
            }
        })
        .collect()
}

fn refresh_program_table(st: &AppState) {
    let t = program_table(&lock(&st.store));
    *lock(&st.actions) = t;
    program_actions::refresh();
}

/// Compile an editor document; other library macros can be nested by name.
/// A trigger key is chosen when there are program actions: the macro's own, else a free F13–F24.
fn compile_doc(store: &Store, name: &str, items: &[Item], software: &[SoftAction]) -> Result<Compiled, String> {
    let trigger = if software.iter().any(|a| a.on) {
        let own = store.macros.get(name).and_then(|m| m.trigger);
        let used: Vec<u8> = store.macros.iter().filter(|(n, _)| n.as_str() != name).filter_map(|(_, m)| m.trigger).collect();
        match own.filter(|k| !used.contains(k)).or_else(|| macro_doc::free_trigger(&used)) {
            Some(k) => Some(k),
            None => return Err("[triggers|] all trigger keys F13–F24 are taken by other macros with program actions".into()),
        }
    } else {
        None
    };
    let lookup = |n: &str| store.macros.get(n).map(|m| m.editor_items());
    macro_doc::compile(name, items, trigger, &lookup)
}

#[derive(Serialize)]
struct MacroCheck {
    ok: bool,
    error: String,
    steps: usize,
    size: usize,
    ms: i64,
    trigger: String,
    lines: Vec<String>,
}

/// Live check for the editor: does it fit into the mouse, how long it takes, trigger key.
#[tauri::command]
async fn check_macro(st: State<'_, AppState>, name: String, items: Vec<Item>, software: Vec<SoftAction>) -> R<MacroCheck> {
    let s = lock(&st.store);
    let lines = items.iter().map(macro_doc::describe).collect();
    Ok(match compile_doc(&s, &name, &items, &software) {
        Ok(c) => MacroCheck {
            ok: true,
            error: String::new(),
            steps: c.steps.len(),
            size: c.size,
            ms: c.ms,
            trigger: c.trigger.map(program_actions::trigger_name).unwrap_or_default(),
            lines,
        },
        Err(e) => MacroCheck { ok: false, error: e, steps: 0, size: 0, ms: 0, trigger: String::new(), lines },
    })
}

fn store_doc(st: &AppState, name: &str, id: u16, items: Vec<Item>, software: Vec<SoftAction>) -> Result<Compiled, String> {
    let mut s = lock(&st.store);
    let compiled = compile_doc(&s, name, &items, &software);
    let (steps, trigger) = match &compiled {
        Ok(c) => (c.steps.clone(), c.trigger),
        Err(_) => (s.macros.get(name).map(|m| m.steps.clone()).unwrap_or_default(), s.macros.get(name).and_then(|m| m.trigger)),
    };
    s.macros.insert(name.to_string(), SavedMacro { id, steps, items, software, trigger });
    s.save();
    compiled
}

#[derive(Serialize)]
struct Written {
    bytes: usize,
    trigger: String,
}

#[tauri::command]
async fn write_macro(st: State<'_, AppState>, id: u16, name: String, items: Vec<Item>, software: Vec<SoftAction>) -> R<Written> {
    log_info!("ui", "write macro #{id} \"{name}\" ({} steps, {} program actions)", items.len(), software.len());
    let c = store_doc(&st, &name, id, items, software).map_err(|e| format!("[invalid] {e}"))?;
    refresh_program_table(&st);
    let bytes = with_mouse(&st, "write macro", |m| m.write_macro(id, &c.steps))?;
    Ok(Written { bytes, trigger: c.trigger.map(program_actions::trigger_name).unwrap_or_default() })
}

#[tauri::command]
async fn delete_macro(st: State<'_, AppState>, id: u16) -> R<()> {
    log_info!("ui", "delete macro #{id} from the mouse");
    with_mouse(&st, "delete macro", |m| m.delete_macro(id))
}

#[derive(Serialize)]
struct LibMacro {
    name: String,
    id: u16,
    items: Vec<Item>,
    software: Vec<SoftAction>,
    trigger: String,
}

#[tauri::command]
async fn macro_library(st: State<'_, AppState>) -> R<Vec<LibMacro>> {
    Ok(lock(&st.store)
        .macros
        .iter()
        .map(|(n, m)| LibMacro {
            name: n.clone(),
            id: m.id,
            items: m.editor_items(),
            software: m.software.clone(),
            trigger: m.trigger.map(program_actions::trigger_name).unwrap_or_default(),
        })
        .collect())
}

/// Save to the library. The document is kept even if it does not compile; the error is returned.
#[tauri::command]
async fn save_macro_lib(st: State<'_, AppState>, name: String, id: u16, items: Vec<Item>, software: Vec<SoftAction>) -> R<String> {
    log_info!("ui", "library: saved \"{name}\" (ID {id})");
    let r = store_doc(&st, &name, id, items, software);
    refresh_program_table(&st);
    Ok(r.err().unwrap_or_default())
}

#[tauri::command]
async fn rename_macro_lib(st: State<'_, AppState>, old: String, name: String) -> R<()> {
    let name = name.trim().to_string();
    let mut s = lock(&st.store);
    if name.is_empty() || s.macros.contains_key(&name) {
        return Err("[invalid] name is empty or taken".into());
    }
    let m = s.macros.remove(&old).ok_or("[not_found] macro")?;
    s.macros.insert(name.clone(), m);
    // nested references follow the new name
    for m in s.macros.values_mut() {
        for it in m.items.iter_mut().filter(|i| i.kind == "macro" && i.text == old) {
            it.text = name.clone();
        }
    }
    s.save();
    log_info!("ui", "library: \"{old}\" renamed to \"{name}\"");
    drop(s);
    refresh_program_table(&st);
    Ok(())
}

#[tauri::command]
async fn delete_macro_lib(st: State<'_, AppState>, name: String) -> R<()> {
    log_info!("ui", "library: removed \"{name}\"");
    {
        let mut s = lock(&st.store);
        s.macros.remove(&name);
        s.save();
    }
    refresh_program_table(&st);
    Ok(())
}

/// Try the program actions now (from the editor).
#[tauri::command]
async fn test_program_actions(software: Vec<SoftAction>) -> R<()> {
    std::thread::spawn(move || {
        if let Err(e) = program_actions::run(&software) {
            log_warn!("app", "program actions test: {e}");
        }
    });
    Ok(())
}

#[derive(Serialize)]
struct ActionsStatus {
    status: program_actions::Status,
    /// trigger key -> macro name
    bound: Vec<(String, String)>,
}

#[tauri::command]
async fn program_actions_status(st: State<'_, AppState>) -> R<ActionsStatus> {
    let mut bound: Vec<(String, String)> =
        lock(&st.actions).iter().map(|(k, (n, _))| (program_actions::trigger_name(*k), n.clone())).collect();
    bound.sort();
    Ok(ActionsStatus { status: program_actions::status(), bound })
}

#[tauri::command]
async fn describe_steps(steps: Vec<MacroStep>) -> R<(Vec<String>, usize)> {
    let size = macros::encode(&steps).map(|d| d.len()).unwrap_or(0);
    Ok((steps.iter().map(|s| s.describe()).collect(), size))
}

// ------------------------------------------------------------------ lighting

#[derive(Serialize)]
struct LightingInfo {
    saved: HashMap<String, ZoneState>,
    brightness: HashMap<String, u8>,
}

#[tauri::command]
async fn get_lighting(st: State<'_, AppState>) -> R<LightingInfo> {
    let serial = lock(&st.serial).clone();
    let saved = lock(&st.store).lighting.get(&serial).cloned().unwrap_or_default();
    let brightness = with_mouse(&st, "brightness", |m| {
        let zones: Vec<&str> = m.zones.iter().map(|z| z.id).collect();
        let mut b = HashMap::new();
        for z in zones {
            if let Ok(v) = m.get_brightness(z) {
                b.insert(z.to_string(), v);
            }
        }
        Ok(b)
    })?;
    Ok(LightingInfo { saved, brightness })
}

#[tauri::command]
async fn set_lighting(st: State<'_, AppState>, targets: Vec<(String, ZoneState)>) -> R<()> {
    let serial = lock(&st.serial).clone();
    for (z, s) in &targets {
        log_info!("ui", "lighting {z}: {} {} brightness {}%", s.effect, lighting::hex_color(s.color1), s.brightness);
    }
    with_mouse(&st, "lighting", |m| {
        for (z, s) in &targets {
            m.set_lighting(z, s)?;
            m.set_brightness(z, s.brightness)?;
        }
        Ok(())
    })?;
    let mut store = lock(&st.store);
    // lighting belongs to the selected profile too
    if let Some(id) = store.current_profile.get(&serial).cloned() {
        if let Ok(mut p) = library::load(&id) {
            for (z, s) in &targets {
                p.lighting.insert(z.clone(), s.clone());
            }
            let _ = library::save(&p);
        }
    }
    let e = store.lighting.entry(serial).or_default();
    for (z, s) in targets {
        e.insert(z, s);
    }
    store.save();
    Ok(())
}

// ------------------------------------------------------------------ profiles (library, on-board slots, linking)

#[derive(Serialize)]
struct ProfileMeta {
    id: String,
    name: String,
    links: Vec<String>,
    /// on-board slots (1..5) holding this profile on the connected mouse
    slots: Vec<u8>,
    /// None: in no slot; Some(true): every slot holds exactly this version; Some(false): changed since
    written: Option<bool>,
}

#[derive(Serialize)]
struct ProfilesInfo {
    list: Vec<ProfileMeta>,
    current: String,
    /// slot 1..5 -> profile id
    slots: Vec<Option<String>>,
    /// (total, free, used) bytes of the on-board memory, if the mouse reports it
    flash: Option<(u32, u32, u32)>,
    autoswitch: store::AutoSwitch,
    /// the program in front can be followed on this system
    linking: bool,
    /// changes go straight into the slot (Settings)
    autowrite: bool,
}

/// What a slot must hold for a profile to count as written: bindings, DPI stages, macros.
fn fingerprint(p: &Profile) -> String {
    use std::hash::{Hash, Hasher};
    let text = serde_json::to_string(&(&p.buttons, &p.dpi_stages, &p.macros)).unwrap_or_default();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Remember that `slot` now holds this version of `p`.
fn mark_written(st: &AppState, slot: u8, p: Option<&Profile>) {
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

fn serial_of(st: &AppState) -> String {
    lock(&st.serial).clone()
}

fn slots_of(st: &AppState, serial: &str) -> Vec<Option<String>> {
    let mut v = lock(&st.store).slots.get(serial).cloned().unwrap_or_default();
    v.resize(5, None);
    v
}

/// Where edits of profile `id` go: its on-board slot, or the temporary layer.
/// Where changes of profile `id` go: its slot when "write changes at once" is on, else the
/// temporary layer (applies at once, kept in RAM) until "Write to mouse".
fn target_slot(st: &AppState, serial: &str, id: &str) -> u8 {
    if !lock(&st.store).autowrite {
        return TEMPORARY;
    }
    slots_of(st, serial).iter().position(|s| s.as_deref() == Some(id)).map(|i| i as u8 + 1).unwrap_or(TEMPORARY)
}

fn profiles_info(st: &AppState) -> ProfilesInfo {
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
async fn profiles(st: State<'_, AppState>) -> R<ProfilesInfo> {
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
async fn profile_get(id: String) -> R<Profile> {
    library::load(&id)
}

/// Make `id` the selected profile. A profile that is not in a slot is written to the temporary layer.
#[tauri::command]
async fn profile_select(st: State<'_, AppState>, id: String) -> R<Profile> {
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

fn write_temporary(st: &AppState, p: &Profile) -> R<()> {
    let prev = lock(&st.temp_layer).clone();
    with_mouse(st, "write profile to the temporary layer", |m| {
        m.write_profile(p, TEMPORARY, prev.as_ref())?;
        m.apply_profile_globals(&globals_of(p))
    })?;
    *lock(&st.temp_layer) = Some(p.clone());
    Ok(())
}

/// Change one binding of a profile: saved in the file and written to the mouse (its slot or the temporary layer).
#[tauri::command]
async fn profile_set_button(st: State<'_, AppState>, id: String, button: u8, hypershift: bool, action: Action) -> R<Action> {
    let mut p = library::load(&id)?;
    let slot = target_slot(&st, &serial_of(&st), &id);
    log_info!("ui", "\"{}\" bind 0x{button:02x}{} -> {} (slot {slot})", p.name, if hypershift { " Hypershift" } else { "" }, action.describe());
    let was_modifier = p.button(button).map(|b| b.normal.kind == "hypershift").unwrap_or(false);
    if hypershift && was_modifier {
        return Err("[invalid] this button holds Hypershift; its Hypershift binding cannot change".into());
    }
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
async fn profile_layer(st: State<'_, AppState>, id: String, op: String) -> R<usize> {
    let mut p = library::load(&id)?;
    let slot = target_slot(&st, &serial_of(&st), &id);
    log_info!("ui", "\"{}\" Hypershift layer: {op} (slot {slot})", p.name);
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
async fn profile_set_dpi(st: State<'_, AppState>, id: String, stages: Vec<(u16, u16)>, active: u8) -> R<(u16, u16)> {
    let mut p = library::load(&id)?;
    let slot = target_slot(&st, &serial_of(&st), &id);
    log_info!("ui", "\"{}\" DPI stages {stages:?}, active {active} (slot {slot})", p.name);
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
async fn profile_write(st: State<'_, AppState>, id: String) -> R<ProfilesInfo> {
    let p = library::load(&id)?;
    let serial = serial_of(&st);
    let slots: Vec<u8> =
        slots_of(&st, &serial).iter().enumerate().filter(|(_, s)| s.as_deref() == Some(id.as_str())).map(|(i, _)| i as u8 + 1).collect();
    if slots.is_empty() {
        return Err("[invalid] the profile is in no slot: choose one".into());
    }
    for slot in slots {
        log_info!("ui", "write \"{}\" -> slot {slot}", p.name);
        let n = with_mouse(&st, "write profile to slot", |m| m.write_profile(&p, slot, None))?;
        log_info!("ui", "slot {slot}: {n} records");
        mark_written(&st, slot, Some(&p));
    }
    Ok(profiles_info(&st))
}

#[tauri::command]
async fn set_autowrite(st: State<'_, AppState>, on: bool) -> R<ProfilesInfo> {
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
async fn profile_create(st: State<'_, AppState>, name: String, from: String) -> R<String> {
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
async fn profile_rename(id: String, name: String) -> R<()> {
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
async fn profile_duplicate(id: String) -> R<String> {
    let mut p = library::load(&id)?;
    log_info!("ui", "duplicate profile \"{}\"", p.name);
    p.name = format!("{} (copy)", p.name);
    p.links.clear();
    Ok(library::add(p)?.id)
}

#[tauri::command]
async fn profile_delete(st: State<'_, AppState>, id: String) -> R<()> {
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
async fn profile_reset(st: State<'_, AppState>, id: String) -> R<Profile> {
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

/// Read the profile back from its on-board slot (when it was changed elsewhere).
#[tauri::command]
async fn profile_reload(st: State<'_, AppState>, id: String) -> R<Profile> {
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
async fn profile_export(id: String) -> R<String> {
    let p = library::load(&id)?;
    library::export(&p).map(|f| f.display().to_string())
}

/// Import the text of an exported profile (the window reads the file).
#[tauri::command]
async fn profile_import(text: String) -> R<String> {
    log_info!("ui", "import profile ({} bytes)", text.len());
    Ok(library::import(&text)?.id)
}

#[tauri::command]
async fn open_folder_of(path: String) -> R<()> {
    let p = std::path::PathBuf::from(&path);
    open_path(p.parent().unwrap_or(&p))
}

/// Put profile `id` into on-board slot 1..5 (None = forget the assignment; the mouse keeps its content).
#[tauri::command]
async fn slot_assign(st: State<'_, AppState>, slot: u8, id: Option<String>) -> R<ProfilesInfo> {
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

/// Every HID endpoint the system reports (Diagnostics), Razer first.
#[tauri::command]
async fn all_hid_devices() -> R<Vec<DeviceInfo>> {
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
async fn wireless_debug(probe: String) -> R<openrzr::wireless::Report> {
    log_info!("ui", "wireless debug (probe: {})", if probe.is_empty() { "no" } else { probe.as_str() });
    Ok(openrzr::wireless::report(&probe))
}

/// Diagnostics → Wireless → Razer GATT channel: subscribe, write a request ("" = listen only,
/// "fw" = read-only firmware request, or hex), collect notifications.
#[tauri::command]
async fn wireless_gatt(request: String, listen_ms: u32) -> R<openrzr::wireless::Exchange> {
    log_info!("ui", "wireless GATT exchange (request: {})", if request.trim().is_empty() { "listen only" } else { request.trim() });
    Ok(tauri::async_runtime::spawn_blocking(move || openrzr::wireless::gatt_exchange(&request, listen_ms)).await.unwrap_or_default())
}

/// Drivers serving Razer devices (Diagnostics): standard HID, or Razer packages / filters left by Synapse.
#[tauri::command]
async fn razer_drivers() -> R<Vec<openrzr::drivers::DriverInfo>> {
    let v = openrzr::drivers::list();
    for d in &v {
        log_info!("ui", "driver: {} — {} {} {} [{}]{}", d.device, d.provider, d.version, d.service, d.filters, if d.razer { " RAZER" } else { "" });
    }
    Ok(v)
}

/// First id of the service macros open-rzr writes itself (turbo for media keys).
const SERVICE_MACRO: u16 = 0xF000;

/// Turbo for a media key: the mouse has no turbo for them, so a small macro "press, release,
/// pause" is played while the button is held. Reuses an identical one already in the mouse.
#[tauri::command]
async fn turbo_media(st: State<'_, AppState>, code: u16, interval: u16) -> R<u16> {
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
async fn profile_links(id: String, links: Vec<String>) -> R<()> {
    let mut p = library::load(&id)?;
    log_info!("ui", "\"{}\" linked programs: {links:?}", p.name);
    p.links = links;
    library::save(&p)
}

#[tauri::command]
async fn running_programs() -> R<Vec<foreground::Program>> {
    Ok(foreground::running_programs())
}

#[tauri::command]
async fn set_autoswitch(st: State<'_, AppState>, enabled: bool, fallback: String, target: u8) -> R<()> {
    log_info!("ui", "auto switch: {enabled}, fallback \"{fallback}\", target {target}");
    let mut s = lock(&st.store);
    s.autoswitch = store::AutoSwitch { enabled, fallback, target: target.min(5) };
    s.save();
    Ok(())
}

#[derive(Serialize, Clone)]
struct Switched {
    id: String,
    name: String,
    program: String,
}

/// Follow the program in front and switch to its linked profile (runs while open-rzr runs).
fn autoswitch_loop(app: AppHandle) {
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
            Err(e) => log_warn!("app", "auto switch failed: {e}"),
        }
    }
}

// ------------------------------------------------------------------ backup

fn backup_dir() -> std::path::PathBuf {
    logs::config_dir().join("backups")
}

#[derive(Serialize)]
struct BackupFile {
    name: String,
    path: String,
    size: u64,
}

#[tauri::command]
async fn backup(st: State<'_, AppState>) -> R<String> {
    log_info!("ui", "backup");
    let b: Backup = with_mouse(&st, "backup", |m| {
        let p = m.list_profiles();
        m.backup(&p)
    })?;
    std::fs::create_dir_all(backup_dir()).map_err(|e| e.to_string())?;
    let name = format!("naga-{}.json", logs::timestamp().replace([' ', ':'], "-").replace(".", "-"));
    let path = backup_dir().join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&b).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    log_info!("ui", "backup saved: {}", path.display());
    Ok(path.display().to_string())
}

#[tauri::command]
async fn list_backups() -> R<Vec<BackupFile>> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(backup_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "json").unwrap_or(false) {
                v.push(BackupFile {
                    name: e.file_name().to_string_lossy().to_string(),
                    path: p.display().to_string(),
                    size: e.metadata().map(|m| m.len()).unwrap_or(0),
                });
            }
        }
    }
    v.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(v)
}

#[tauri::command]
async fn restore(st: State<'_, AppState>, path: String) -> R<usize> {
    log_info!("ui", "restore from {path}");
    let s = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let b: Backup = serde_json::from_str(&s).map_err(|e| format!("{path}: {e}"))?;
    with_mouse(&st, "restore", |m| m.restore(&b))
}

#[tauri::command]
async fn open_backup_folder() -> R<()> {
    let d = backup_dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    open_path(&d)
}

fn open_path(p: &std::path::Path) -> R<()> {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(p).env_remove("LD_LIBRARY_PATH").spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ diagnostics

#[derive(Serialize)]
struct LogChunk {
    seq: u64,
    lines: Vec<String>,
}

#[tauri::command]
async fn logs_since(since: u64) -> R<LogChunk> {
    let (seq, lines) = logs::recent(since);
    Ok(LogChunk { seq, lines })
}

#[tauri::command]
async fn set_log_level(st: State<'_, AppState>, level: String) -> R<()> {
    let l = Level::parse(&level).ok_or("[invalid] level: DEBUG/INFO/WARN/ERROR")?;
    logs::set_level(l);
    log_info!("ui", "log level: {}", l.as_str());
    let mut s = lock(&st.store);
    s.log_level = l.as_str().into();
    s.save();
    Ok(())
}

#[tauri::command]
async fn open_log_folder() -> R<()> {
    logs::open_folder().map_err(|e| e.to_string())
}

/// UI-side events (clicks, page switches) go to the same log.
#[tauri::command]
async fn ui_log(level: String, msg: String) -> R<()> {
    match Level::parse(&level).unwrap_or(Level::Info) {
        Level::Error => log_error!("ui", "{msg}"),
        Level::Warn => log_warn!("ui", "{msg}"),
        Level::Debug => log_debug!("ui", "{msg}"),
        Level::Info => log_info!("ui", "{msg}"),
    }
    Ok(())
}

#[tauri::command]
async fn save_report(st: State<'_, AppState>) -> R<String> {
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
async fn raw_command(st: State<'_, AppState>, cmd: String, args: String, size: Option<u8>) -> R<String> {
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

// ------------------------------------------------------------------ tables for the UI

#[derive(Serialize)]
struct Tables {
    kinds: Vec<(&'static str, &'static str)>,
    dpi_ops: Vec<(u8, &'static str)>,
    profile_ops: Vec<(u8, &'static str)>,
    system_ops: Vec<(u8, &'static str)>,
    macro_modes: Vec<(&'static str, &'static str)>,
    mouse_fn: Vec<(u8, &'static str)>,
    consumer: Vec<(u16, &'static str)>,
    keys: Vec<(u8, String)>,
    zones: Vec<(&'static str, &'static str)>,
    effects: Vec<(&'static str, &'static str)>,
    zone_effects: HashMap<&'static str, Vec<&'static str>>,
}

#[tauri::command]
async fn tables() -> R<Tables> {
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

fn main() {
    let store = Store::load();
    let level = Level::parse(&store.log_level).unwrap_or(Level::Debug);
    let console = std::env::args().any(|a| a == "-v");
    logs::init(level, console, true);
    logs::startup_banner("window");
    let mock = std::env::args().any(|a| a == "--mock" || a == "--mock-gui");
    if mock {
        log_info!("app", "emulator mode");
    }
    let lang = store.language.clone();
    compat::set_mode(compat::Mode::parse(&store.openrazer_mode).unwrap_or(compat::Mode::Auto));
    let state = AppState {
        tray: Mutex::new(None),
        mouse: Mutex::new(None),
        store: Mutex::new(store),
        serial: Mutex::new(String::new()),
        mock,
        hotplug: AtomicBool::new(false),
        temp_layer: Mutex::new(None),
        closing: Mutex::new(None),
        geometry: Mutex::new(None),
        ui_memory: Mutex::new(String::new()),
        actions: Arc::new(Mutex::new(HashMap::new())),
    };
    let minimized = std::env::args().any(|a| a == autostart::MINIMIZED_ARG);
    let result = tauri::Builder::default()
        // a second launch (e.g. autostart + manual start) just shows the running window
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .manage(state)
        .setup(move |app| {
            {
                // program actions of macros: listen for their F13–F24 trigger keys
                let st = app.state::<AppState>();
                refresh_program_table(&st);
                program_actions::start(st.actions.clone());
            }
            let (show_t, quit_t) = tray_labels(&lang);
            let show = MenuItem::with_id(app, "show", show_t, true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", quit_t, true, None::<&str>)?;
            let poly = if compat::polychromatic_path().is_some() {
                Some(MenuItem::with_id(app, "polychromatic", poly_label(&lang), true, None::<&str>)?)
            } else {
                None
            };
            let menu = match &poly {
                Some(pm) => Menu::with_items(app, &[&show, pm, &quit])?,
                None => Menu::with_items(app, &[&show, &quit])?,
            };
            let mut tb = TrayIconBuilder::with_id("main")
                .tooltip("open-rzr")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "polychromatic" => {
                        if let Err(e) = compat::open_polychromatic() {
                            log_warn!("app", "Polychromatic: {e}");
                        }
                    }
                    "quit" => {
                        log_info!("app", "quit from tray");
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tb = tb.icon(icon.clone());
            }
            let icon = tb.build(app)?;
            // plug / unplug: tell the window at once instead of polling
            let handle = app.handle().clone();
            if let Some(w) = hotplug::watch(move |c| {
                let _ = handle.emit("device-change", c.code());
            }) {
                w.keep();
                app.state::<AppState>().hotplug.store(true, Ordering::Relaxed);
            }
            let handle = app.handle().clone();
            std::thread::Builder::new().name("auto-switch".into()).spawn(move || autoswitch_loop(handle))?;
            *lock(&app.state::<AppState>().tray) = Some(TrayHandles { icon, show, quit, poly });
            if minimized {
                log_info!("app", "started minimized to tray");
            } else {
                create_main(app.handle())?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let st = window.state::<AppState>();
                if lock(&st.store).close_to_tray() {
                    api.prevent_close();
                    // ask the page first (unsaved macro); a second press while it does not answer closes anyway
                    let asked = *lock(&st.closing);
                    match asked {
                        Some(t) if t.elapsed() > std::time::Duration::from_millis(1500) => close_to_tray(window),
                        Some(_) => {}
                        None => {
                            *lock(&st.closing) = Some(std::time::Instant::now());
                            let _ = window.app_handle().emit("close-requested", ());
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info, list_devices, connect, battery, install_udev,
            set_dpi_stages, get_dpi_stages, set_polling, set_power,
            scan_buttons, button_names, side_plate, set_plate_choice, set_button, rename_button, copy_profile, describe_action,
            macros_on_mouse, write_macro, delete_macro, macro_library, save_macro_lib,
            delete_macro_lib, describe_steps, check_macro, rename_macro_lib, test_program_actions,
            program_actions_status,
            get_lighting, set_lighting,
            backup, list_backups, restore, open_backup_folder,
            logs_since, set_log_level, open_log_folder, ui_log, save_report, raw_command,
            tables, get_settings, set_language, set_autostart, set_close_to_tray, default_action, quit_app,
            openrazer_status, set_openrazer_mode, stop_openrazer_daemon, supported_devices,
            razer_software, stop_razer_software, start_razer_software,
            profiles, profile_get, profile_select, profile_set_button, profile_layer, profile_write, set_autowrite, all_hid_devices, razer_drivers, wireless_debug, wireless_gatt, turbo_media, profile_set_dpi, profile_create,
            profile_rename, profile_duplicate, profile_delete, profile_reset, profile_reload,
            profile_export, profile_import, open_folder_of, slot_assign, profile_links,
            running_programs, set_autoswitch, set_ui_style, open_polychromatic,
            hide_to_tray, close_cancelled, ui_state_get, ui_state_set
        ])
        .build(tauri::generate_context!())
        .map(|app| {
            app.run(|handle, event| {
                // the window closed to the tray is destroyed: keep running without windows
                if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                    if lock(&handle.state::<AppState>().store).close_to_tray() {
                        api.prevent_exit();
                    }
                }
            })
        });
    if let Err(e) = result {
        log_error!("app", "window failed to start: {e}");
        eprintln!("open-rzr: {e}");
        std::process::exit(1);
    }
    log_info!("app", "exit");
}
