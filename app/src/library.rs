//! Profile library on this computer: one JSON file per profile in <config>/profiles/.
//! Export writes the same JSON to the Downloads folder; import reads it back.

use std::path::PathBuf;

use openrzr::logs;
use openrzr::profile::{self, Profile};

pub fn dir() -> PathBuf {
    logs::config_dir().join("profiles")
}

fn path(id: &str) -> PathBuf {
    dir().join(format!("{id}.json"))
}

pub fn list() -> Vec<Profile> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x != "json").unwrap_or(true) {
                continue;
            }
            match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| {
                serde_json::from_str::<Profile>(&s).map_err(|e| e.to_string())
            }) {
                Ok(mut pr) => {
                    // the file name is the id
                    pr.id = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    v.push(pr);
                }
                Err(err) => openrzr::log_warn!("app", "profile {} skipped: {err}", p.display()),
            }
        }
    }
    v.sort_by_key(|p| p.name.to_lowercase());
    v
}

pub fn load(id: &str) -> Result<Profile, String> {
    let s = std::fs::read_to_string(path(id)).map_err(|e| format!("[not_found] profile {id}: {e}"))?;
    let mut p: Profile = serde_json::from_str(&s).map_err(|e| format!("[invalid] profile {id}: {e}"))?;
    p.id = id.to_string();
    Ok(p)
}

pub fn save(p: &Profile) -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|e| format!("[io] {e}"))?;
    let tmp = path(&p.id).with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(p).map_err(|e| e.to_string())?).map_err(|e| format!("[io] {e}"))?;
    std::fs::rename(&tmp, path(&p.id)).map_err(|e| format!("[io] {e}"))
}

pub fn delete(id: &str) -> Result<(), String> {
    std::fs::remove_file(path(id)).map_err(|e| format!("[io] {e}"))
}

/// Free id for a new profile based on its name.
pub fn new_id(name: &str) -> String {
    let base = profile::slug(name);
    let mut id = base.clone();
    let mut n = 2;
    while path(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// Free name: "Name", "Name (2)", ...
pub fn free_name(base: &str) -> String {
    let names: Vec<String> = list().into_iter().map(|p| p.name.to_lowercase()).collect();
    if !names.contains(&base.to_lowercase()) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|c| !names.contains(&c.to_lowercase()))
        .unwrap_or_else(|| base.to_string())
}

/// Store a new profile under a fresh id and a free name.
pub fn add(mut p: Profile) -> Result<Profile, String> {
    p.name = free_name(if p.name.trim().is_empty() { "Profile" } else { p.name.trim() });
    p.id = new_id(&p.name);
    p.format = profile::FORMAT.into();
    p.version = profile::VERSION;
    save(&p)?;
    openrzr::log_info!("app", "profile added: \"{}\" ({})", p.name, p.id);
    Ok(p)
}

fn downloads() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    match home {
        Some(h) if h.join("Downloads").is_dir() => h.join("Downloads"),
        _ => logs::config_dir().join("exports"),
    }
}

pub fn export(p: &Profile) -> Result<PathBuf, String> {
    let d = downloads();
    std::fs::create_dir_all(&d).map_err(|e| format!("[io] {e}"))?;
    let mut f = d.join(format!("{}.openrzr.json", profile::slug(&p.name)));
    let mut n = 2;
    while f.exists() {
        f = d.join(format!("{}-{n}.openrzr.json", profile::slug(&p.name)));
        n += 1;
    }
    std::fs::write(&f, serde_json::to_string_pretty(p).map_err(|e| e.to_string())?).map_err(|e| format!("[io] {e}"))?;
    openrzr::log_info!("app", "profile exported: \"{}\" -> {}", p.name, f.display());
    Ok(f)
}

pub fn import(text: &str) -> Result<Profile, String> {
    let p: Profile = serde_json::from_str(text).map_err(|e| format!("[invalid] {e}"))?;
    p.validate().map_err(|e| format!("[invalid] {e}"))?;
    add(p)
}
