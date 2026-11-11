//! Razer software on Windows (Synapse, Razer Central, Chroma SDK services).
//!
//! These programs talk to the mouse at the same time as open-rzr; both then receive each other's
//! replies. This module finds them, stops them (services need administrator rights: Windows asks
//! through UAC) and can start them again, restoring how they started with Windows.
//!
//! Only standard Windows tools are used: `sc` (services), `tasklist` / `taskkill` (programs),
//! PowerShell `Start-Process -Verb RunAs` for the elevation prompt. The parsers are plain functions
//! so they are tested on every OS.

use crate::protocol::{Error, Result};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct Service {
    /// service name for `sc`
    pub name: String,
    pub display: String,
    pub running: bool,
    /// how it starts with Windows: "auto" | "delayed-auto" | "demand" | "disabled" | ""
    pub start: String,
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct RazerSoftware {
    /// the check exists only on Windows
    pub supported_os: bool,
    pub services: Vec<Service>,
    /// running programs, e.g. "RazerAppEngine.exe"
    pub processes: Vec<String>,
}

impl RazerSoftware {
    /// Something of Razer is running and may talk to the mouse.
    pub fn active(&self) -> bool {
        !self.processes.is_empty() || self.services.iter().any(|s| s.running)
    }
}

/// What was changed by `stop`, so `start_again` can undo it: (service, original start mode).
pub type Saved = Vec<(String, String)>;

pub fn is_razer_service(name: &str, display: &str) -> bool {
    let (n, d) = (name.to_ascii_lowercase(), display.to_ascii_lowercase());
    n.contains("razer") || d.contains("razer") || name.starts_with("Rz") || n.starts_with("cortex")
}

pub fn is_razer_process(image: &str) -> bool {
    let l = image.to_ascii_lowercase();
    (l.starts_with("razer") || image.starts_with("Rz")) && l.ends_with(".exe")
}

/// `sc query type= service state= all` -> (name, display, running). Keys are not localised.
pub fn parse_sc_query(text: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, String, bool)> = None;
    for line in text.lines() {
        let t = line.trim();
        let Some((k, v)) = t.split_once(':') else { continue };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "SERVICE_NAME" => {
                if let Some(c) = cur.take() {
                    out.push(c);
                }
                cur = Some((v.to_string(), String::new(), false));
            }
            "DISPLAY_NAME" => {
                if let Some(c) = cur.as_mut() {
                    c.1 = v.to_string();
                }
            }
            // "STATE              : 4  RUNNING"
            "STATE" => {
                if let Some(c) = cur.as_mut() {
                    c.2 = v.split_whitespace().next() == Some("4");
                }
            }
            _ => {}
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}

/// `sc qc NAME` -> start mode token for `sc config start=`.
pub fn parse_sc_qc(text: &str) -> String {
    for line in text.lines() {
        let t = line.trim();
        if let Some((k, v)) = t.split_once(':') {
            if k.trim() == "START_TYPE" {
                let delayed = v.contains("DELAYED");
                return match v.split_whitespace().next() {
                    Some("2") if delayed => "delayed-auto",
                    Some("2") => "auto",
                    Some("3") => "demand",
                    Some("4") => "disabled",
                    _ => "",
                }
                .to_string();
            }
        }
    }
    String::new()
}

/// `tasklist /FO CSV /NH` -> Razer image names (first CSV field), without duplicates.
pub fn parse_tasklist(text: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for line in text.lines() {
        let image = line.trim().trim_start_matches('"').split("\",").next().unwrap_or("").trim_matches('"');
        if is_razer_process(image) && !v.iter().any(|x| x.eq_ignore_ascii_case(image)) {
            v.push(image.to_string());
        }
    }
    v
}

/// Batch script that stops services (optionally keeps them from starting with Windows) and programs.
pub fn stop_script(sw: &RazerSoftware, disable_autostart: bool) -> String {
    let mut s = String::from("@echo off\r\n");
    for svc in &sw.services {
        if disable_autostart && svc.start != "demand" && svc.start != "disabled" && !svc.start.is_empty() {
            s += &format!("sc config \"{}\" start= demand >nul 2>&1\r\n", svc.name);
        }
        if svc.running {
            s += &format!("sc stop \"{}\" >nul 2>&1\r\n", svc.name);
        }
    }
    // give the services a moment, then close the programs they may have left behind
    s += "ping -n 3 127.0.0.1 >nul\r\n";
    for p in &sw.processes {
        s += &format!("taskkill /F /IM \"{p}\" >nul 2>&1\r\n");
    }
    s += "exit /b 0\r\n";
    s
}

/// Batch script that restores start modes and starts the services again.
pub fn start_script(saved: &Saved) -> String {
    let mut s = String::from("@echo off\r\n");
    for (name, start) in saved {
        if !start.is_empty() {
            s += &format!("sc config \"{name}\" start= {start} >nul 2>&1\r\n");
        }
        if start != "disabled" {
            s += &format!("sc start \"{name}\" >nul 2>&1\r\n");
        }
    }
    s += "exit /b 0\r\n";
    s
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
fn run(cmd: &str, args: &[&str]) -> Result<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new(cmd)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| Error::Io(format!("{cmd}: {e}")))?;
    // console tools print in the OEM code page; the parsed keys are ASCII
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(windows)]
pub fn scan() -> RazerSoftware {
    let mut sw = RazerSoftware { supported_os: true, ..Default::default() };
    if let Ok(t) = run("sc", &["query", "type=", "service", "state=", "all"]) {
        for (name, display, running) in parse_sc_query(&t) {
            if is_razer_service(&name, &display) {
                let start = run("sc", &["qc", &name]).map(|q| parse_sc_qc(&q)).unwrap_or_default();
                sw.services.push(Service { name, display, running, start });
            }
        }
    }
    if let Ok(t) = run("tasklist", &["/FO", "CSV", "/NH"]) {
        sw.processes = parse_tasklist(&t);
    }
    // checked every few seconds while the window is open: log only what changed
    let line = format!(
        "Razer software: services {:?}, programs {:?}",
        sw.services.iter().map(|s| format!("{}{}", s.name, if s.running { " (running)" } else { "" })).collect::<Vec<_>>(),
        sw.processes
    );
    static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if *last != line {
        log_debug!("app", "{line}");
        *last = line;
    }
    sw
}

