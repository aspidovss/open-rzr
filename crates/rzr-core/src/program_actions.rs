//! Program actions of macros: things the mouse cannot do by itself (launch a program or a web
//! page, run a command, type any text). The mouse macro ends with a press of an F13–F24 key;
//! open-rzr listens for that key while it runs and performs the actions.
//!
//! * Windows — a low-level keyboard hook takes the trigger keys in use (other programs don't see
//!   them) and sees both press and release, so a key or a mouse button can be held as long as the
//!   mouse button; launching through `ShellExecuteW`, keys, mouse buttons and text through
//!   `SendInput`.
//! * Linux   — reads the mouse's own keyboard interface (`/dev/input/event*` of Razer devices;
//!   the udev rule gives the logged-in user read access to these nodes only) and sees press and
//!   release too. Keys, mouse buttons and media keys are sent through a virtual input device
//!   (`/dev/uinput`, also opened to the user by the udev rule). Works on X11 and Wayland. Text is
//!   typed with `xdotool` (X11), `wtype` or `ydotool` (Wayland), if installed.

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
            "key" | "mouse" | "media" => {
                press(a, true)?;
                std::thread::sleep(Duration::from_millis(15));
                press(a, false)?;
            }
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

    const WM_APP: u32 = 0x8000;
    const VK_F13: u32 = 0x7C;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_UNICODE: u32 = 0x0004;

    static THREAD: AtomicU32 = AtomicU32::new(0);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[repr(C)]
    struct KbdLl {
        vk: u32,
        scan: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MouseInput {
        kind: u32,
        pad0: u32,
        dx: i32,
        dy: i32,
        data: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowsHookExW(id: i32, f: unsafe extern "system" fn(i32, usize, isize) -> isize, module: *mut c_void, thread: u32) -> *mut c_void;
        fn CallNextHookEx(h: *mut c_void, code: i32, w: usize, l: isize) -> isize;
        fn UnhookWindowsHookEx(h: *mut c_void) -> i32;
    }

    // INPUT on 64-bit Windows: 40 bytes, the union at offset 8
    #[cfg(target_pointer_width = "64")]
    const _: () = assert!(std::mem::size_of::<Input>() == 40 && std::mem::size_of::<MouseInput>() == 40);

    const WH_KEYBOARD_LL: i32 = 13;
    const LLKHF_INJECTED: u32 = 0x10;
    const WM_KEYDOWN: usize = 0x100;
    const WM_SYSKEYDOWN: usize = 0x104;

    enum Ev {
        Down(u8),
        Up(u8),
    }

    /// The table and the queue the hook hands its keys to (the hook itself must return at once).
    static HOOK: Mutex<Option<(Table, std::sync::mpsc::Sender<Ev>)>> = Mutex::new(None);

    unsafe extern "system" fn hook(code: i32, w: usize, l: isize) -> isize {
        if code == 0 && l != 0 {
            let kb = &*(l as *const KbdLl);
            if kb.flags & LLKHF_INJECTED == 0 && (VK_F13..VK_F13 + 12).contains(&kb.vk) {
                let k = *TRIGGER_KEYS.start() + (kb.vk - VK_F13) as u8;
                if let Ok(g) = HOOK.try_lock() {
                    if let Some((table, tx)) = g.as_ref() {
                        if table.try_lock().map(|t| t.contains_key(&k)).unwrap_or(false) {
                            let _ = tx.send(if w == WM_KEYDOWN || w == WM_SYSKEYDOWN { Ev::Down(k) } else { Ev::Up(k) });
                            return 1; // taken: other programs don't see it
                        }
                    }
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }

    pub fn start(table: Table) {
        let (tx, rx) = std::sync::mpsc::channel::<Ev>();
        *HOOK.lock().unwrap_or_else(|e| e.into_inner()) = Some((table.clone(), tx));
        // the worker: holds keys and buttons, starts action lists (repeats of a held key go on
        // to a held key, like a keyboard's own repeat)
        let t2 = table.clone();
        std::thread::spawn(move || {
            let mut down: HashSet<u8> = HashSet::new();
            while let Ok(ev) = rx.recv() {
                let (k, is_down) = match ev {
                    Ev::Down(k) => (k, true),
                    Ev::Up(k) => (k, false),
                };
                let entry = t2.lock().ok().and_then(|t| t.get(&k).cloned());
                let Some((name, actions)) = entry else { continue };
                let first = if is_down { down.insert(k) } else { down.remove(&k) };
                if crate::macro_doc::is_hold(&actions) {
                    let a = actions.iter().find(|a| a.on).cloned().unwrap_or_default();
                    // a mouse button is pressed once; a key repeats while held
                    if !is_down || first || a.kind == "key" {
                        if let Err(e) = press(&a, is_down) {
                            log_warn!("app", "{name}: {e}");
                        }
                    }
                } else if is_down && first {
                    fire(&t2, k);
                }
            }
        });
        std::thread::spawn(move || unsafe {
            let mut msg: Msg = std::mem::zeroed();
            PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, 0); // create the message queue
            THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
            let h = SetWindowsHookExW(WH_KEYBOARD_LL, hook, std::ptr::null_mut(), 0);
            if h.is_null() {
                log_warn!("app", "cannot listen for trigger keys (keyboard hook refused)");
                set_status(false, "keyboard hook refused");
                return;
            }
            let count = || table.lock().map(|t| t.len()).unwrap_or(0);
            set_status(true, &format!("{} trigger keys", count()));
            // the hook runs in this thread's message loop
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_APP {
                    set_status(true, &format!("{} trigger keys", count()));
                }
            }
            UnhookWindowsHookEx(h);
            set_status(false, "stopped");
        });
    }

    pub fn refresh() {
        let t = THREAD.load(Ordering::SeqCst);
        if t != 0 {
            unsafe { PostThreadMessageW(t, WM_APP, 0, 0) };
        }
    }

    /// Set-1 scan code of a HID key (0xE0xx = extended), for games that read scan codes.
    pub fn scan_code(hid: u8) -> Option<u16> {
        const LETTERS: [u16; 26] = [
            0x1E, 0x30, 0x2E, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18, 0x19, 0x10, 0x13, 0x1F, 0x14, 0x16, 0x2F, 0x11,
            0x2D, 0x15, 0x2C,
        ];
        const F13: [u16; 12] = [0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x76];
        const F1: [u16; 12] = [0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x57, 0x58];
        Some(match hid {
            0x04..=0x1D => LETTERS[(hid - 0x04) as usize],
            0x1E..=0x27 => 0x02 + (hid - 0x1E) as u16,
            0x28 => 0x1C,
            0x29 => 0x01,
            0x2A => 0x0E,
            0x2B => 0x0F,
            0x2C => 0x39,
            0x2D => 0x0C,
            0x2E => 0x0D,
            0x2F => 0x1A,
            0x30 => 0x1B,
            0x31 | 0x32 => 0x2B,
            0x33 => 0x27,
            0x34 => 0x28,
            0x35 => 0x29,
            0x36 => 0x33,
            0x37 => 0x34,
            0x38 => 0x35,
            0x39 => 0x3A,
            0x3A..=0x45 => F1[(hid - 0x3A) as usize],
            0x46 => 0xE037,
            0x47 => 0x46,
            0x49 => 0xE052,
            0x4A => 0xE047,
            0x4B => 0xE049,
            0x4C => 0xE053,
            0x4D => 0xE04F,
            0x4E => 0xE051,
            0x4F => 0xE04D,
            0x50 => 0xE04B,
            0x51 => 0xE050,
            0x52 => 0xE048,
            0x53 => 0x45,
            0x54 => 0xE035,
            0x55 => 0x37,
            0x56 => 0x4A,
            0x57 => 0x4E,
            0x58 => 0xE01C,
            0x59 => 0x4F,
            0x5A => 0x50,
            0x5B => 0x51,
            0x5C => 0x4B,
            0x5D => 0x4C,
            0x5E => 0x4D,
            0x5F => 0x47,
            0x60 => 0x48,
            0x61 => 0x49,
            0x62 => 0x52,
            0x63 => 0x53,
            0x64 => 0x56,
            0x65 => 0xE05D,
            0x68..=0x73 => F13[(hid - 0x68) as usize],
            0xE0 => 0x1D,
            0xE1 => 0x2A,
            0xE2 => 0x38,
            0xE3 => 0xE05B,
            0xE4 => 0xE01D,
            0xE5 => 0x36,
            0xE6 => 0xE038,
            0xE7 => 0xE05C,
            _ => return None,
        })
    }

    const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
    const KEYEVENTF_SCANCODE: u32 = 0x0008;

    fn key_input(hid: u8, down: bool) -> Result<Input, String> {
        let up = if down { 0 } else { KEYEVENTF_KEYUP };
        if hid == 0x48 {
            // Pause has no simple scan code
            return Ok(Input { kind: 1, ki: KeybdInput { vk: 0x13, scan: 0, flags: up, time: 0, extra: 0 }, pad: [0; 8] });
        }
        let sc = scan_code(hid).ok_or_else(|| format!("key 0x{hid:02x} cannot be sent"))?;
        let ext = if sc > 0xFF { KEYEVENTF_EXTENDEDKEY } else { 0 };
        Ok(Input { kind: 1, ki: KeybdInput { vk: 0, scan: sc & 0xFF, flags: KEYEVENTF_SCANCODE | ext | up, time: 0, extra: 0 }, pad: [0; 8] })
    }

    fn send(v: &[Input]) -> Result<(), String> {
        let n = unsafe { SendInput(v.len() as u32, v.as_ptr(), std::mem::size_of::<Input>() as i32) };
        if n as usize == v.len() {
            Ok(())
        } else {
            Err("Windows did not accept the input (a window of an administrator may be in front)".into())
        }
    }

    /// Press (`down`) or release a `key`, `mouse` or `media` action.
    pub fn press(a: &SoftAction, down: bool) -> Result<(), String> {
        match a.kind.as_str() {
            "key" => {
                let (mods, key) = ((a.value >> 8) as u8, a.value as u8);
                let mut v = Vec::new();
                let mod_keys: Vec<u8> = (0..8).filter(|b| mods & (1 << b) != 0).map(|b| 0xE0 + b).collect();
                if down {
                    for m in &mod_keys {
                        v.push(key_input(*m, true)?);
                    }
                    if key != 0 {
                        v.push(key_input(key, true)?);
                    }
                } else {
                    if key != 0 {
                        v.push(key_input(key, false)?);
                    }
                    for m in mod_keys.iter().rev() {
                        v.push(key_input(*m, false)?);
                    }
                }
                send(&v)
            }
            "media" => {
                let vk: u16 = match a.value {
                    0xCD => 0xB3,
                    0xB5 => 0xB0,
                    0xB6 => 0xB1,
                    0xB7 => 0xB2,
                    0xE2 => 0xAD,
                    0xE9 => 0xAF,
                    0xEA => 0xAE,
                    v => return Err(format!("media key 0x{v:x} cannot be sent")),
                };
                let up = if down { 0 } else { KEYEVENTF_KEYUP };
                send(&[Input { kind: 1, ki: KeybdInput { vk, scan: 0, flags: KEYEVENTF_EXTENDEDKEY | up, time: 0, extra: 0 }, pad: [0; 8] }])
            }
            "mouse" => {
                let (flags, data) = match (a.value, down) {
                    (1, true) => (0x0002, 0),
                    (1, false) => (0x0004, 0),
                    (2, true) => (0x0008, 0),
                    (2, false) => (0x0010, 0),
                    (3, true) => (0x0020, 0),
                    (3, false) => (0x0040, 0),
                    (4, true) => (0x0080, 1),
                    (4, false) => (0x0100, 1),
                    (5, true) => (0x0080, 2),
                    (5, false) => (0x0100, 2),
                    (b, _) => return Err(format!("mouse button {b} cannot be sent")),
                };
                let m = MouseInput { kind: 0, pad0: 0, dx: 0, dy: 0, data, flags, time: 0, extra: 0 };
                // INPUT is a union: the mouse member has the same size as `Input`
                let i: Input = unsafe { std::mem::transmute_copy(&m) };
                send(&[i])
            }
            k => Err(format!("{k} cannot be held")),
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

    /// A trigger key pressed (`true`) or released; the keyboard's own repeats (value 2) are left
    /// out: a held key is repeated by the desktop itself.
    pub fn trigger_event(ty: u16, code: u16, val: i32) -> Option<(u8, bool)> {
        (ty == EV_KEY && (val == 0 || val == 1) && (KEY_F13..=KEY_F24).contains(&code)).then(|| (TRIGGER_KEYS.start() + (code - KEY_F13) as u8, val == 1))
    }

    /// The worker: holds keys and buttons while the mouse button is held, starts action lists.
    fn worker(table: Table) -> std::sync::mpsc::Sender<(u8, bool)> {
        let (tx, rx) = std::sync::mpsc::channel::<(u8, bool)>();
        std::thread::spawn(move || {
            let mut down: HashSet<u8> = HashSet::new();
            while let Ok((k, is_down)) = rx.recv() {
                let entry = table.lock().ok().and_then(|t| t.get(&k).cloned());
                let Some((name, actions)) = entry else { continue };
                let first = if is_down { down.insert(k) } else { down.remove(&k) };
                if crate::macro_doc::is_hold(&actions) {
                    if first {
                        let a = actions.iter().find(|a| a.on).cloned().unwrap_or_default();
                        if let Err(e) = press(&a, is_down) {
                            log_warn!("app", "{name}: {e}");
                        }
                    }
                } else if is_down && first {
                    fire(&table, k);
                }
            }
        });
        tx
    }

    pub fn start(table: Table) {
        if !cfg!(target_os = "linux") {
            set_status(false, "not available on this system");
            return;
        }
        let tx = worker(table.clone());
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
                            open.lock().unwrap_or_else(|e| e.into_inner()).insert(n.clone());
                            log_info!("app", "listening for macro trigger keys on {}", path.display());
                            let (tx, open, n) = (tx.clone(), open.clone(), n.clone());
                            std::thread::spawn(move || {
                                let mut buf = [0u8; 24 * 16];
                                while let Ok(len) = f.read(&mut buf) {
                                    if len == 0 {
                                        break;
                                    }
                                    for ev in buf[..len].chunks_exact(24) {
                                        if let Some(k) = parse_event(ev).and_then(|(t, c, v)| trigger_event(t, c, v)) {
                                            let _ = tx.send(k);
                                        }
                                    }
                                }
                                open.lock().unwrap_or_else(|e| e.into_inner()).remove(&n);
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

    // ---------------------------------------------------------- sending input: /dev/uinput

    /// Linux input codes of HID keyboard usages 0x00..=0xFF (the kernel's `hid_keyboard` table;
    /// 0 = no such key).
    #[rustfmt::skip]
    const HID_TO_EV: [u8; 256] = [
          0,  0,  0,  0, 30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38,
         50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44,  2,  3,
          4,  5,  6,  7,  8,  9, 10, 11, 28,  1, 14, 15, 57, 12, 13, 26,
         27, 43, 43, 39, 40, 41, 51, 52, 53, 58, 59, 60, 61, 62, 63, 64,
         65, 66, 67, 68, 87, 88, 99, 70,119,110,102,104,111,107,109,106,
        105,108,103, 69, 98, 55, 74, 78, 96, 79, 80, 81, 75, 76, 77, 71,
         72, 73, 82, 83, 86,127,116,117,183,184,185,186,187,188,189,190,
        191,192,193,194,134,138,130,132,128,129,131,137,133,135,136,113,
        115,114,  0,  0,  0,121,  0, 89, 93,124, 92, 94, 95,  0,  0,  0,
        122,123, 90, 91, 85,  0,  0,  0,  0,  0,  0,  0,111,  0,  0,  0,
          0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,
          0,  0,  0,  0,  0,  0,179,180,  0,  0,  0,  0,  0,  0,  0,  0,
          0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,
          0,  0,  0,  0,  0,  0,  0,  0,111,  0,  0,  0,  0,  0,  0,  0,
         29, 42, 56,125, 97, 54,100,126,164,166,165,163,161,115,114,113,
        150,158,159,128,136,177,178,176,142,152,173,140,  0,  0,  0,  0,
    ];

    /// Linux code of a media key (HID consumer usage), as offered by the program.
    pub fn media_code(usage: i64) -> Option<u16> {
        Some(match usage {
            0xCD => 164, // KEY_PLAYPAUSE
            0xB5 => 163, // KEY_NEXTSONG
            0xB6 => 165, // KEY_PREVIOUSSONG
            0xB7 => 166, // KEY_STOPCD
            0xE2 => 113, // KEY_MUTE
            0xE9 => 115, // KEY_VOLUMEUP
            0xEA => 114, // KEY_VOLUMEDOWN
            _ => return None,
        })
    }

    /// Linux code of mouse button 1..=5 (left, right, middle, back, forward).
    pub fn button_code(b: i64) -> Option<u16> {
        (1..=5).contains(&b).then(|| 0x110 + (b - 1) as u16)
    }

    /// Linux code of a HID key usage.
    pub fn key_code(hid: u8) -> Option<u16> {
        match HID_TO_EV[hid as usize] {
            0 => None,
            c => Some(c as u16),
        }
    }

    /// The events of pressing (`down`) or releasing an action: (type, code, value).
    pub fn events(a: &SoftAction, down: bool) -> Result<Vec<(u16, u16, i32)>, String> {
        let v = down as i32;
        let mut out = Vec::new();
        match a.kind.as_str() {
            "key" => {
                let (mods, key) = ((a.value >> 8) as u8, a.value as u8);
                let mut codes: Vec<u16> = (0..8u8).filter(|b| mods & (1 << b) != 0).filter_map(|b| key_code(0xE0 + b)).collect();
                if key != 0 {
                    codes.push(key_code(key).ok_or_else(|| format!("key 0x{key:02x} cannot be sent"))?);
                }
                if !down {
                    codes.reverse();
                }
                out.extend(codes.into_iter().map(|c| (EV_KEY, c, v)));
            }
            "media" => out.push((EV_KEY, media_code(a.value).ok_or_else(|| format!("media key 0x{:x} cannot be sent", a.value))?, v)),
            "mouse" => out.push((EV_KEY, button_code(a.value).ok_or_else(|| format!("mouse button {} cannot be sent", a.value))?, v)),
            k => return Err(format!("{k} is not a key")),
        }
        out.push((EV_SYN, SYN_REPORT, 0));
        Ok(out)
    }

    const EV_SYN: u16 = 0;
    const SYN_REPORT: u16 = 0;
    const EV_REL: u16 = 2;
    // _IO / _IOW('U', n, ...) of <linux/uinput.h>
    const UI_DEV_CREATE: std::os::raw::c_ulong = 0x5501;
    const UI_DEV_SETUP: std::os::raw::c_ulong = 0x405C_5503; // size of uinput_setup: 92
    const UI_SET_EVBIT: std::os::raw::c_ulong = 0x4004_5564;
    const UI_SET_KEYBIT: std::os::raw::c_ulong = 0x4004_5565;
    const UI_SET_RELBIT: std::os::raw::c_ulong = 0x4004_5566;

    #[repr(C)]
    struct UinputSetup {
        /// bustype, vendor, product, version
        id: [u16; 4],
        name: [u8; 80],
        ff_effects_max: u32,
    }
    const _: () = assert!(std::mem::size_of::<UinputSetup>() == 92);

    extern "C" {
        fn ioctl(fd: std::os::raw::c_int, req: std::os::raw::c_ulong, ...) -> std::os::raw::c_int;
    }

    /// The virtual input device, made on first use.
    static UINPUT: Mutex<Option<std::fs::File>> = Mutex::new(None);

    fn create() -> Result<std::fs::File, String> {
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::io::AsRawFd;
        let f = std::fs::OpenOptions::new().write(true).custom_flags(0o4000 /* O_NONBLOCK */).open("/dev/uinput").map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                "no access to /dev/uinput — install the udev rule (Settings → Linux access) and log in again".to_string()
            } else {
                format!("/dev/uinput: {e} (is the uinput kernel module loaded?)")
            }
        })?;
        let fd = f.as_raw_fd();
        let ok = |r: std::os::raw::c_int, what: &str| if r < 0 { Err(format!("uinput {what}: {}", std::io::Error::last_os_error())) } else { Ok(()) };
        unsafe {
            ok(ioctl(fd, UI_SET_EVBIT, EV_KEY as std::os::raw::c_int), "keys")?;
            for c in HID_TO_EV.iter().filter(|c| **c != 0) {
                ok(ioctl(fd, UI_SET_KEYBIT, *c as std::os::raw::c_int), "key")?;
            }
            for c in (0x110..=0x114).chain([164, 163, 165, 166, 113, 115, 114]) {
                ok(ioctl(fd, UI_SET_KEYBIT, c as std::os::raw::c_int), "button")?;
            }
            // a pointer axis, so that desktops take the buttons as mouse buttons
            ok(ioctl(fd, UI_SET_EVBIT, EV_REL as std::os::raw::c_int), "pointer")?;
            ok(ioctl(fd, UI_SET_RELBIT, 0), "x")?;
            ok(ioctl(fd, UI_SET_RELBIT, 1), "y")?;
            // vendor 0, not Razer: the trigger listener must not read this device
            let mut setup = UinputSetup { id: [0x06 /* BUS_VIRTUAL */, 0, 0, 1], name: [0; 80], ff_effects_max: 0 };
            let name = b"open-rzr virtual input";
            setup.name[..name.len()].copy_from_slice(name);
            ok(ioctl(fd, UI_DEV_SETUP, &setup as *const UinputSetup), "setup")?;
            ok(ioctl(fd, UI_DEV_CREATE), "create")?;
        }
        // the desktop needs a moment to open the new device before its first event
        std::thread::sleep(Duration::from_millis(150));
        log_info!("app", "program actions: virtual input device created (/dev/uinput)");
        Ok(f)
    }

    /// One input event as `struct input_event` (time 0: the kernel stamps it).
    pub fn event_bytes(ty: u16, code: u16, value: i32) -> [u8; 24] {
        let mut b = [0u8; 24];
        b[16..18].copy_from_slice(&ty.to_ne_bytes());
        b[18..20].copy_from_slice(&code.to_ne_bytes());
        b[20..24].copy_from_slice(&value.to_ne_bytes());
        b
    }

    /// Press (`down`) or release a `key`, `mouse` or `media` action through /dev/uinput.
    pub fn press(a: &SoftAction, down: bool) -> Result<(), String> {
        use std::io::Write;
        let evs = events(a, down)?;
        let bytes: Vec<u8> = evs.iter().flat_map(|(t, c, v)| event_bytes(*t, *c, *v)).collect();
        let mut g = UINPUT.lock().unwrap_or_else(|e| e.into_inner());
        for attempt in 0..2 {
            if g.is_none() {
                *g = Some(create()?);
            }
            match g.as_mut().map(|f| f.write_all(&bytes)) {
                Some(Ok(())) => return Ok(()),
                Some(Err(e)) if attempt == 0 => {
                    log_warn!("app", "virtual input device: {e}; making it again");
                    *g = None;
                }
                Some(Err(e)) => return Err(format!("uinput: {e}")),
                None => {}
            }
        }
        Err("uinput: not available".into())
    }

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
        fn trigger_events() {
            let mut b = [0u8; 24];
            b[16..18].copy_from_slice(&1u16.to_ne_bytes());
            b[18..20].copy_from_slice(&184u16.to_ne_bytes());
            b[20..24].copy_from_slice(&1i32.to_ne_bytes());
            let (t, c, v) = parse_event(&b).unwrap();
            assert_eq!(trigger_event(t, c, v), Some((0x69, true)));
            assert_eq!(trigger_event(t, 30, 1), None);
        }

        #[test]
        fn input_events() {
            // Ctrl+Shift+A down: modifiers first; up: the key first
            let a = SoftAction { kind: "key".into(), value: 0x03 << 8 | 0x04, text: String::new(), on: true };
            assert_eq!(events(&a, true).unwrap(), vec![(1, 29, 1), (1, 42, 1), (1, 30, 1), (0, 0, 0)]);
            assert_eq!(events(&a, false).unwrap(), vec![(1, 30, 0), (1, 42, 0), (1, 29, 0), (0, 0, 0)]);
            // digits, Enter, F13, right Alt
            assert_eq!([key_code(0x1e), key_code(0x27), key_code(0x28), key_code(0x68), key_code(0xe6)], [Some(2), Some(11), Some(28), Some(183), Some(100)]);
            assert_eq!(key_code(0x00), None);
            let m = SoftAction { kind: "mouse".into(), value: 4, text: String::new(), on: true };
            assert_eq!(events(&m, true).unwrap()[0], (1, 0x113, 1)); // BTN_SIDE (back)
            let v = SoftAction { kind: "media".into(), value: 0xE9, text: String::new(), on: true };
            assert_eq!(events(&v, true).unwrap()[0], (1, 115, 1));
            assert!(events(&SoftAction { kind: "media".into(), value: 0x1234, text: String::new(), on: true }, true).is_err());
            let b = event_bytes(1, 30, 1);
            assert_eq!(parse_event(&b), Some((1, 30, 1)));
            // a trigger's press and release; the keyboard's repeats are left out
            assert_eq!(trigger_event(1, 183, 1), Some((0x68, true)));
            assert_eq!(trigger_event(1, 183, 0), Some((0x68, false)));
            assert_eq!(trigger_event(1, 183, 2), None);
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

use imp::{launch, press, run_command, type_text};

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
