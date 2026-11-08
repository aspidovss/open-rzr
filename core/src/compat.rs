//! Compatibility with OpenRazer (https://openrazer.github.io) on Linux.
//!
//! OpenRazer consists of a kernel driver (`razermouse`) and a daemon (`openrazer-daemon`). When
//! both open-rzr and OpenRazer talk to the mouse at the same time, they receive each other's replies.
//! Modes:
//! * `auto` (default) — if the OpenRazer driver is bound to the mouse, everything it supports goes
//!   through its sysfs files (see transport/openrazer.rs); the rest goes over HID as usual;
//! * `on`   — the same, and a warning in the log when the driver is not found;
//! * `off`  — always talk to the mouse directly; stop the daemon to avoid conflicts.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::protocol::{Error, Result};
use crate::transport::{self, DeviceInfo, Transport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    On,
    Off,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "auto" | "" => Some(Mode::Auto),
            "on" | "openrazer" => Some(Mode::On),
            "off" | "direct" => Some(Mode::Off),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::On => "on",
            Mode::Off => "off",
        }
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);

pub fn set_mode(m: Mode) {
    MODE.store(
        match m {
            Mode::Auto => 0,
            Mode::On => 1,
            Mode::Off => 2,
        },
        Ordering::Relaxed,
    );
    log_info!("app", "OpenRazer compatibility: {}", m.code());
}

pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::On,
        2 => Mode::Off,
        _ => Mode::Auto,
    }
}

const DRIVER_DIR: &str = "/sys/bus/hid/drivers/razermouse";

/// State of OpenRazer on this computer.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Status {
    /// OpenRazer exists only for Linux
    pub supported_os: bool,
    /// the `razermouse` kernel module is loaded
    pub driver_loaded: bool,
    pub daemon_running: bool,
    /// mice the driver is bound to: (product id, sysfs directory)
    pub bound: Vec<(u16, String)>,
    pub mode: String,
    /// Polychromatic (https://polychromatic.app) — the OpenRazer lighting front-end — is installed
    pub polychromatic_installed: bool,
    /// its window, tray applet or effect helper is running: it manages the lighting
    pub polychromatic_running: bool,
}

pub fn status() -> Status {
    let root = sysfs_root();
    Status {
        supported_os: cfg!(target_os = "linux") || std::env::var_os("OPENRZR_OPENRAZER_SYSFS").is_some(),
        driver_loaded: root.exists(),
        daemon_running: daemon_running(),
        bound: bound_devices(&root).into_iter().map(|(p, d)| (p, d.display().to_string())).collect(),
        mode: mode().code().into(),
        polychromatic_installed: polychromatic_path().is_some(),
        polychromatic_running: polychromatic_running(),
    }
}

// ------------------------------------------------------------------ Polychromatic (separate program, not modified)

const POLY_PROGRAMS: &[&str] = &["polychromatic-controller", "polychromatic-tray-applet", "polychromatic-helper"];

/// Full path of `polychromatic-controller`, if installed.
pub fn polychromatic_path() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(["/usr/bin", "/usr/local/bin"].iter().map(PathBuf::from));
    dirs.into_iter().map(|d| d.join("polychromatic-controller")).find(|p| p.is_file())
}

/// Is `cmdline` (NUL-separated, as in /proc/PID/cmdline) one of the programs `names`?
/// Looks at the first two arguments: the program itself, or "python3 /usr/bin/polychromatic-…".
pub fn cmdline_is(cmdline: &str, names: &[&str]) -> bool {
    cmdline.split('\0').take(2).any(|a| names.iter().any(|n| crate::profile::file_name(a) == *n))
}

fn running_any(names: &[&str]) -> bool {
    let Ok(rd) = std::fs::read_dir("/proc") else { return false };
    rd.flatten().any(|e| {
        e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())
            && std::fs::read(e.path().join("cmdline")).map(|c| cmdline_is(&String::from_utf8_lossy(&c), names)).unwrap_or(false)
    })
}

pub fn polychromatic_running() -> bool {
    cfg!(target_os = "linux") && running_any(POLY_PROGRAMS)
}

/// Start the Polychromatic window as a separate process.
pub fn open_polychromatic() -> Result<()> {
    let p = polychromatic_path().ok_or_else(|| Error::NotFound("Polychromatic is not installed".into()))?;
    log_info!("app", "starting {}", p.display());
    std::process::Command::new(&p)
        .env_remove("LD_LIBRARY_PATH")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| Error::Io(format!("{}: {e}", p.display())))
}

