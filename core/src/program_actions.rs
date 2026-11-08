//! Program actions of macros: things the mouse cannot do by itself (launch a program or a web
//! page, run a command, type any text). The mouse macro ends with a press of an F13–F24 key;
//! open-rzr listens for that key while it runs and performs the actions.
//!
//! * Windows — `RegisterHotKey` for the trigger keys in use (other programs don't see them);
//!   launching through `ShellExecuteW`, text through `SendInput` (Unicode).
//! * Linux   — reads the mouse's own keyboard interface (`/dev/input/event*` of Razer devices;
//!   the udev rule gives the logged-in user read access to these nodes only). Works on X11 and
//!   Wayland. Text is typed with `xdotool` (X11), `wtype` or `ydotool` (Wayland), if installed.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::macro_doc::{SoftAction, TRIGGER_KEYS};

/// trigger key (HID code) -> (macro name, actions)
pub type Table = Arc<Mutex<HashMap<u8, (String, Vec<SoftAction>)>>>;

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Status {
    pub supported: bool,
    pub active: bool,
    /// why it does not work, or how many devices are listened to
    pub note: String,
}

static STATUS: Mutex<Option<Status>> = Mutex::new(None);
static BUSY: Mutex<Option<HashSet<u8>>> = Mutex::new(None);

pub fn status() -> Status {
    STATUS.lock().map(|s| s.clone()).unwrap_or(None).unwrap_or_default()
}

fn set_status(active: bool, note: &str) {
    if let Ok(mut s) = STATUS.lock() {
        *s = Some(Status { supported: cfg!(any(windows, target_os = "linux")), active, note: note.into() });
    }
}

/// F13.. as a name.
pub fn trigger_name(k: u8) -> String {
    format!("F{}", 13 + k.saturating_sub(*TRIGGER_KEYS.start()))
}

/// Perform actions one after another (blocking).
pub fn run(actions: &[SoftAction]) -> Result<(), String> {
    for a in actions.iter().filter(|a| a.on) {
        match a.kind.as_str() {
            "delay" => std::thread::sleep(Duration::from_millis(a.value.clamp(0, 600_000) as u64)),
            "launch" => launch(a.text.trim())?,
            "run" => run_command(a.text.trim())?,
            "type" => type_text(&a.text)?,
            k => return Err(format!("unknown program action {k}")),
        }
    }
    Ok(())
}

/// Called by the listeners: run the actions bound to `key` in a thread (one run per key at a time).
fn fire(table: &Table, key: u8) {
    let Some((name, actions)) = table.lock().ok().and_then(|t| t.get(&key).cloned()) else { return };
    {
        let mut b = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        if !b.get_or_insert_with(HashSet::new).insert(key) {
            log_info!("app", "macro \"{name}\": still running, {} ignored", trigger_name(key));
            return;
        }
    }
    std::thread::spawn(move || {
        log_info!("app", "macro \"{name}\": program actions ({})", trigger_name(key));
        if let Err(e) = run(&actions) {
            log_warn!("app", "macro \"{name}\": {e}");
        }
        if let Some(b) = BUSY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            b.remove(&key);
        }
    });
}

/// Start listening for trigger keys. Call once; call `refresh` after changing the table.
pub fn start(table: Table) {
    set_status(false, "starting");
    imp::start(table);
}

/// The table changed (Windows re-registers its hot keys).
pub fn refresh() {
    imp::refresh();
}

#[cfg_attr(windows, allow(dead_code))]
fn is_web(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://") || l.starts_with("www.") || l.starts_with("mailto:")
}

