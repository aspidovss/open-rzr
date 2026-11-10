//! Which program is in front, and which programs are running — for profiles linked to programs.
//!
//! * Windows — GetForegroundWindow → process → QueryFullProcessImageNameW; the list of programs
//!   with a window comes from PowerShell `Get-Process`.
//! * Linux   — X11 only (`xprop`: _NET_ACTIVE_WINDOW → _NET_WM_PID → /proc/PID/exe). Wayland has no
//!   common way for a program to see the active window, so linking is not available there.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Program {
    /// file name, e.g. "witcher3.exe"
    pub name: String,
    pub path: String,
    /// window title, if known
    pub title: String,
}

/// `xprop -root _NET_ACTIVE_WINDOW` -> window id ("0x3a00007"); None for 0x0.
pub fn parse_xprop_window(text: &str) -> Option<String> {
    let id = text.split('#').nth(1)?.split(',').next()?.trim();
    if id.is_empty() || id == "0x0" {
        None
    } else {
        Some(id.to_string())
    }
}

/// `xprop -id W _NET_WM_PID` -> pid
pub fn parse_xprop_pid(text: &str) -> Option<u32> {
    text.split('=').nth(1)?.trim().parse().ok()
}

/// "name\tpath\ttitle" lines (PowerShell output) -> programs, one per path.
pub fn parse_program_lines(text: &str) -> Vec<Program> {
    let mut v: Vec<Program> = Vec::new();
    for line in text.lines() {
        let mut it = line.trim_end_matches('\r').split('\t');
        let (Some(_), Some(path)) = (it.next(), it.next()) else { continue };
        let title = it.next().unwrap_or("").to_string();
        if path.is_empty() || v.iter().any(|p| p.path.eq_ignore_ascii_case(path)) {
            continue;
        }
        v.push(Program { name: crate::profile::file_name(path).to_string(), path: path.to_string(), title });
    }
    v.sort_by_key(|p| p.name.to_lowercase());
    v
}

/// Can the active window be followed on this system?
pub fn supported() -> bool {
    if cfg!(windows) {
        return true;
    }
    cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_some() && std::env::var_os("WAYLAND_DISPLAY").is_none()
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
mod win {
    use std::os::raw::c_void;

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn QueryFullProcessImageNameW(process: *mut c_void, flags: u32, name: *mut u16, size: *mut u32) -> i32;
        fn CloseHandle(h: *mut c_void) -> i32;
    }

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    pub fn foreground_exe() -> Option<String> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 1024];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size);
            CloseHandle(h);
            if ok == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buf[..size as usize]))
        }
    }
}

#[cfg(windows)]
pub fn foreground_exe() -> Option<String> {
    win::foreground_exe()
}

#[cfg(windows)]
pub fn running_programs() -> Vec<Program> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let ps = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
              Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and $_.Path } | \
              ForEach-Object { \"$($_.ProcessName)`t$($_.Path)`t$($_.MainWindowTitle)\" }";
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => parse_program_lines(&String::from_utf8_lossy(&o.stdout)),
        Err(e) => {
            log_warn!("app", "program list: {e}");
            Vec::new()
        }
    }
}

// ------------------------------------------------------------------ Linux (X11)

#[cfg(not(windows))]
fn cmd(c: &str, args: &[&str]) -> Option<String> {
    let o = std::process::Command::new(c).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).to_string())
}

#[cfg(not(windows))]
pub fn foreground_exe() -> Option<String> {
    if !supported() {
        return None;
    }
    let w = parse_xprop_window(&cmd("xprop", &["-root", "_NET_ACTIVE_WINDOW"])?)?;
    let pid = parse_xprop_pid(&cmd("xprop", &["-id", &w, "_NET_WM_PID"])?)?;
    std::fs::read_link(format!("/proc/{pid}/exe")).ok().map(|p| p.display().to_string())
}

/// Programs of the current user (Linux: no window information, so system helpers are skipped).
#[cfg(not(windows))]
pub fn running_programs() -> Vec<Program> {
    use std::os::unix::fs::MetadataExt;
    let me = std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(u32::MAX);
    let mut lines = String::new();
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            if !e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if e.metadata().map(|m| m.uid()).unwrap_or(0) != me {
                continue;
            }
            let Ok(exe) = std::fs::read_link(e.path().join("exe")) else { continue };
            let exe = exe.display().to_string();
            let skip = ["/usr/lib", "/usr/libexec", "/lib", "/usr/sbin", "/sbin"];
            if skip.iter().any(|s| exe.starts_with(s)) || exe.ends_with(" (deleted)") {
                continue;
            }
            lines += &format!("x\t{exe}\t\n");
        }
    }
    parse_program_lines(&lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers() {
        assert_eq!(parse_xprop_window("_NET_ACTIVE_WINDOW(WINDOW): window id # 0x3a00007\n").as_deref(), Some("0x3a00007"));
        assert_eq!(parse_xprop_window("_NET_ACTIVE_WINDOW(WINDOW): window id # 0x0\n"), None);
        assert_eq!(parse_xprop_pid("_NET_WM_PID(CARDINAL) = 4242\n"), Some(4242));
        let v = parse_program_lines(
            "chrome\tC:\\Program Files\\Chromium\\Application\\chrome.exe\tНовая вкладка\r\n\
             chrome\tC:\\Program Files\\Chromium\\Application\\chrome.exe\tx\r\n\
             Audacity\tC:\\Program Files\\Audacity\\audacity.exe\t\r\nbroken\r\n",
        );
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "audacity.exe");
        assert_eq!(v[1].title, "Новая вкладка");
    }
}