/// Override for tests and trying the bridge without the driver: a directory that looks like
/// /sys/bus/hid/drivers/razermouse (with "0003:1532:PPPP.N" subdirectories).
fn sysfs_root() -> PathBuf {
    std::env::var_os("OPENRZR_OPENRAZER_SYSFS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DRIVER_DIR))
}

/// Driver-bound mice: "0003:1532:008F.0005" directories that carry OpenRazer attributes.
pub fn bound_devices(root: &Path) -> Vec<(u16, PathBuf)> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else { return v };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let parts: Vec<&str> = name.split([':', '.']).collect();
        if parts.len() < 3 || !parts[1].eq_ignore_ascii_case("1532") {
            continue;
        }
        let Ok(pid) = u16::from_str_radix(parts[2], 16) else { continue };
        let dir = e.path();
        if dir.join("device_type").exists() {
            v.push((pid, dir));
        }
    }
    v.sort();
    v
}

pub fn sysfs_dir(pid: u16) -> Option<PathBuf> {
    bound_devices(&sysfs_root()).into_iter().find(|(p, _)| *p == pid).map(|(_, d)| d)
}

pub fn daemon_running() -> bool {
    let Ok(rd) = std::fs::read_dir("/proc") else { return false };
    rd.flatten().any(|e| {
        e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())
            && std::fs::read(e.path().join("cmdline"))
                .map(|c| String::from_utf8_lossy(&c).contains("openrazer-daemon"))
                .unwrap_or(false)
    })
}

/// Stop the OpenRazer daemon of the current user (systemd user service).
pub fn stop_daemon() -> Result<String> {
    if !cfg!(target_os = "linux") {
        return Err(Error::Invalid("OpenRazer exists only on Linux".into()));
    }
    let out = std::process::Command::new("systemctl")
        .args(["--user", "stop", "openrazer-daemon.service"])
        .output()
        .map_err(|e| Error::Io(format!("systemctl: {e}")))?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    if daemon_running() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        log_warn!("app", "OpenRazer daemon still running after systemctl stop: {err}");
        return Err(Error::Io(format!(
            "The OpenRazer daemon is still running. Stop it manually: systemctl --user stop openrazer-daemon \
             (to keep it off: systemctl --user disable openrazer-daemon). {err}"
        )));
    }
    log_info!("app", "OpenRazer daemon stopped");
    Ok("stopped".into())
}

/// Transport for `info`: HID, or HID behind the OpenRazer bridge. Returns the sysfs dir when bridged.
pub fn open_transport(info: &DeviceInfo) -> Result<(Box<dyn Transport>, Option<PathBuf>)> {
    let mut m = mode();
    if m == Mode::Off && polychromatic_running() && sysfs_dir(info.pid).is_some() {
        // Polychromatic drives the mouse through OpenRazer: talking to it directly would collide
        log_warn!("device", "Polychromatic is running: working through the OpenRazer driver despite mode \"off\"");
        m = Mode::On;
    }
    let dir = if m == Mode::Off { None } else { sysfs_dir(info.pid) };
    match dir {
        Some(dir) => {
            // HID is still needed for buttons and macros; without access the bridge serves the rest
            let inner = match transport::open(info) {
                Ok(t) => Some(t),
                Err(e) => {
                    log_warn!("device", "HID not available behind OpenRazer ({e}); buttons and macros are off");
                    None
                }
            };
            log_info!("device", "OpenRazer driver found: {} — compatible mode", dir.display());
            let b = transport::openrazer::OpenRazerBridge::new(dir.clone(), inner);
            Ok((Box::new(b), Some(dir)))
        }
        None => {
            if m == Mode::On {
                log_warn!("device", "OpenRazer mode is on, but the driver is not bound to {:04x}; using HID", info.pid);
            }
            if m != Mode::Off && daemon_running() {
                log_warn!("device", "openrazer-daemon is running: replies may get mixed up");
            }
            Ok((transport::open(info)?, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polychromatic_processes() {
        assert!(cmdline_is("/usr/bin/python3\0/usr/bin/polychromatic-controller\0", POLY_PROGRAMS));
        assert!(cmdline_is("polychromatic-tray-applet\0--verbose\0", POLY_PROGRAMS));
        assert!(!cmdline_is("/usr/bin/vim\0polychromatic-controller.txt\0", POLY_PROGRAMS));
        assert!(!cmdline_is("/usr/bin/python3\0/x/y.py\0polychromatic-helper\0", POLY_PROGRAMS));
    }

    #[test]
    fn modes() {
        assert_eq!(Mode::parse("off"), Some(Mode::Off));
        assert_eq!(Mode::parse(""), Some(Mode::Auto));
        assert_eq!(Mode::parse("x"), None);
    }

    // sysfs names contain ':' which Windows does not allow in file names; OpenRazer is Linux-only anyway
    #[cfg(not(windows))]
    #[test]
    fn finds_bound_mice() {
        let root = std::env::temp_dir().join(format!("openrzr-compat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["0003:1532:008F.0007", "0003:1532:0090.0002", "0003:046D:C52B.0001", "module"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(root.join("0003:1532:008F.0007/device_type"), "Razer Naga Pro (Wired)\n").unwrap();
        std::fs::write(root.join("0003:046D:C52B.0001/device_type"), "x").unwrap();
        let v = bound_devices(&root);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, 0x008F);
        let _ = std::fs::remove_dir_all(&root);
    }
}
