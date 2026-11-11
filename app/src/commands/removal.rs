//! "Remove open-rzr from this computer" (Settings): what the installer's uninstaller does, for
//! the portable version, the AppImage and builds from source — and the way to the uninstaller
//! when the program was installed with it.
//!
//! Order: start the Razer services open-rzr stopped (Windows, asks for administrator rights) →
//! autostart off → udev rule written by the program removed (Linux, asks for the password) →
//! the window says what is left → on quit, the data folders are deleted by a small shell that
//! waits until the program has exited (files the running program keeps open could not be
//! deleted on Windows, and a last log line would create the folder again).
//! The program files themselves are not deleted: the user does that (or the package manager).

use crate::*;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub(crate) struct RemovalInfo {
    /// Windows: the uninstaller of the installed program (uninstall.exe next to open-rzr.exe)
    uninstaller: Option<String>,
    /// Linux: installed by the package manager (.deb) — program files are removed with it
    package: bool,
    /// the program file or folder the user deletes afterwards
    program: String,
    autostart: bool,
    /// Linux: /etc/udev/rules.d/99-open-rzr.rules written by the program
    udev_rule: bool,
    /// Windows: Razer services open-rzr stopped (they would stay stopped)
    razer_stopped: bool,
    /// settings, profile library, backups, logs
    data_dir: String,
    /// every folder deleted with "delete my data"
    data_dirs: Vec<String>,
}

fn exe_path() -> PathBuf {
    if let Some(a) = std::env::var_os("APPIMAGE") {
        return PathBuf::from(a);
    }
    std::env::current_exe().unwrap_or_default()
}

fn uninstaller() -> Option<PathBuf> {
    if !cfg!(windows) || std::env::var_os("APPIMAGE").is_some() {
        return None;
    }
    let u = std::env::current_exe().ok()?.parent()?.join("uninstall.exe");
    u.is_file().then_some(u)
}

fn packaged() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() && exe_path().starts_with("/usr")
}

/// The program file of an AppImage, else the folder of the executable.
fn program_location() -> PathBuf {
    let exe = exe_path();
    if std::env::var_os("APPIMAGE").is_some() {
        exe
    } else {
        exe.parent().map(Path::to_path_buf).unwrap_or(exe)
    }
}

/// Our config folder and the web view's folders (named after the app identifier). Anything
/// else is never deleted, whatever the environment variables say.
fn data_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let id = app.config().identifier.clone();
    let mut v = vec![logs::config_dir()];
    let p = app.path();
    for d in [p.app_data_dir(), p.app_local_data_dir(), p.app_cache_dir(), p.app_config_dir()].into_iter().flatten() {
        if !v.contains(&d) {
            v.push(d);
        }
    }
    v.retain(|d| {
        let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
        (name == "open-rzr" || name == id) && d.components().count() > 3
    });
    v
}

#[tauri::command]
pub(crate) async fn removal_info(app: AppHandle, st: State<'_, AppState>) -> R<RemovalInfo> {
    Ok(RemovalInfo {
        uninstaller: uninstaller().map(|p| p.display().to_string()),
        package: packaged(),
        program: program_location().display().to_string(),
        autostart: autostart::get().enabled,
        udev_rule: cfg!(target_os = "linux") && udev::rule_present(),
        razer_stopped: !lock(&st.store).razer_saved.is_empty(),
        data_dir: logs::config_dir().display().to_string(),
        data_dirs: data_dirs(&app).iter().map(|d| d.display().to_string()).collect(),
    })
}

/// Start the installer's uninstaller and quit (it closes the program itself as well).
#[tauri::command]
pub(crate) async fn run_uninstaller(app: AppHandle) -> R<()> {
    let u = uninstaller().ok_or("[invalid] the program was not installed with the installer")?;
    log_info!("ui", "run the uninstaller {}", u.display());
    std::process::Command::new(&u).spawn().map_err(|e| format!("[io] {e}"))?;
    app.exit(0);
    Ok(())
}

/// Undo what the program changed on the computer. Stops at the first step that fails (for
/// example, the administrator prompt was cancelled), so it can simply be run again.
#[tauri::command]
pub(crate) async fn remove_from_computer(st: State<'_, AppState>, start_razer: bool) -> R<()> {
    log_info!("ui", "remove open-rzr from this computer (start Razer services again: {start_razer})");
    let saved = lock(&st.store).razer_saved.clone();
    if start_razer && !saved.is_empty() {
        tokio::task::block_in_place(|| synapse::start_again(&saved)).map_err(|e| err(&e))?;
        let mut s = lock(&st.store);
        s.razer_saved.clear();
        s.save();
    }
    autostart::set(false, false).map_err(|e| format!("[io] autostart: {e}"))?;
    if cfg!(target_os = "linux") && udev::rule_present() {
        tokio::task::block_in_place(|| udev::uninstall(true))?;
    }
    log_info!("app", "removal: done; program files are left at {}", program_location().display());
    Ok(())
}

/// Quit; with `delete_data`, the data folders are deleted right after the program has exited.
/// `open_folder` opens the program's folder so the user can delete the files.
#[tauri::command]
pub(crate) async fn remove_finish(app: AppHandle, delete_data: bool, open_folder: bool) -> R<()> {
    if open_folder && !packaged() {
        let loc = program_location();
        let dir = if loc.is_dir() { loc } else { loc.parent().map(Path::to_path_buf).unwrap_or(loc) };
        let _ = open_path(&dir);
    }
    if delete_data {
        let dirs = data_dirs(&app);
        log_info!("app", "removal: deleting {dirs:?} after exit");
        delete_after_exit(&dirs)?;
    }
    app.exit(0);
    Ok(())
}

#[cfg(windows)]
fn delete_after_exit(dirs: &[PathBuf]) -> R<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // the program exits right after this; wait ~3 s (ping is the sleep available everywhere),
    // then delete. Paths with characters cmd would read as syntax are skipped.
    let mut script = String::from("ping -n 4 127.0.0.1 >nul");
    for d in dirs {
        let d = d.display().to_string();
        if d.contains('"') || d.contains('%') || d.contains('&') {
            continue;
        }
        script.push_str(&format!(" & rmdir /s /q \"{d}\""));
    }
    std::process::Command::new("cmd")
        .raw_arg(format!("/c {script}"))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("[io] {e}"))
}

#[cfg(not(windows))]
fn delete_after_exit(dirs: &[PathBuf]) -> R<()> {
    // wait until this process is gone (at most ~20 s), then delete; paths are passed as
    // arguments, never pasted into the script
    let pid = std::process::id().to_string();
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c")
        .arg("p=$1; shift; i=0; while kill -0 \"$p\" 2>/dev/null && [ $i -lt 40 ]; do sleep 0.5; i=$((i+1)); done; rm -rf -- \"$@\"")
        .arg("sh")
        .arg(&pid);
    for d in dirs {
        cmd.arg(d);
    }
    cmd.env_remove("LD_LIBRARY_PATH").spawn().map(|_| ()).map_err(|e| format!("[io] {e}"))
}
