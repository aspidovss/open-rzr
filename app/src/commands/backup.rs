//! Commands: backup.

use crate::*;

pub(crate) fn backup_dir() -> std::path::PathBuf {
    logs::config_dir().join("backups")
}

#[derive(Serialize)]
pub(crate) struct BackupFile {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) size: u64,
}

#[tauri::command]
pub(crate) async fn backup(st: State<'_, AppState>) -> R<String> {
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
pub(crate) async fn list_backups() -> R<Vec<BackupFile>> {
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
pub(crate) async fn restore(st: State<'_, AppState>, path: String) -> R<usize> {
    log_info!("ui", "restore from {path}");
    let s = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let b: Backup = serde_json::from_str(&s).map_err(|e| format!("{path}: {e}"))?;
    with_mouse(&st, "restore", |m| m.restore(&b))
}

#[tauri::command]
pub(crate) async fn open_backup_folder() -> R<()> {
    let d = backup_dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    open_path(&d)
}

pub(crate) fn open_path(p: &std::path::Path) -> R<()> {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(p).env_remove("LD_LIBRARY_PATH").spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}
