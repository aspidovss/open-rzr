//! Logging: on by default, written to a local file, never sent anywhere.
//!
//! Targets:
//!   app     — program events (start, devices found, errors)
//!   ui      — what the user did in the window
//!   device  — high-level mouse operations (set DPI, bind button, ...)
//!   usb     — every raw packet to/from the mouse with timing (DEBUG)
//!
//! File: <config dir>/logs/open-rzr.log, rotated at 2 MB, 5 old files kept.
//! The last lines are also kept in memory for the Diagnostics tab.

use std::collections::VecDeque;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
    pub fn parse(s: &str) -> Option<Level> {
        match s.to_ascii_uppercase().as_str() {
            "DEBUG" => Some(Level::Debug),
            "INFO" => Some(Level::Info),
            "WARN" | "WARNING" => Some(Level::Warn),
            "ERROR" => Some(Level::Error),
            _ => None,
        }
    }
}

const MAX_FILE: u64 = 2 * 1024 * 1024;
const KEEP_FILES: usize = 5;
const RING: usize = 3000;

struct State {
    level: Level,
    console: bool,
    file: Option<File>,
    path: Option<PathBuf>,
    written: u64,
    ring: VecDeque<String>,
    seq: u64,
}

static STATE: Mutex<State> = Mutex::new(State {
    level: Level::Debug,
    console: false,
    file: None,
    path: None,
    written: 0,
    ring: VecDeque::new(),
    seq: 0,
});

/// Directory with settings: %APPDATA%\open-rzr or $XDG_CONFIG_HOME/open-rzr.
pub fn config_dir() -> PathBuf {
    if cfg!(windows) {
        if let Ok(a) = std::env::var("APPDATA") {
            return PathBuf::from(a).join("open-rzr");
        }
    }
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x).join("open-rzr");
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config").join("open-rzr")
}

pub fn log_dir() -> PathBuf {
    config_dir().join("logs")
}

pub fn log_file() -> PathBuf {
    log_dir().join("open-rzr.log")
}

/// Configure logging. Safe to call more than once (later calls change level/console).
pub fn init(level: Level, console: bool, to_file: bool) {
    {
        let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
        st.level = level;
        st.console = console;
        if to_file && st.file.is_none() {
            let dir = log_dir();
            let path = log_file();
            match fs::create_dir_all(&dir).and_then(|_| {
                OpenOptions::new().create(true).append(true).open(&path)
            }) {
                Ok(f) => {
                    st.written = f.metadata().map(|m| m.len()).unwrap_or(0);
                    st.file = Some(f);
                    st.path = Some(path);
                }
                Err(e) => eprintln!("open-rzr: cannot write the log: {e}"),
            }
        }
    }
    install_panic_hook();
}

pub fn set_level(level: Level) {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).level = level;
}

pub fn level() -> Level {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).level
}

pub fn file_path() -> Option<PathBuf> {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).path.clone()
}

fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            let bt = std::backtrace::Backtrace::force_capture();
            write(
                Level::Error,
                "app",
                format_args!(
                    "PANIC in thread {}: {}\n{}",
                    thread.name().unwrap_or("?"),
                    info,
                    bt
                ),
            );
            prev(info);
        }));
    });
}

/// Main entry used by the macros.
pub fn write(level: Level, target: &str, args: fmt::Arguments) {
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if level < st.level {
        return;
    }
    let line = format!("{} {:<5} {:<6} {}", timestamp(), level.as_str(), target, args);
    if st.console {
        eprintln!("{line}");
    }
    st.seq += 1;
    st.ring.push_back(line.clone());
    while st.ring.len() > RING {
        st.ring.pop_front();
    }
    if st.file.is_some() {
        let bytes = line.len() as u64 + 1;
        if st.written + bytes > MAX_FILE {
            rotate(&mut st);
        }
        if let Some(f) = st.file.as_mut() {
            if writeln!(f, "{line}").is_ok() {
                st.written += bytes;
            }
        }
    }
}

fn rotate(st: &mut State) {
    let Some(path) = st.path.clone() else { return };
    st.file = None;
    for i in (1..KEEP_FILES).rev() {
        let from = path.with_extension(format!("log.{i}"));
        let to = path.with_extension(format!("log.{}", i + 1));
        let _ = fs::rename(&from, &to);
    }
    let _ = fs::rename(&path, path.with_extension("log.1"));
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
        st.file = Some(f);
        st.written = 0;
    }
}

/// Lines kept in memory with their sequence numbers: returns (last_seq, lines after `since`).
pub fn recent(since: u64) -> (u64, Vec<String>) {
    let st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let total = st.seq;
    let have = st.ring.len() as u64;
    let first_seq = total - have + 1; // seq of ring[0]
    let skip = if since + 1 > first_seq { (since + 1 - first_seq) as usize } else { 0 };
    (total, st.ring.iter().skip(skip).cloned().collect())
}

/// Read the tail of the log file (for reports).
pub fn tail(lines: usize) -> Vec<String> {
    let Some(p) = file_path() else { return Vec::new() };
    match fs::read_to_string(p) {
        Ok(s) => {
            let v: Vec<&str> = s.lines().collect();
            v[v.len().saturating_sub(lines)..].iter().map(|x| x.to_string()).collect()
        }
        Err(_) => Vec::new(),
    }
}

/// Open the log folder in the file manager.
pub fn open_folder() -> std::io::Result<()> {
    let dir = log_dir();
    fs::create_dir_all(&dir)?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer").arg(&dir).spawn()?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .env_remove("LD_LIBRARY_PATH")
            .spawn()?;
    }
    Ok(())
}

/// UTC timestamp "2026-11-07 12:34:56.789Z" without external crates.
pub fn timestamp() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let ms = d.subsec_millis();
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, dd) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}Z",
        y,
        m,
        dd,
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60,
        ms
    )
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn startup_banner(kind: &str) {
    crate::log_info!("app", "{}", "=".repeat(60));
    crate::log_info!("app", "open-rzr {} ({}) started", crate::VERSION, kind);
    crate::log_info!(
        "app",
        "OS: {} {} | log: {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        file_path().map(|p| p.display().to_string()).unwrap_or_else(|| "not written".into())
    );
}

#[macro_export]
macro_rules! log_debug {
    ($target:expr, $($arg:tt)*) => { $crate::logs::write($crate::logs::Level::Debug, $target, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! log_info {
    ($target:expr, $($arg:tt)*) => { $crate::logs::write($crate::logs::Level::Info, $target, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! log_warn {
    ($target:expr, $($arg:tt)*) => { $crate::logs::write($crate::logs::Level::Warn, $target, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! log_error {
    ($target:expr, $($arg:tt)*) => { $crate::logs::write($crate::logs::Level::Error, $target, format_args!($($arg)*)) };
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_399), (2025, 11, 7));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn ring_and_since() {
        write(Level::Error, "test", format_args!("one"));
        let (seq, _) = recent(0);
        write(Level::Error, "test", format_args!("two"));
        let (seq2, lines) = recent(seq);
        assert_eq!(seq2, seq + 1);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("two"));
    }
}
