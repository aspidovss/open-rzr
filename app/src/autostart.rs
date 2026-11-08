//! Start with the system, std-only.
//! * Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run (via reg.exe, current user only)
//! * Linux:   ~/.config/autostart/open-rzr.desktop (XDG autostart)

use std::path::PathBuf;
#[cfg(windows)]
use std::process::Command;

pub const MINIMIZED_ARG: &str = "--minimized";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct State {
    pub enabled: bool,
    pub minimized: bool,
}

/// The program to start: the AppImage file when running from one, else this executable.
fn exe() -> Result<PathBuf, String> {
    if let Some(a) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(a));
    }
    std::env::current_exe().map_err(|e| e.to_string())
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const VALUE: &str = "open-rzr";

#[cfg(windows)]
fn reg(args: &[&str]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| format!("reg: {e}"))
}

#[cfg(windows)]
pub fn get() -> State {
    match reg(&["query", RUN_KEY, "/v", VALUE]) {
        Ok(o) if o.status.success() => {
            let out = String::from_utf8_lossy(&o.stdout);
            State { enabled: true, minimized: out.contains(MINIMIZED_ARG) }
        }
        _ => State::default(),
    }
}

#[cfg(windows)]
pub fn set(enabled: bool, minimized: bool) -> Result<(), String> {
    if !enabled {
        let _ = reg(&["delete", RUN_KEY, "/v", VALUE, "/f"]);
        return Ok(());
    }
    let mut cmd = format!("\"{}\"", exe()?.display());
    if minimized {
        cmd.push(' ');
        cmd.push_str(MINIMIZED_ARG);
    }
    let o = reg(&["add", RUN_KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &cmd, "/f"])?;
    if o.status.success() {
        Ok(())
    } else {
        Err(format!("reg add failed: {}", String::from_utf8_lossy(&o.stderr).trim()))
    }
}

#[cfg(not(windows))]
fn desktop_file() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("autostart").join("open-rzr.desktop")
}

#[cfg(not(windows))]
pub fn get() -> State {
    match std::fs::read_to_string(desktop_file()) {
        Ok(s) => State { enabled: !s.contains("Hidden=true"), minimized: s.contains(MINIMIZED_ARG) },
        Err(_) => State::default(),
    }
}

#[cfg(not(windows))]
pub fn set(enabled: bool, minimized: bool) -> Result<(), String> {
    let f = desktop_file();
    if !enabled {
        let _ = std::fs::remove_file(&f);
        return Ok(());
    }
    let exe = exe()?;
    let arg = if minimized { format!(" {MINIMIZED_ARG}") } else { String::new() };
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=open-rzr\nComment=Razer mouse configurator\n\
         Exec=\"{}\"{arg}\nIcon=open-rzr\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f, text).map_err(|e| format!("{}: {e}", f.display()))
}
