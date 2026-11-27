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
    /// update check on GitHub: null = not asked yet
    pub(crate) updates: Option<bool>,
    pub(crate) update_skip: String,
    /// also optional updates (fourth number of the version)
    pub(crate) updates_optional: bool,
    pub(crate) side_list: bool,
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
        updates: s.updates,
        update_skip: s.update_skip.clone(),
        updates_optional: s.updates_optional,
        side_list: s.side_list,
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

// ------------------------------------------------------------------ user agreement, update check

/// Edition of `ui/legal/eula.*.txt`. A new edition is shown again to everyone.
pub(crate) const AGREEMENT_EDITION: u32 = 2;

#[derive(Serialize)]
pub(crate) struct FirstRun {
    pub(crate) agreement_edition: u32,
    pub(crate) agreement_accepted: bool,
    /// null = the update question was not asked yet
    pub(crate) updates: Option<bool>,
}

/// The installer shows the agreement on its licence page and, when it is accepted, leaves
/// `<config dir>/agreement.txt` with the edition number: then the window does not ask again.
fn agreement_from_installer() -> u32 {
    let p = logs::config_dir().join("agreement.txt");
    std::fs::read_to_string(p).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

#[tauri::command]
pub(crate) async fn first_run(st: State<'_, AppState>) -> R<FirstRun> {
    let mut s = lock(&st.store);
    if s.agreement < AGREEMENT_EDITION {
        let inst = agreement_from_installer();
        if inst >= AGREEMENT_EDITION {
            log_info!("app", "user agreement edition {inst} accepted in the installer");
            s.agreement = inst;
            s.save();
        }
    }
    Ok(FirstRun { agreement_edition: AGREEMENT_EDITION, agreement_accepted: s.agreement >= AGREEMENT_EDITION, updates: s.updates })
}

#[tauri::command]
pub(crate) async fn agreement_accept(st: State<'_, AppState>) -> R<()> {
    log_info!("ui", "user agreement edition {AGREEMENT_EDITION} accepted in the window");
    let mut s = lock(&st.store);
    s.agreement = AGREEMENT_EDITION;
    s.save();
    Ok(())
}

/// "Do not accept": the program closes.
#[tauri::command]
pub(crate) async fn agreement_decline(app: AppHandle) -> R<()> {
    log_info!("ui", "user agreement declined: quitting");
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub(crate) async fn set_updates(st: State<'_, AppState>, enabled: bool) -> R<()> {
    log_info!("ui", "update check on GitHub: {}", if enabled { "on" } else { "off" });
    let mut s = lock(&st.store);
    s.updates = Some(enabled);
    s.save();
    Ok(())
}

/// Customize: the list of all buttons beside the picture (like Synapse) instead of in place of it.
#[tauri::command]
pub(crate) async fn set_side_list(st: State<'_, AppState>, enabled: bool) -> R<()> {
    log_info!("ui", "list of all buttons beside the picture: {}", if enabled { "on" } else { "off" });
    let mut s = lock(&st.store);
    s.side_list = enabled;
    s.save();
    Ok(())
}

/// Optional updates (only the fourth number of the version changes: intermediate, device support).
#[tauri::command]
pub(crate) async fn set_updates_optional(st: State<'_, AppState>, enabled: bool) -> R<()> {
    log_info!("ui", "optional updates: {}", if enabled { "on" } else { "off" });
    let mut s = lock(&st.store);
    s.updates_optional = enabled;
    s.save();
    Ok(())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Is an automatic check due (turned on, last one more than a day ago)?
#[tauri::command]
pub(crate) async fn update_due(st: State<'_, AppState>) -> R<bool> {
    let s = lock(&st.store);
    Ok(s.updates == Some(true) && now_secs().saturating_sub(s.update_checked) >= 24 * 3600)
}

/// The window checked GitHub (the request itself is made by the window: the only address it may
/// reach is api.github.com, see the CSP in tauri.conf.json). `result` goes to the log.
#[tauri::command]
pub(crate) async fn update_checked(st: State<'_, AppState>, result: String) -> R<()> {
    log_info!("ui", "update check: {result}");
    let mut s = lock(&st.store);
    s.update_checked = now_secs();
    s.save();
    Ok(())
}

/// "Don't tell me about this version again".
#[tauri::command]
pub(crate) async fn update_skip(st: State<'_, AppState>, version: String) -> R<()> {
    log_info!("ui", "update {version}: hidden");
    let mut s = lock(&st.store);
    s.update_skip = version;
    s.save();
    Ok(())
}

/// Open a page of the project on GitHub in the browser (only that site).
#[tauri::command]
pub(crate) async fn open_project_page(url: String) -> R<()> {
    if !url.starts_with("https://github.com/aspidovss/open-rzr") || url.contains(char::is_whitespace) || url.contains('"') {
        return Err("[invalid] only pages of github.com/aspidovss/open-rzr".into());
    }
    log_info!("ui", "open {url}");
    #[cfg(windows)]
    let r = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url]).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(&url).env_remove("LD_LIBRARY_PATH").spawn();
    r.map(|_| ()).map_err(|e| format!("[io] {e}"))
}

/// Download a published package without executing it. Only numeric release tags and the
/// platform's fixed artifact name can enter the URL or destination path.
#[tauri::command]
pub(crate) async fn download_update(app: tauri::AppHandle, version: String, size: u64) -> R<String> {
    use tauri::Manager;
    let components: Vec<_> = version.split('.').collect();
    if !(3..=4).contains(&components.len()) || !components.iter().all(|v| !v.is_empty() && v.len() <= 9 && v.bytes().all(|b| b.is_ascii_digit())) || size == 0 {
        return Err("Invalid release metadata".into());
    }
    let suffix = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64-setup.exe",
        ("linux", "x86_64") => "linux-x86_64.tar.gz",
        _ => return Err("No release package for this platform".into()),
    };
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || -> R<String> {
        let name = format!("open-rzr-{version}-{suffix}");
        let url = format!("https://github.com/aspidovss/open-rzr/releases/download/v{version}/{name}");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        let curl = std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or("Missing SystemRoot")?).join("System32/curl.exe");
        #[cfg(not(windows))]
        let curl = "curl";
        let mut cmd = std::process::Command::new(curl);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let result = cmd.args(["--disable", "--fail", "--location", "--silent", "--show-error", "--proto", "=https",
                "--proto-redir", "=https", "--connect-timeout", "20", "--max-time", "900", "--output"])
            .arg(tmp.path()).arg(url).output().map_err(|e| format!("curl: {e}"))?;
        if !result.status.success() {
            return Err(String::from_utf8_lossy(&result.stderr).trim().to_owned());
        }
        if tmp.as_file().metadata().map_err(|e| e.to_string())?.len() != size {
            return Err("Incomplete download".into());
        }
        let mut temp = tmp;
        for copy in 1.. {
            let path = dir.join(if copy == 1 { name.clone() } else { format!("open-rzr-{version}-{copy}-{suffix}") });
            match temp.persist_noclobber(&path) {
                Ok(_) => return Ok(path.display().to_string()),
                Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => temp = e.file,
                Err(e) => return Err(e.error.to_string()),
            }
        }
        unreachable!()
    }).await.map_err(|e| e.to_string())?
}