fn web(t: &str) -> String {
    if t.to_ascii_lowercase().starts_with("www.") {
        format!("https://{t}")
    } else {
        t.to_string()
    }
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::raw::c_void;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[repr(C)]
    struct Msg {
        hwnd: *mut c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        x: i32,
        y: i32,
        private: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeybdInput {
        vk: u16,
        scan: u16,
        flags: u32,
        time: u32,
        extra: usize,
    }

    /// INPUT with the keyboard member; padded to the size of the union (MOUSEINPUT)
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Input {
        kind: u32,
        ki: KeybdInput,
        pad: [u8; 8],
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hwnd: *mut c_void, id: i32, mods: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hwnd: *mut c_void, id: i32) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32) -> i32;
        fn PeekMessageW(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32, remove: u32) -> i32;
        fn PostThreadMessageW(thread: u32, msg: u32, w: usize, l: isize) -> i32;
        fn SendInput(n: u32, inputs: *const Input, size: i32) -> u32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(hwnd: *mut c_void, op: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
    }

    const WM_HOTKEY: u32 = 0x0312;
    const WM_APP: u32 = 0x8000;
    const MOD_NOREPEAT: u32 = 0x4000;
    const VK_F13: u32 = 0x7C;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_UNICODE: u32 = 0x0004;

    static THREAD: AtomicU32 = AtomicU32::new(0);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn start(table: Table) {
        std::thread::spawn(move || unsafe {
            let mut msg: Msg = std::mem::zeroed();
            PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, 0); // create the message queue
            THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
            let mut registered: HashSet<u8> = HashSet::new();
            loop {
                let want: HashSet<u8> = table.lock().map(|t| t.keys().copied().collect()).unwrap_or_default();
                for k in registered.difference(&want) {
                    UnregisterHotKey(std::ptr::null_mut(), *k as i32);
                }
                registered.retain(|k| want.contains(k));
                let mut failed = Vec::new();
                for k in want.difference(&registered.clone()) {
                    let vk = VK_F13 + (*k - *TRIGGER_KEYS.start()) as u32;
                    if RegisterHotKey(std::ptr::null_mut(), *k as i32, MOD_NOREPEAT, vk) != 0 {
                        registered.insert(*k);
                    } else {
                        failed.push(trigger_name(*k));
                    }
                }
                if failed.is_empty() {
                    set_status(true, &format!("{} trigger keys", registered.len()));
                } else {
                    log_warn!("app", "trigger keys taken by another program: {}", failed.join(", "));
                    set_status(true, &format!("taken by another program: {}", failed.join(", ")));
                }
                if GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) <= 0 {
                    break;
                }
                if msg.message == WM_HOTKEY {
                    fire(&table, msg.wparam as u8);
                }
            }
            set_status(false, "stopped");
        });
    }

    pub fn refresh() {
        let t = THREAD.load(Ordering::SeqCst);
        if t != 0 {
            unsafe { PostThreadMessageW(t, WM_APP, 0, 0) };
        }
    }

    pub fn launch(target: &str) -> Result<(), String> {
        let t = web(target);
        let r = unsafe {
            ShellExecuteW(std::ptr::null_mut(), wide("open").as_ptr(), wide(&t).as_ptr(), std::ptr::null(), std::ptr::null(), 1)
        };
        if r > 32 {
            Ok(())
        } else {
            Err(format!("cannot open {t} (error {r})"))
        }
    }

    pub fn run_command(cmd: &str) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .arg("/C")
            .raw_arg(cmd)
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("{cmd}: {e}"))
    }

    pub fn type_text(text: &str) -> Result<(), String> {
        let mut v = Vec::new();
        for u in text.replace("\r\n", "\n").replace('\n', "\r").encode_utf16() {
            for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
                v.push(Input { kind: 1, ki: KeybdInput { vk: 0, scan: u, flags, time: 0, extra: 0 }, pad: [0; 8] });
            }
        }
        let n = unsafe { SendInput(v.len() as u32, v.as_ptr(), std::mem::size_of::<Input>() as i32) };
        if n as usize == v.len() {
            Ok(())
        } else {
            Err("Windows did not accept the text (another window may block input)".into())
        }
    }
}

// ------------------------------------------------------------------ Linux and others