/// Run a batch script as administrator (UAC prompt) and wait for it.
#[cfg(windows)]
fn run_elevated(script: &str) -> Result<()> {
    let path = std::env::temp_dir().join(format!("open-rzr-razer-{}.cmd", std::process::id()));
    std::fs::write(&path, script).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
    let p = path.display().to_string().replace('\'', "''");
    let ps = format!(
        "try {{ $p = Start-Process -FilePath $env:ComSpec -ArgumentList '/c \"{p}\"' -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}"
    );
    let r = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &ps])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
    };
    let _ = std::fs::remove_file(&path);
    match r {
        Ok(s) if s.success() => Ok(()),
        // 1223 = ERROR_CANCELLED: the user declined the UAC prompt
        Ok(s) if s.code() == Some(1223) => Err(Error::NoAccess("Administrator rights were not granted".into())),
        Ok(s) => Err(Error::Io(format!("script exited with {s}"))),
        Err(e) => Err(Error::Io(format!("powershell: {e}"))),
    }
}

/// Stop Razer services and programs. Returns what to restore later.
#[cfg(windows)]
pub fn stop(disable_autostart: bool) -> Result<Saved> {
    let sw = scan();
    if !sw.active() && !disable_autostart {
        return Ok(Vec::new());
    }
    let script = stop_script(&sw, disable_autostart);
    let need_admin = sw.services.iter().any(|s| s.running) || disable_autostart;
    log_info!("app", "stopping Razer software (admin: {need_admin}, keep from autostart: {disable_autostart})");
    if need_admin {
        run_elevated(&script)?;
    } else {
        for p in &sw.processes {
            let _ = run("taskkill", &["/F", "/IM", p]);
        }
    }
    let saved: Saved = sw
        .services
        .iter()
        .filter(|s| s.running || (disable_autostart && s.start != "demand" && s.start != "disabled"))
        .map(|s| (s.name.clone(), if disable_autostart { s.start.clone() } else { String::new() }))
        .collect();
    let after = scan();
    if after.active() {
        log_warn!("app", "Razer software still running after stop: {:?}", after.processes);
    }
    Ok(saved)
}

