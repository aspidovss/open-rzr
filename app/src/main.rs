//! open-rzr — Tauri shell around the openrzr core. All device I/O happens here, in Rust;
//! the web UI only calls these commands. Nothing leaves the computer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod autostart;
mod commands;
mod library;
mod store;

#[allow(unused_imports)]
use commands::{device::*, settings::*, openrazer_cmds::*, razer::*, performance::*, buttons::*, macro_cmds::*, lighting_cmds::*, profiles::*, backup::*, diagnostics::*, tables::*};

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
    /// One profile operation at a time (edits from the window, the automatic switch, writes):
    /// read profile file → change → write to the mouse → save, without another one in between.
    /// Lock order: `profile_op` before `mouse`.
    profile_op: Mutex<()>,
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
    // USB requests block (up to the 2 s deadline of a sleeping or busy mouse): tell the async
    // runtime, so the other commands of the window keep running on other threads.
    // Outside the runtime (the automatic switch thread) this simply runs the closure.
    tokio::task::block_in_place(|| with_mouse_blocking(st, what, f))
}

fn with_mouse_blocking<T>(st: &AppState, what: &str, f: impl FnOnce(&mut RazerMouse) -> openrzr::Result<T>) -> R<T> {
    let mut g = lock(&st.mouse);
    let m = g.as_mut().ok_or_else(|| "[disconnected] Mouse is not connected".to_string())?;
    match f(m) {
        Ok(v) => Ok(v),
        Err(e) => {
            if e.code() == "asleep" {
                // a sleeping wireless mouse is expected; the channel already logged it once
                log_debug!("ui", "{what}: mouse asleep");
                return Err(err(&e));
            }
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

fn main() {
    let store = Store::load();
    let level = Level::parse(&store.log_level).unwrap_or(Level::Debug);
    let console = std::env::args().any(|a| a == "-v");
    logs::init(level, console, true);
    logs::startup_banner("window");
    openrzr::caps::load_device_files();
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
        profile_op: Mutex::new(()),
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
            commands::device::app_info, commands::device::list_devices, commands::device::connect, commands::device::battery, commands::razer::install_udev,
            commands::performance::set_dpi_stages, commands::performance::get_dpi_stages, commands::performance::set_polling, commands::performance::set_power,
            commands::buttons::scan_buttons, commands::buttons::button_names, commands::buttons::side_plate, commands::buttons::set_plate_choice, commands::buttons::set_button, commands::buttons::rename_button, commands::buttons::copy_profile, commands::buttons::describe_action,
            commands::macro_cmds::macros_on_mouse, commands::macro_cmds::write_macro, commands::macro_cmds::delete_macro, commands::macro_cmds::macro_library, commands::macro_cmds::save_macro_lib,
            commands::macro_cmds::delete_macro_lib, commands::macro_cmds::describe_steps, commands::macro_cmds::check_macro, commands::macro_cmds::rename_macro_lib, commands::macro_cmds::test_program_actions,
            commands::macro_cmds::program_actions_status,
            commands::lighting_cmds::get_lighting, commands::lighting_cmds::set_lighting,
            commands::backup::backup, commands::backup::list_backups, commands::backup::restore, commands::backup::open_backup_folder,
            commands::diagnostics::logs_since, commands::diagnostics::set_log_level, commands::diagnostics::open_log_folder, commands::diagnostics::ui_log, commands::diagnostics::save_report, commands::diagnostics::raw_command,
            commands::tables::tables, commands::settings::get_settings, commands::settings::set_language, commands::settings::set_autostart, commands::settings::set_close_to_tray, commands::settings::default_action, commands::razer::quit_app,
            commands::openrazer_cmds::openrazer_status, commands::openrazer_cmds::set_openrazer_mode, commands::openrazer_cmds::stop_openrazer_daemon, commands::razer::supported_devices,
            commands::razer::razer_software, commands::razer::stop_razer_software, commands::razer::start_razer_software,
            commands::profiles::profiles, commands::profiles::profile_get, commands::profiles::profile_select, commands::profiles::profile_set_button, commands::profiles::profile_layer, commands::profiles::profile_write, commands::profiles::set_autowrite, commands::diagnostics::all_hid_devices, commands::diagnostics::razer_drivers, commands::diagnostics::wireless_debug, commands::diagnostics::wireless_gatt, commands::diagnostics::device_files, commands::profiles::turbo_media, commands::profiles::profile_set_dpi, commands::profiles::profile_create,
            commands::profiles::profile_rename, commands::profiles::profile_duplicate, commands::profiles::profile_delete, commands::profiles::profile_reset, commands::profiles::profile_reload,
            commands::profiles::profile_export, commands::profiles::profile_import, commands::profiles::open_folder_of, commands::profiles::slot_assign, commands::profiles::profile_links,
            commands::profiles::running_programs, commands::profiles::set_autoswitch, commands::settings::set_ui_style, commands::openrazer_cmds::open_polychromatic,
            hide_to_tray, close_cancelled, ui_state_get, ui_state_set, commands::profiles::profile_restore
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
