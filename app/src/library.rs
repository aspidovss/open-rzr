//! Profile library on this computer: one JSON file per profile in <config>/profiles/.
//! Export writes the same JSON to the Downloads folder; import reads it back.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use openrzr::logs;
use openrzr::profile::{self, Profile};

pub fn dir() -> PathBuf {
    logs::config_dir().join("profiles")
}

fn path(id: &str) -> PathBuf {
    dir().join(format!("{id}.json"))
}

/// One writer at a time; each write gets its own temporary file.
static WRITE_LOCK: Mutex<()> = Mutex::new(());
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Write a file so that it is never left empty or half-written: a unique temporary file is
/// written and flushed to disk, then renamed over the old one. With `backup`, the previous
/// version (when it is valid JSON) is kept as `<name>.bak`.
pub fn write_safely(path: &Path, data: &[u8], backup: bool) -> std::io::Result<()> {
    let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension(format!("tmp{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        if backup {
            let valid = std::fs::read(path).ok().is_some_and(|old| serde_json::from_slice::<serde_json::Value>(&old).is_ok());
            if valid {
                let _ = std::fs::copy(path, path.with_extension("bak"));
            }
        }
        std::fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// A damaged profile file is replaced by its backup, if there is a good one.
fn from_backup(p: &Path) -> Option<Profile> {
    let bak = p.with_extension("bak");
    let pr: Profile = serde_json::from_str(&std::fs::read_to_string(&bak).ok()?).ok()?;
    let text = serde_json::to_string_pretty(&pr).ok()?;
    match write_safely(p, text.as_bytes(), false) {
        Ok(()) => openrzr::log_warn!("app", "profile {} was damaged and is restored from {}", p.display(), bak.display()),
        Err(e) => openrzr::log_warn!("app", "profile {} was damaged; using {} ({e})", p.display(), bak.display()),
    }
    Some(pr)
}

pub fn list() -> Vec<Profile> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x != "json").unwrap_or(true) {
                continue;
            }
            match std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<Profile>(&s).map_err(|e| e.to_string()))
            {
                Ok(mut pr) => {
                    // the file name is the id
                    pr.id = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    v.push(pr);
                }
                Err(err) => match from_backup(&p) {
                    Some(mut pr) => {
                        pr.id = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        v.push(pr);
                    }
                    None => openrzr::log_warn!("app", "profile {} skipped: {err}", p.display()),
                },
            }
        }
    }
    v.sort_by_key(|p| p.name.to_lowercase());
    v
}

pub fn load(id: &str) -> Result<Profile, String> {
    let s = std::fs::read_to_string(path(id)).map_err(|e| format!("[not_found] profile {id}: {e}"))?;
    let mut p: Profile = match serde_json::from_str(&s) {
        Ok(p) => p,
        Err(e) => from_backup(&path(id)).ok_or_else(|| format!("[invalid] profile {id}: {e}"))?,
    };
    p.id = id.to_string();
    Ok(p)
}

pub fn save(p: &Profile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    write_safely(&path(&p.id), text.as_bytes(), true).map_err(|e| format!("[io] {e}"))
}

pub fn delete(id: &str) -> Result<(), String> {
    let _ = std::fs::remove_file(path(id).with_extension("bak"));
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