#[cfg(windows)]
pub fn start_again(saved: &Saved) -> Result<()> {
    if saved.is_empty() {
        return Ok(());
    }
    log_info!("app", "starting Razer services again: {saved:?}");
    run_elevated(&start_script(saved))
}

// ------------------------------------------------------------------ other systems

#[cfg(not(windows))]
pub fn scan() -> RazerSoftware {
    RazerSoftware::default()
}

#[cfg(not(windows))]
pub fn stop(_disable_autostart: bool) -> Result<Saved> {
    Err(Error::Unsupported("Razer Synapse exists only on Windows".into()))
}

#[cfg(not(windows))]
pub fn start_again(_saved: &Saved) -> Result<()> {
    Err(Error::Unsupported("Razer Synapse exists only on Windows".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SC_QUERY: &str = "
SERVICE_NAME: Razer Synapse Service
DISPLAY_NAME: Razer Synapse Service
        TYPE               : 10  WIN32_OWN_PROCESS
        STATE              : 4  RUNNING
                                (STOPPABLE, NOT_PAUSABLE, IGNORES_SHUTDOWN)
        WIN32_EXIT_CODE    : 0  (0x0)

SERVICE_NAME: RzActionSvc
DISPLAY_NAME: Razer Chroma SDK Service
        TYPE               : 10  WIN32_OWN_PROCESS
        STATE              : 1  STOPPED

SERVICE_NAME: Spooler
DISPLAY_NAME: Диспетчер печати
        STATE              : 4  RUNNING
";

    #[test]
    fn services() {
        let v = parse_sc_query(SC_QUERY);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], ("Razer Synapse Service".into(), "Razer Synapse Service".into(), true));
        assert!(!v[1].2);
        let razer: Vec<_> = v.iter().filter(|(n, d, _)| is_razer_service(n, d)).collect();
        assert_eq!(razer.len(), 2);
        assert_eq!(parse_sc_qc("        START_TYPE         : 2   AUTO_START  (DELAYED)\n"), "delayed-auto");
        assert_eq!(parse_sc_qc("        START_TYPE         : 2   AUTO_START\n"), "auto");
        assert_eq!(parse_sc_qc("        START_TYPE         : 3   DEMAND_START\n"), "demand");
    }

    #[test]
    fn processes_and_scripts() {
        let t = "\"System\",\"4\",\"Services\",\"0\",\"144 K\"\r\n\"RazerAppEngine.exe\",\"8812\",\"Console\",\"1\",\"212 400 K\"\r\n\
                 \"RazerAppEngine.exe\",\"8900\",\"Console\",\"1\",\"80 K\"\r\n\"RzSDKService.exe\",\"77\",\"Services\",\"0\",\"9 K\"\r\n\
                 \"open-rzr.exe\",\"1\",\"Console\",\"1\",\"9 K\"\r\n";
        assert_eq!(parse_tasklist(t), vec!["RazerAppEngine.exe".to_string(), "RzSDKService.exe".to_string()]);
        let sw = RazerSoftware {
            supported_os: true,
            services: vec![
                Service { name: "Razer Synapse Service".into(), display: String::new(), running: true, start: "auto".into() },
                Service { name: "RzActionSvc".into(), display: String::new(), running: false, start: "demand".into() },
            ],
            processes: vec!["RazerAppEngine.exe".into()],
        };
        assert!(sw.active());
        let s = stop_script(&sw, true);
        assert!(s.contains("sc config \"Razer Synapse Service\" start= demand"));
        assert!(s.contains("sc stop \"Razer Synapse Service\""));
        assert!(!s.contains("sc stop \"RzActionSvc\""));
        assert!(!s.contains("sc config \"RzActionSvc\""));
        assert!(s.contains("taskkill /F /IM \"RazerAppEngine.exe\""));
        let back = start_script(&vec![("Razer Synapse Service".into(), "delayed-auto".into())]);
        assert!(back.contains("start= delayed-auto") && back.contains("sc start \"Razer Synapse Service\""));
    }
}