#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    const EV_KEY: u16 = 1;
    /// Linux input codes of F13..F24
    const KEY_F13: u16 = 183;
    const KEY_F24: u16 = 194;

    /// event nodes of Razer devices that can send keys
    pub fn razer_event_nodes(sys: &Path) -> Vec<String> {
        let mut v = Vec::new();
        let Ok(rd) = std::fs::read_dir(sys) else { return v };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("event") {
                continue;
            }
            let id = e.path().join("device/id");
            let vendor = std::fs::read_to_string(id.join("vendor")).unwrap_or_default();
            if vendor.trim().eq_ignore_ascii_case("1532") {
                // only interfaces that report keys (the mouse's keyboard part)
                let keys = std::fs::read_to_string(e.path().join("device/capabilities/key")).unwrap_or_default();
                if keys.trim() != "0" && !keys.trim().is_empty() {
                    v.push(name);
                }
            }
        }
        v.sort();
        v
    }

    /// Parse a 64-bit `struct input_event` (24 bytes): -> (type, code, value)
    pub fn parse_event(b: &[u8]) -> Option<(u16, u16, i32)> {
        if b.len() < 24 {
            return None;
        }
        let ty = u16::from_ne_bytes([b[16], b[17]]);
        let code = u16::from_ne_bytes([b[18], b[19]]);
        let val = i32::from_ne_bytes([b[20], b[21], b[22], b[23]]);
        Some((ty, code, val))
    }

    pub fn trigger_of(ty: u16, code: u16, val: i32) -> Option<u8> {
        (ty == EV_KEY && val == 1 && (KEY_F13..=KEY_F24).contains(&code)).then(|| TRIGGER_KEYS.start() + (code - KEY_F13) as u8)
    }

    pub fn start(table: Table) {
        if !cfg!(target_os = "linux") {
            set_status(false, "not available on this system");
            return;
        }
        std::thread::spawn(move || {
            let open: Arc<Mutex<HashSet<String>>> = Arc::default();
            let mut last_note = String::new();
            loop {
                let nodes = razer_event_nodes(Path::new("/sys/class/input"));
                let mut denied = 0;
                for n in &nodes {
                    if open.lock().map(|o| o.contains(n)).unwrap_or(true) {
                        continue;
                    }
                    let path = PathBuf::from("/dev/input").join(n);
                    match std::fs::File::open(&path) {
                        Ok(mut f) => {
                            open.lock().unwrap().insert(n.clone());
                            log_info!("app", "listening for macro trigger keys on {}", path.display());
                            let (table, open, n) = (table.clone(), open.clone(), n.clone());
                            std::thread::spawn(move || {
                                let mut buf = [0u8; 24 * 16];
                                while let Ok(len) = f.read(&mut buf) {
                                    if len == 0 {
                                        break;
                                    }
                                    for ev in buf[..len].chunks_exact(24) {
                                        if let Some(k) = parse_event(ev).and_then(|(t, c, v)| trigger_of(t, c, v)) {
                                            fire(&table, k);
                                        }
                                    }
                                }
                                open.lock().unwrap().remove(&n);
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => denied += 1,
                        Err(_) => {}
                    }
                }
                let count = open.lock().map(|o| o.len()).unwrap_or(0);
                let note = if count > 0 {
                    format!("{count} input devices")
                } else if denied > 0 {
                    "no access to the mouse's key input — install the udev rule (Settings → Linux access) and reconnect the mouse".into()
                } else {
                    "mouse not found".into()
                };
                if note != last_note {
                    set_status(count > 0, &note);
                    last_note = note;
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        });
    }

    pub fn refresh() {}

    fn which(cmd: &str) -> bool {
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file())).unwrap_or(false)
    }

    fn spawn(cmd: &str, args: &[&str]) -> Result<(), String> {
        std::process::Command::new(cmd)
            .args(args)
            .env_remove("LD_LIBRARY_PATH")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("{cmd}: {e}"))
    }

    pub fn launch(target: &str) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let exec = std::fs::metadata(target).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false);
        // an executable file, or a program name found in PATH ("firefox"); anything else is opened
        if exec || (!is_web(target) && !target.contains('/') && which(target)) {
            spawn(target, &[])
        } else {
            spawn("xdg-open", &[&web(target)])
        }
    }

    pub fn run_command(cmd: &str) -> Result<(), String> {
        spawn("sh", &["-c", cmd])
    }

    pub fn type_text(text: &str) -> Result<(), String> {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let tools: &[(&str, &[&str])] = if wayland {
            &[("wtype", &["--"]), ("ydotool", &["type", "--"])]
        } else {
            &[("xdotool", &["type", "--delay", "8", "--"]), ("ydotool", &["type", "--"])]
        };
        for (t, args) in tools {
            if which(t) {
                let mut a: Vec<&str> = args.to_vec();
                a.push(text);
                let st = std::process::Command::new(t).args(&a).status().map_err(|e| format!("{t}: {e}"))?;
                return if st.success() { Ok(()) } else { Err(format!("{t} failed")) };
            }
        }
        Err(if wayland { "install wtype or ydotool to type text" } else { "install xdotool to type text" }.into())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn events() {
            let mut b = [0u8; 24];
            b[16..18].copy_from_slice(&1u16.to_ne_bytes());
            b[18..20].copy_from_slice(&184u16.to_ne_bytes());
            b[20..24].copy_from_slice(&1i32.to_ne_bytes());
            let (t, c, v) = parse_event(&b).unwrap();
            assert_eq!(trigger_of(t, c, v), Some(0x69));
            assert_eq!(trigger_of(t, c, 0), None); // release
            assert_eq!(trigger_of(t, 30, 1), None);
        }

        #[test]
        fn finds_nodes() {
            let root = std::env::temp_dir().join(format!("openrzr-input-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for (n, vendor, keys) in [("event3", "1532", "ff 0"), ("event4", "1532", "0"), ("event5", "046d", "ff")] {
                let d = root.join(n).join("device");
                std::fs::create_dir_all(d.join("id")).unwrap();
                std::fs::create_dir_all(d.join("capabilities")).unwrap();
                std::fs::write(d.join("id/vendor"), format!("{vendor}\n")).unwrap();
                std::fs::write(d.join("capabilities/key"), format!("{keys}\n")).unwrap();
            }
            std::fs::create_dir_all(root.join("mouse0")).unwrap();
            assert_eq!(razer_event_nodes(&root), ["event3"]);
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}

use imp::{launch, run_command, type_text};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_web() {
        assert_eq!(trigger_name(0x68), "F13");
        assert_eq!(trigger_name(0x73), "F24");
        assert!(is_web("www.example.org"));
        assert_eq!(web("www.example.org"), "https://www.example.org");
        assert!(!is_web("C:\\Games\\x.exe"));
        assert!(run(&[SoftAction { kind: "delay".into(), value: 1, text: String::new(), on: true }]).is_ok());
        assert!(run(&[SoftAction::new("bogus", "")]).is_err());
    }
}
