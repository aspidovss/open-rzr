//! Debugging of wireless (Bluetooth LE) connections — Diagnostics → Wireless.
//!
//! Over Bluetooth the mouse is a "HID over GATT" device: Windows splits it into HID collections
//! and, separately, lists its GATT services. This module shows what is there, so the way the
//! settings travel over Bluetooth can be found:
//! * **HID collections** — usage page, report lengths and report ids (from the HID descriptor);
//! * **probe** (on request) — the read-only "firmware version" request (0x0081) sent in every
//!   form a HID collection can take it: feature report with each report id, output report →
//!   input report. By default only vendor-defined collections; "all" includes the standard ones;
//! * **GATT** (Windows) — services and characteristics with their properties; readable values
//!   are read (battery level, device information, …). Nothing is written over GATT.

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct HidCaps {
    pub path: String,
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
    pub input_ids: Vec<u8>,
    pub output_ids: Vec<u8>,
    pub feature_ids: Vec<u8>,
    /// HID report descriptor (Linux) — empty on Windows
    pub descriptor: String,
    pub error: String,
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Probe {
    pub path: String,
    /// "feature id 0x05", "output id 0x00 → input", …
    pub method: String,
    pub ok: bool,
    /// error, or the reply in hex
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct GattChar {
    pub uuid: String,
    pub name: String,
    pub handle: u16,
    /// "read write write-no-response notify indicate"
    pub props: String,
    /// value in hex (readable characteristics), or why it could not be read
    pub value: String,
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct GattService {
    pub uuid: String,
    pub name: String,
    pub handle: u16,
    pub chars: Vec<GattChar>,
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Report {
    /// Bluetooth address of the mouse, if found ("eac88650b167")
    pub address: String,
    pub hid: Vec<HidCaps>,
    pub probes: Vec<Probe>,
    pub gatt: Vec<GattService>,
    pub gatt_error: String,
    /// from the standard Battery service
    pub battery: Option<u8>,
    pub notes: Vec<String>,
}

/// One notification received from a GATT characteristic.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Notification {
    /// milliseconds after the subscription
    pub t_ms: u64,
    pub handle: u16,
    pub uuid: String,
    pub hex: String,
    pub len: usize,
}

/// Result of talking to the Razer GATT service (write a request, listen to notifications).
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Exchange {
    /// what was written (hex), empty when only listening
    pub request: String,
    /// every step with its result, in order
    pub steps: Vec<String>,
    pub notifications: Vec<Notification>,
    pub error: String,
}

/// The vendor GATT service the Naga Pro offers over Bluetooth (with 1524 write, 1525/1526 notify).
pub const RAZER_SERVICE: &str = "52401523-f97c-7f90-0e7f-6c6f4e36db1c";

/// Parse "1f 00 81", "1f0081", "0x1f,0x00" into bytes.
pub fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let clean: String = s.replace("0x", " ").replace("0X", " ").chars().map(|c| if c.is_ascii_hexdigit() { c } else { ' ' }).collect();
    let mut out = Vec::new();
    for tok in clean.split_whitespace() {
        if tok.len() % 2 != 0 {
            return Err(format!("odd number of hex digits in “{tok}”"));
        }
        for i in (0..tok.len()).step_by(2) {
            out.push(u8::from_str_radix(&tok[i..i + 2], 16).map_err(|e| e.to_string())?);
        }
    }
    if out.len() > 512 {
        return Err("at most 512 bytes".into());
    }
    Ok(out)
}

/// A list of bytes in hex with ranges: "0b", "00-1f", "80-9f, a0" (at most 256 values).
pub fn parse_byte_list(s: &str) -> Result<Vec<u8>, String> {
    let mut v = Vec::new();
    for part in s.split([',', ' ', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let byte = |x: &str| u8::from_str_radix(x.trim().trim_start_matches("0x"), 16).map_err(|_| format!("not a hex byte: “{x}”"));
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (byte(a)?, byte(b)?);
                if a > b {
                    return Err(format!("range “{part}” goes backwards"));
                }
                v.extend(a..=b);
            }
            None => v.push(byte(part)?),
        }
    }
    v.dedup();
    if v.is_empty() || v.len() > 256 {
        return Err("give 1..256 values".into());
    }
    Ok(v)
}

/// Bluetooth address of the first Razer device connected over Bluetooth.
fn bt_address() -> Option<(String, u16)> {
    crate::transport::enumerate(false).into_iter().filter(|d| d.bluetooth).find_map(|d| address_from_path(&d.path).map(|a| (a, d.pid)))
}

/// Talk to the Razer GATT service: subscribe to its notifying characteristics, write `request`
/// ("" = only listen, "fw" = the read-only firmware request in the USB format, or hex bytes)
/// to its writable characteristic and collect what comes back during `listen_ms`.
pub fn gatt_exchange(request: &str, listen_ms: u32) -> Exchange {
    let mut x = Exchange::default();
    let Some((address, pid)) = bt_address() else {
        x.error = "No Razer device is connected over Bluetooth.".into();
        return x;
    };
    let bytes = match request.trim() {
        "" => Vec::new(),
        "fw" => {
            let tid = crate::devices::spec_for(pid).map(|s| s.transaction_id).unwrap_or(0x1F);
            firmware_request(tid).to_vec()
        }
        h => match parse_hex(h) {
            Ok(b) => b,
            Err(e) => {
                x.error = e;
                return x;
            }
        },
    };
    x.request = hex(&bytes, 512);
    log_info!("app", "BT GATT exchange with {address}: write [{}], listen {listen_ms} ms", x.request);
    let r = with_session(&address, |s| {
        let mut ok = true;
        if !bytes.is_empty() {
            let r = s.write(&bytes);
            ok = r.is_ok() || r.as_ref().is_err_and(|e| e.contains("ATT"));
            s.steps.push(format!("write {} bytes to 52401524-f97c-7f90-0e7f-6c6f4e36db1c: {}", bytes.len(), r.err().unwrap_or_else(|| "OK".into())));
        }
        std::thread::sleep(std::time::Duration::from_millis(listen_ms.clamp(100, 10_000) as u64));
        let mut notes = Vec::new();
        for (t, handle, uuid, b) in s.take() {
            notes.push(Notification { t_ms: t.saturating_duration_since(s.start).as_millis() as u64, handle, uuid, hex: hex(&b, 128), len: b.len() });
        }
        let reads = s.read_all();
        let mut steps = std::mem::take(&mut s.steps);
        steps.extend(reads);
        ((steps, notes), ok)
    });
    match r {
        Ok((steps, notes)) => {
            x.steps = steps;
            x.notifications = notes;
        }
        Err((e, steps)) => {
            x.steps = steps;
            x.error = e;
        }
    }
    for s in &x.steps {
        log_info!("app", "BT GATT   {s}");
    }
    for n in &x.notifications {
        log_info!("app", "BT GATT   notify +{} ms {} (handle {}) {} bytes: {}", n.t_ms, n.uuid, n.handle, n.len, n.hex);
    }
    if !x.error.is_empty() {
        log_info!("app", "BT GATT   error: {}", x.error);
    }
    x
}

/// One subscribed session to the Razer service, kept open and shared by the connected mouse
/// (`BleBridge`) and Diagnostics; the mutex also makes the conversations one at a time (the
/// notification queue is shared).
static SESSION: std::sync::Mutex<Option<(String, imp::Session)>> = std::sync::Mutex::new(None);

/// Run `f` on the open session to `address` (opened first if needed). `f` returns whether the
/// session still works; a broken one is closed, and the next call opens a new one.
fn with_session<T>(address: &str, f: impl FnOnce(&mut imp::Session) -> (T, bool)) -> Result<T, (String, Vec<String>)> {
    let mut g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if g.as_ref().is_some_and(|(a, _)| a != address) {
        *g = None;
    }
    let mut opened = Vec::new();
    if g.is_none() {
        let s = imp::Session::open(address)?;
        opened = s.steps.clone();
        log_info!("app", "BT: session to the Razer service of {address} opened");
        *g = Some((address.to_string(), s));
    }
    let Some((_, s)) = g.as_mut() else {
        return Err(("no session".into(), opened));
    };
    s.steps = opened;
    let (v, ok) = f(s);
    if !ok {
        log_info!("app", "BT: session closed (the mouse stopped answering)");
        *g = None;
    }
    Ok(v)
}

/// Close the shared session (the mouse was disconnected).
pub fn close_session() {
    *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Request numbers of the Bluetooth protocol: 1..=255, never 0, different for each request.
fn next_tid() -> u8 {
    static TID: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0x10);
    loop {
        let t = TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if t != 0 {
            return t;
        }
    }
}

/// One "get" command over Bluetooth and its reply.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct BleResult {
    pub class: u8,
    pub cmd: u8,
    /// sub1 sub2 in hex
    pub sub: String,
    /// 0 = no reply
    pub status: u8,
    pub status_name: String,
    pub len: usize,
    pub data: String,
    /// what the data means, for known commands
    pub meaning: String,
    pub ms: u64,
    pub error: String,
    /// the data as bytes (for the bridge)
    #[cfg_attr(feature = "json", serde(skip))]
    pub raw: Vec<u8>,
}

/// Send one request on an open session and wait for its reply (at most `timeout_ms`).
fn ble_ask(s: &mut imp::Session, class: u8, cmd: u8, sub: [u8; 2], timeout_ms: u64) -> BleResult {
    // BUSY (01): the mouse is still doing something — ask again, a few times
    let mut r = ble_ask_once(s, class, cmd, sub, timeout_ms);
    for _ in 0..3 {
        if r.status != crate::protocol::status::BUSY {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
        let ms = r.ms;
        r = ble_ask_once(s, class, cmd, sub, timeout_ms);
        r.ms += ms + 150;
    }
    r
}

fn ble_ask_once(s: &mut imp::Session, class: u8, cmd: u8, sub: [u8; 2], timeout_ms: u64) -> BleResult {
    ble_send(s, class, cmd, sub, &[], false, timeout_ms)
}

/// Send a request (with `payload` for a "set": in a second write after the header, or in the
/// same write when `combined`) and wait for its reply.
fn ble_send(s: &mut imp::Session, class: u8, cmd: u8, sub: [u8; 2], payload: &[u8], combined: bool, timeout_ms: u64) -> BleResult {
    let mut r = BleResult { class, cmd, sub: hex(&sub, 2), ..Default::default() };
    let tid = next_tid();
    let t0 = std::time::Instant::now();
    s.take(); // drop what is left of earlier requests
    let head = crate::ble::header(tid, payload.len() as u8, class, cmd, sub);
    let writes: Vec<Vec<u8>> = if payload.is_empty() {
        vec![head.to_vec()]
    } else if combined {
        vec![[&head[..], payload].concat()]
    } else {
        vec![head.to_vec(), payload.to_vec()]
    };
    for w in &writes {
        if let Err(e) = s.write(w) {
            r.error = format!("write [{}]: {e}", hex(w, 32));
            r.ms = t0.elapsed().as_millis() as u64;
            return r;
        }
    }
    let mut notes: Vec<Vec<u8>> = Vec::new();
    let mut seen = Vec::new();
    // long replies (480 bytes = 24 notifications) keep the request open while data still comes
    let mut last = t0;
    let hard = std::time::Duration::from_millis(5000);
    loop {
        // replies come on …1525; …1526 sends something else (8 bytes)
        for n in s.take() {
            last = std::time::Instant::now();
            seen.push(format!("{} {}", if n.2.is_empty() { format!("handle {}", n.1) } else { n.2.chars().take(8).collect::<String>() }, hex(&n.3, 20)));
            if !n.2.starts_with("52401526") {
                notes.push(n.3);
            }
        }
        let quiet = last.elapsed().as_millis() as u64 >= 300 && t0.elapsed().as_millis() as u64 >= timeout_ms;
        let over = quiet || t0.elapsed() >= hard;
        if let Some(rep) = crate::ble::assemble(&notes, tid) {
            if rep.complete() || over {
                r.status = rep.status;
                r.status_name = rep.status_name().into();
                r.len = rep.len;
                r.data = hex(&rep.data, 4096);
                r.raw = rep.data.clone();
                r.meaning = crate::ble::describe(class, cmd, sub, &rep);
                if !rep.complete() {
                    r.error = format!("only {} of {} bytes arrived", rep.data.len(), rep.len);
                }
                break;
            }
        } else if over {
            seen.truncate(6);
            r.error = if seen.is_empty() { "no reply: no notifications".into() } else { format!("no reply to request {tid:02x}; got: {}", seen.join(" | ")) };
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    r.ms = t0.elapsed().as_millis() as u64;
    r
}

/// Send "get" commands over Bluetooth: every class × command × sub given (commands without
/// bit 7 — "set" — are refused, so nothing in the mouse can change).
pub fn ble_get(classes: &[u8], cmds: &[u8], subs: &[[u8; 2]]) -> Result<Vec<BleResult>, String> {
    if let Some(c) = cmds.iter().find(|c| !crate::ble::is_get(**c)) {
        return Err(format!("command {c:02x} is not a “get” (bit 7 not set): refused"));
    }
    let (address, _) = bt_address().ok_or("No Razer device is connected over Bluetooth.")?;
    with_session(&address, |s| {
        let out = ble_get_on(s, classes, cmds, subs);
        let ok = !out.last().is_some_and(|r| r.status == 0 && !r.error.is_empty());
        (out, ok)
    })
    .map_err(|(e, _)| e)
}

fn ble_get_on(s: &mut imp::Session, classes: &[u8], cmds: &[u8], subs: &[[u8; 2]]) -> Vec<BleResult> {
    let mut out = Vec::new();
    let mut silent = 0;
    'all: for &class in classes {
        for &cmd in cmds {
            for &sub in subs {
                let r = ble_ask(s, class, cmd, sub, 400);
                log_info!(
                    "app",
                    "BT get {class:02x} {cmd:02x} {}: {} {} [{}] {}{}",
                    r.sub,
                    if r.status == 0 { "—".to_string() } else { format!("{:02x}", r.status) },
                    r.status_name,
                    r.data,
                    r.meaning,
                    if r.error.is_empty() { String::new() } else { format!(" ({})", r.error) }
                );
                silent = if r.status == 0 { silent + 1 } else { 0 };
                out.push(r);
                if silent >= 5 {
                    log_warn!("app", "BT get: the mouse stopped answering after {} requests", out.len());
                    out.push(BleResult { error: "the mouse stopped answering — stopped here".into(), ..Default::default() });
                    break 'all;
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------ write self-test

/// One step of the Bluetooth write self-test (Diagnostics → Wireless).
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct TestReport {
    pub ok: bool,
    /// the way of writing that worked (see [`WRITE_WAYS`]), for the next steps
    pub way: Option<u8>,
    /// "button_change": the record read back equals the one written
    pub same: Option<bool>,
    pub lines: Vec<String>,
}

/// Ways to send a "set", tried in this order: (sub1 of the button commands, combined in one write).
pub const WRITE_WAYS: [(u8, bool); 4] = [(0, false), (1, false), (0, true), (1, true)];

/// The test button: side button 12 (12-button plate) — "=" by default.
pub const TEST_BUTTON: u8 = 0x4b;

/// The binding of the test button before step 3, to put back.
static ORIGINAL: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

/// The action "key 7" (HID 0x24): a digit types the same on every keyboard layout.
const KEY_7: [u8; 7] = [0x02, 0x02, 0x00, 0x24, 0, 0, 0];

/// The test button in profiles 0..=5 as read by the "targets" step, to put back.
static ORIG_T: std::sync::Mutex<[Option<Vec<u8>>; 6]> = std::sync::Mutex::new([None, None, None, None, None, None]);

/// Data of a button "set" into profile `tg`: profile, button, layer (0 normal), 7-byte action.
pub fn target_payload(tg: u8, btn: u8, layer: u8, block: &[u8]) -> Vec<u8> {
    [&[tg, btn, layer][..], &pad7(block)].concat()
}

/// "normal key 09 · Hypershift mouse 01" for a 16-byte button record.
fn target_text(rec: &[u8]) -> String {
    match crate::ble::binding(rec) {
        Some((_, [n, h])) => format!("normal {} · Hypershift {} [{}]", crate::ble::action_text(&n), crate::ble::action_text(&h), hex(rec, 16)),
        None => hex(rec, 16),
    }
}

fn line(t: &mut TestReport, r: &BleResult, what: &str) {
    let st = if r.status == 0 { "no reply".to_string() } else { format!("{:02x} {}", r.status, r.status_name) };
    let msg = format!(
        "{what}: {:02x} {:02x} sub {} → {st}{}{}",
        r.class,
        r.cmd,
        r.sub,
        if r.data.is_empty() { String::new() } else { format!(" [{}]", r.data) },
        if r.error.is_empty() { String::new() } else { format!(" ({})", r.error) }
    );
    log_info!("app", "BT self-test {msg}");
    t.lines.push(msg);
}

fn note(t: &mut TestReport, msg: String) {
    log_info!("app", "BT self-test {msg}");
    t.lines.push(msg);
}

/// Binding record of one button over Bluetooth (16 bytes).
fn read_button(s: &mut imp::Session, t: &mut TestReport, id: u8) -> Option<Vec<u8>> {
    let r = ble_ask(s, 0x08, 0x84, [0, id], 600);
    line(t, &r, "read button");
    (r.status == crate::protocol::status::OK && r.raw.len() >= 16).then(|| r.raw[..16].to_vec())
}

/// Write a button record with way `w`; true when the mouse answered OK.
fn write_button(s: &mut imp::Session, t: &mut TestReport, rec: &[u8], w: u8) -> bool {
    let (sub1, combined) = WRITE_WAYS[w as usize % WRITE_WAYS.len()];
    let r = ble_send(s, 0x08, 0x04, [sub1, rec[0]], rec, combined, 600);
    line(t, &r, &format!("write button (way {w}: sub1 {sub1:02x}, {})", if combined { "one write" } else { "header + data" }));
    r.status == crate::protocol::status::OK
}

/// The test button with its normal action replaced by the key "F" (Hypershift kept).
pub fn with_key_f(rec: &[u8]) -> Vec<u8> {
    let mut v = rec.to_vec();
    v.resize(16, 0);
    v[2] = 0x02; // keyboard
    v[4] = 0x02; // 2 bytes: modifiers, key
    for i in 0..5 {
        v[6 + 2 * i] = 0;
    }
    v[8] = 0x09; // F
    v
}

/// Ways to lay out a button "set" (stage 3 of the self-test tries them one by one).
/// `n`, `h`: the normal and the Hypershift record `[type, len, data…]` padded to 7 bytes.
pub const BUTTON_LAYOUTS: &[&str] = &[
    "interleaved 16 bytes, sub 00 <btn>",
    "normal record only (7 bytes), sub 00 <btn>",
    "button, layer 00, record (9 bytes), sub 00 <btn>",
    "USB arguments: profile 01, button, layer 00, record (10 bytes), sub 01 <btn>",
    "interleaved 16 bytes, sub 00 00",
    "interleaved 16 bytes, sub <btn> 00",
    "interleaved without button and 01 (14 bytes), sub 00 <btn>",
    "button, record (8 bytes), sub 00 00",
];

fn pad7(v: &[u8]) -> Vec<u8> {
    let mut r = v.iter().copied().take(7).collect::<Vec<_>>();
    r.resize(7, 0);
    r
}

/// The interleaved 16-byte record of `btn` from its two layers.
pub fn interleave(btn: u8, n: &[u8], h: &[u8]) -> Vec<u8> {
    let (n, h) = (pad7(n), pad7(h));
    let mut v = vec![btn, 1, n[0], h[0], n[1], h[1]];
    for i in 0..5 {
        v.push(n[2 + i]);
        v.push(h[2 + i]);
    }
    v
}

/// (sub, payload) of layout `l` for setting `btn` to normal `n`, Hypershift `h`.
pub fn button_layout(l: usize, btn: u8, n: &[u8], h: &[u8]) -> ([u8; 2], Vec<u8>) {
    let il = interleave(btn, n, h);
    match l {
        1 => ([0, btn], pad7(n)),
        2 => ([0, btn], [&[btn, 0][..], &pad7(n)].concat()),
        3 => ([1, btn], [&[1, btn, 0][..], &pad7(n)].concat()),
        4 => ([0, 0], il),
        5 => ([btn, 0], il),
        6 => ([0, btn], il[2..].to_vec()),
        7 => ([0, 0], [&[btn][..], &pad7(n)].concat()),
        _ => ([0, btn], il),
    }
}

/// Run one step: "sleep" (write the sleep timer, read it back, put it back), "button_same"
/// (write the test button's own binding back), "button_change" (normal action → key F),
/// "button_restore". `way` = the way that worked in "button_same".
pub fn ble_selftest(step: &str, way: Option<u8>, value: &str) -> Result<TestReport, String> {
    let (address, _) = bt_address().ok_or("No Razer device is connected over Bluetooth.")?;
    log_info!("app", "BT self-test: {step}");
    with_session(&address, |s| {
        let mut t = TestReport::default();
        match step {
            "sleep" => {
                let r = ble_ask(s, 0x05, 0x84, [0, 0], 600);
                line(&mut t, &r, "read sleep timer");
                let Some(orig) = (r.status == 2 && r.raw.len() >= 2).then(|| u16::from_le_bytes([r.raw[0], r.raw[1]])) else {
                    note(&mut t, "cannot read the sleep timer — stopped".into());
                    return (t, true);
                };
                let new: u16 = if orig == 360 { 300 } else { 360 };
                note(&mut t, format!("sleep timer {orig} s; trying to set {new} s"));
                for (w, &(_, combined)) in WRITE_WAYS.iter().enumerate().filter(|(_, x)| x.0 == 0) {
                    let r = ble_send(s, 0x05, 0x04, [0, 0], &new.to_le_bytes(), combined, 600);
                    line(&mut t, &r, &format!("set sleep timer (way {w}: {})", if combined { "one write" } else { "header + data" }));
                    let back = ble_ask(s, 0x05, 0x84, [0, 0], 600);
                    line(&mut t, &back, "read back");
                    let now = (back.raw.len() >= 2).then(|| u16::from_le_bytes([back.raw[0], back.raw[1]]));
                    if now == Some(new) {
                        note(&mut t, format!("✓ the mouse took {new} s (way {w}); putting {orig} s back"));
                        let r = ble_send(s, 0x05, 0x04, [0, 0], &orig.to_le_bytes(), combined, 600);
                        line(&mut t, &r, "set sleep timer back");
                        let back = ble_ask(s, 0x05, 0x84, [0, 0], 600);
                        line(&mut t, &back, "read back");
                        let now = (back.raw.len() >= 2).then(|| u16::from_le_bytes([back.raw[0], back.raw[1]]));
                        t.ok = now == Some(orig);
                        t.way = Some(w as u8);
                        let ok = t.ok;
                        note(&mut t, if ok { format!("✓ {orig} s again") } else { format!("✗ the timer is now {now:?} s, not {orig} s — set it by cable") });
                        return (t, true);
                    }
                }
                note(&mut t, "✗ no way of writing changed the sleep timer".into());
            }
            "button_same" => {
                let Some(rec) = read_button(s, &mut t, TEST_BUTTON) else {
                    note(&mut t, "cannot read the test button — stopped".into());
                    return (t, true);
                };
                note(&mut t, format!("test button {TEST_BUTTON:02x}: {}", crate::ble::describe(0x08, 0x84, [0, TEST_BUTTON], &crate::ble::Reply { tid: 0, status: 2, len: 16, data: rec.clone() })));
                for w in 0..WRITE_WAYS.len() as u8 {
                    if write_button(s, &mut t, &rec, w) {
                        let back = read_button(s, &mut t, TEST_BUTTON);
                        t.ok = back.as_deref() == Some(&rec[..]);
                        t.way = Some(w);
                        let ok = t.ok;
                        note(&mut t, if ok { format!("✓ the mouse takes button records (way {w}); the binding is unchanged") } else { format!("✗ OK, but the binding reads back differently: {}", hex(&back.unwrap_or_default(), 16)) });
                        if !t.ok {
                            write_button(s, &mut t, &rec, w);
                        }
                        return (t, true);
                    }
                }
                note(&mut t, "✗ the mouse refused every way of writing a button".into());
            }
            "button_change" => {
                // the change may go to the live (unsaved) layer while reading shows the stored
                // profile: the user presses the button whatever reads back
                let w = way.unwrap_or(0);
                let Some(rec) = read_button(s, &mut t, TEST_BUTTON) else {
                    note(&mut t, "cannot read the test button — stopped".into());
                    return (t, true);
                };
                let mut orig = ORIGINAL.lock().unwrap_or_else(|e| e.into_inner());
                if orig.is_none() {
                    *orig = Some(rec.clone());
                }
                drop(orig);
                let f = with_key_f(&rec);
                note(&mut t, format!("setting the normal action of button {TEST_BUTTON:02x} to the key F (way {w}): {}", hex(&f, 16)));
                t.way = Some(w);
                if write_button(s, &mut t, &f, w) {
                    t.ok = true;
                    let back = read_button(s, &mut t, TEST_BUTTON);
                    let same = back.as_deref() == Some(&f[..]);
                    t.same = Some(same);
                    note(&mut t, if same { "reads back as written".into() } else { "reads back unchanged (the change may be live only)".into() });
                }
            }
            "button_restore" => {
                // put the original back with every way of writing that was used
                let Some(orig) = ORIGINAL.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
                    note(&mut t, "nothing to restore".into());
                    t.ok = true;
                    return (t, true);
                };
                for w in [0u8, 1] {
                    write_button(s, &mut t, &orig, w);
                }
                let back = read_button(s, &mut t, TEST_BUTTON);
                t.ok = back.as_deref() == Some(&orig[..]);
                let ok = t.ok;
                note(&mut t, if ok { "✓ the stored binding is as before — press the button to check that it types = again".into() } else { format!("✗ not restored — set side button 12 by cable (was {})", hex(&orig, 16)) });
                if ok {
                    *ORIGINAL.lock().unwrap_or_else(|e| e.into_inner()) = None;
                }
            }
            "button_layout" => {
                // stage 3: layout `way` — set the normal action to F ("f") or back ("orig")
                let l = way.unwrap_or(0) as usize;
                let Some(rec) = read_button(s, &mut t, TEST_BUTTON) else {
                    note(&mut t, "cannot read the test button — stopped".into());
                    return (t, true);
                };
                let mut g = ORIGINAL.lock().unwrap_or_else(|e| e.into_inner());
                let orig = g.get_or_insert_with(|| rec.clone()).clone();
                drop(g);
                let Some((_, [n0, h0])) = crate::ble::binding(&orig) else {
                    note(&mut t, "cannot decode the test button".into());
                    return (t, true);
                };
                let n = if value == "orig" { n0.clone() } else { vec![0x02, 0x02, 0x00, 0x09] };
                let (sub, payload) = button_layout(l, TEST_BUTTON, &n, &h0);
                let r = ble_send(s, 0x08, 0x04, sub, &payload, false, 600);
                line(&mut t, &r, &format!("layout {l} ({}) {} [{}]", BUTTON_LAYOUTS.get(l).unwrap_or(&"?"), if value == "orig" { "back to the original" } else { "→ F" }, hex(&payload, 16)));
                t.ok = r.status == crate::protocol::status::OK;
                t.way = Some(l as u8);
                let back = read_button(s, &mut t, TEST_BUTTON);
                t.same = Some(back.as_deref() == Some(&orig[..]));
            }
            "targets" => {
                // stage 2: which profile ("target") the mouse works by, and the test button in
                // each one. Target 0 mirrors the active profile and cannot be written; 1 is the
                // live layer; 2..5 are stored (the layout found by OpenSnek on Basilisk mice).
                let list = ble_ask(s, 0x03, 0x80, [0, 0], 600);
                line(&mut t, &list, "profile list");
                let act = ble_ask(s, 0x03, 0x82, [0, 0], 600);
                line(&mut t, &act, "active profile");
                let active = (act.status == crate::protocol::status::OK).then(|| act.raw.first().copied()).flatten();
                let mut g = ORIG_T.lock().unwrap_or_else(|e| e.into_inner());
                for tg in 0..=5u8 {
                    let r = ble_ask(s, 0x08, 0x84, [tg, TEST_BUTTON], 600);
                    line(&mut t, &r, &format!("test button in profile {tg}"));
                    if r.status == crate::protocol::status::OK && r.raw.len() >= 16 {
                        let rec = r.raw[..16].to_vec();
                        note(&mut t, format!("  profile {tg}: {}", target_text(&rec)));
                        // read anew on every run: the binding may have been changed since
                        g[tg as usize] = Some(rec);
                    }
                }
                drop(g);
                t.way = active;
                t.ok = active.is_some();
                note(&mut t, match active {
                    Some(a) => format!("the mouse works by profile {a}"),
                    None => "cannot read the active profile".into(),
                });
            }
            "target_write" => {
                // stage 3: write the test button into profile `way` the way Synapse does on
                // Basilisk mice: key 08 04 <profile> <button>, data <profile> <button> <layer>
                // <7-byte action>; "f" sets the normal action to the key 7 (a digit: the same on every
                // keyboard layout), "orig" puts it back; "hs_f" / "hs_orig" the same in the
                // Hypershift layer (layer byte 01, stage 4)
                let (layer, restore) = match value {
                    "orig" => (0u8, true),
                    "hs_f" => (1, false),
                    "hs_orig" => (1, true),
                    _ => (0, false),
                };
                let lane = if layer == 1 { "Hypershift" } else { "normal" };
                let tg = way.unwrap_or(1);
                if tg == 0 || tg > 5 {
                    note(&mut t, format!("profile {tg} cannot be written"));
                    return (t, true);
                }
                let rec = {
                    let g = ORIG_T.lock().unwrap_or_else(|e| e.into_inner());
                    g[tg as usize].clone()
                };
                let rec = match rec {
                    Some(r) => r,
                    None => {
                        let r = ble_ask(s, 0x08, 0x84, [tg, TEST_BUTTON], 600);
                        line(&mut t, &r, &format!("read test button in profile {tg}"));
                        if r.status != crate::protocol::status::OK || r.raw.len() < 16 {
                            note(&mut t, "cannot read the test button — stopped".into());
                            return (t, true);
                        }
                        let v = r.raw[..16].to_vec();
                        ORIG_T.lock().unwrap_or_else(|e| e.into_inner())[tg as usize] = Some(v.clone());
                        v
                    }
                };
                let Some((_, orig)) = crate::ble::binding(&rec) else {
                    note(&mut t, "cannot decode the test button".into());
                    return (t, true);
                };
                let other = pad7(&orig[1 - layer as usize]);
                let block = if restore { pad7(&orig[layer as usize]) } else { KEY_7.to_vec() };
                let payload = target_payload(tg, TEST_BUTTON, layer, &block);
                let r = ble_send(s, 0x08, 0x04, [tg, TEST_BUTTON], &payload, false, 600);
                line(&mut t, &r, &format!("write profile {tg} {lane} {} [{}]", if restore { "back to the original" } else { "→ key 7" }, hex(&payload, 16)));
                t.ok = r.status == crate::protocol::status::OK;
                t.way = Some(tg);
                let back = ble_ask(s, 0x08, 0x84, [tg, TEST_BUTTON], 600);
                line(&mut t, &back, &format!("read back profile {tg}"));
                let lanes = (back.raw.len() >= 16).then(|| crate::ble::binding(&back.raw[..16])).flatten().map(|(_, l)| [pad7(&l[0]), pad7(&l[1])]);
                // written as asked, and the other layer untouched
                t.same = Some(lanes.as_ref().is_some_and(|l| l[layer as usize] == block && l[1 - layer as usize] == other));
                if let Some(rec) = (back.raw.len() >= 16).then(|| back.raw[..16].to_vec()) {
                    note(&mut t, format!("  profile {tg} now: {}", target_text(&rec)));
                }
                let mirror = ble_ask(s, 0x08, 0x84, [0, TEST_BUTTON], 600);
                line(&mut t, &mirror, "read back profile 0 (active)");
            }
            "read_target" => {
                // stage 5, after the mouse was switched off and on: is the key 7 still there?
                let tg = way.unwrap_or(1);
                let r = ble_ask(s, 0x08, 0x84, [tg, TEST_BUTTON], 1000);
                line(&mut t, &r, &format!("read test button in profile {tg}"));
                if r.status == 0 {
                    // no reply: the mouse is still reconnecting; the next call opens a new session
                    return (t, false);
                }
                t.ok = r.status == crate::protocol::status::OK;
                t.way = Some(tg);
                if let Some(rec) = (r.raw.len() >= 16).then(|| r.raw[..16].to_vec()) {
                    note(&mut t, format!("  profile {tg}: {}", target_text(&rec)));
                    t.same = crate::ble::binding(&rec).map(|(_, l)| pad7(&l[0]) == KEY_7);
                }
            }
            _ => note(&mut t, format!("unknown step {step}")),
        }
        (t, true)
    })
    .map_err(|(e, _)| e)
}

// ------------------------------------------------------------------ the mouse over Bluetooth

/// Features open-rzr can read over Bluetooth.
pub const BLE_FEATURES: u32 = {
    use crate::devices::feat::*;
    FIRMWARE | SERIAL | DPI | DPI_STAGES | BATTERY | CHARGING | IDLE | LOW_BATTERY | LIGHTING | BUTTONS
};

/// Of those, the ones that can also be changed over Bluetooth (checked on a Naga Pro: button
/// bindings in the mouse's slots, the sleep timer).
pub const BLE_WRITES: u32 = {
    use crate::devices::feat::*;
    BUTTONS | IDLE
};

/// Why a "set" is refused over Bluetooth.
pub const BLE_READ_ONLY: &str =
    "over Bluetooth open-rzr can change only button bindings and the sleep timer so far — connect the mouse by cable or through the HyperSpeed receiver to change this";

/// The usual 90-byte requests of `RazerMouse`, answered over the Razer GATT service: each "get"
/// with a known Bluetooth counterpart is asked there and its reply rewritten into the USB form.
/// Of the "set" requests, button bindings (`08 04 <profile> <button>`, data `<profile> <button>
/// <layer> <action, 7 bytes>`) and the sleep timer (`05 04`) are written; the others are refused.
/// See `crate::ble` for the Bluetooth commands.
pub struct BleBridge {
    address: String,
    reply: Option<[u8; crate::protocol::REPORT_LEN]>,
    /// all bindings of one profile (`08 83 <profile> 00`): profile, when read, 16-byte records
    /// (`None`: the bulk read cannot be used for that profile — buttons are read one by one)
    bulk: Option<(u8, std::time::Instant, Option<Records>)>,
}

/// 16-byte button records of one profile.
type Records = Vec<Vec<u8>>;

impl BleBridge {
    pub fn new(address: &str) -> BleBridge {
        BleBridge { address: address.to_string(), reply: None, bulk: None }
    }

    /// One request on the shared session; no reply closes it (the next request opens a new one).
    fn ask(&self, f: impl FnOnce(&mut imp::Session) -> BleResult) -> crate::protocol::Result<BleResult> {
        with_session(&self.address, |s| {
            let r = f(s);
            let ok = r.status != 0;
            (r, ok)
        })
        .map_err(|(e, _)| crate::protocol::Error::Io(format!("Bluetooth: {e}")))
    }

    /// The bindings of `profile` read at once, if that works for this profile: the bulk reply is
    /// checked against its last button read alone, so a bulk read that ignores the profile is
    /// never taken for another profile's bindings. Kept 3 s (a scan asks every button id).
    fn bulk_records(&mut self, profile: u8) -> crate::protocol::Result<Option<Records>> {
        if let Some((p, t, recs)) = &self.bulk {
            if *p == profile && t.elapsed() < std::time::Duration::from_secs(3) {
                return Ok(recs.clone());
            }
        }
        let r = self.ask(|s| ble_ask(s, 0x08, 0x83, [profile, 0], 1500))?;
        let recs: Option<Records> = (r.status == crate::protocol::status::OK && r.raw.len() >= 16)
            .then(|| r.raw.chunks_exact(16).filter(|c| c[0] != 0).map(<[u8]>::to_vec).collect());
        let recs = match recs {
            Some(v) if !v.is_empty() => {
                let last = v[v.len() - 1].clone();
                let one = self.ask(|s| ble_ask(s, 0x08, 0x84, [profile, last[0]], 600))?;
                let same = one.status == crate::protocol::status::OK && one.raw.get(..16) == Some(&last[..]);
                log_debug!("usb", "BT bindings of profile {profile} at once: {} records, {}", v.len(), if same { "checked" } else { "do not match one read alone — reading buttons one by one" });
                same.then_some(v)
            }
            _ => None,
        };
        self.bulk = Some((profile, std::time::Instant::now(), recs.clone()));
        Ok(recs)
    }

    /// A "set" over Bluetooth: (class, cmd, sub, data), or why it is refused.
    fn ble_set(usb: u16, args: &[u8]) -> Result<(u8, u8, [u8; 2], Vec<u8>), String> {
        use crate::protocol::cmd;
        let a = |i: usize| args.get(i).copied().unwrap_or(0);
        match usb {
            cmd::SET_BUTTON => {
                let (profile, button, layer) = (a(0), a(1), a(2));
                if profile == 0 {
                    // target 0 mirrors the active profile and cannot be written
                    return Err("over Bluetooth only the profiles in the mouse's slots can be written, not the temporary layer — put the profile into a slot (“In slot…”) or connect the mouse by cable".into());
                }
                Ok((0x08, 0x04, [profile, button], target_payload(profile, button, layer, &args[3.min(args.len())..args.len().min(10)])))
            }
            cmd::SET_IDLE_TIME => Ok((0x05, 0x04, [0, 0], u16::from_be_bytes([a(0), a(1)]).to_le_bytes().to_vec())),
            _ => Err(BLE_READ_ONLY.into()),
        }
    }
}

/// The Bluetooth command for a USB "get": (class, cmd, sub).
fn ble_for(usb: u16, args: &[u8]) -> Option<(u8, u8, [u8; 2])> {
    use crate::protocol::cmd;
    let a = |i: usize| args.get(i).copied().unwrap_or(0);
    Some(match usb {
        cmd::GET_FIRMWARE => (0x00, 0x81, [0, 0]),
        cmd::GET_SERIAL => (0x01, 0x83, [0, 0]),
        cmd::GET_DPI => (0x0b, 0x81, [0, 0]),
        cmd::GET_DPI_STAGES => (0x0b, 0x84, [a(0).max(1), 0]),
        cmd::GET_BATTERY => (0x05, 0x81, [0, 1]),
        cmd::GET_CHARGING => (0x05, 0x85, [0, 0]),
        cmd::GET_IDLE_TIME => (0x05, 0x84, [0, 0]),
        cmd::GET_LOW_BATTERY => (0x05, 0x82, [0, 0]),
        cmd::GET_BRIGHTNESS => (0x10, 0x85, [0, a(1)]),
        // profile 0 = the active one (read only)
        cmd::GET_BUTTON => (0x08, 0x84, [a(0), a(1)]),
        // over Bluetooth profiles are class 03: 03 82 the active one, 03 80 the list (01 02 on a
        // Naga Pro with two slots, numbered as over USB)
        cmd::GET_ACTIVE_PROFILE => (0x03, 0x82, [0, 0]),
        cmd::GET_PROFILE_COUNT | cmd::GET_PROFILE_LIST => (0x03, 0x80, [0, 0]),
        _ => return None,
    })
}

/// The USB reply arguments made from the Bluetooth data.
pub fn usb_args(usb: u16, args: &[u8], d: &[u8]) -> Option<Vec<u8>> {
    use crate::protocol::cmd;
    let a = |i: usize| args.get(i).copied().unwrap_or(0);
    let b = |i: usize| d.get(i).copied();
    let le = |i: usize| Some(u16::from_le_bytes([b(i)?, b(i + 1)?]));
    Some(match usb {
        cmd::GET_FIRMWARE => vec![b(0)?, b(1)?],
        cmd::GET_SERIAL => {
            let mut v = d.iter().copied().take(22).collect::<Vec<_>>();
            v.resize(22, 0);
            v
        }
        cmd::GET_DPI => {
            let mut v = vec![a(0)];
            v.extend(le(0)?.to_be_bytes());
            v.extend(le(2)?.to_be_bytes());
            v.extend([0, 0]);
            v
        }
        cmd::GET_DPI_STAGES => {
            let (active, n) = (b(0)?, b(1)?);
            let mut v = vec![a(0), active, n];
            for i in 0..n as usize {
                let o = 2 + i * 7;
                v.push(b(o)?);
                v.extend(le(o + 1)?.to_be_bytes());
                v.extend(le(o + 3)?.to_be_bytes());
                v.extend([0, 0]);
            }
            v
        }
        cmd::GET_BATTERY | cmd::GET_CHARGING => vec![0, b(0)?],
        cmd::GET_IDLE_TIME => le(0)?.to_be_bytes().to_vec(),
        cmd::GET_LOW_BATTERY | cmd::GET_ACTIVE_PROFILE => vec![b(0)?],
        cmd::GET_PROFILE_COUNT => vec![d.iter().filter(|p| **p != 0).count() as u8],
        cmd::GET_PROFILE_LIST => {
            let ids: Vec<u8> = d.iter().copied().filter(|p| *p != 0).collect();
            [&[ids.len() as u8][..], &ids].concat()
        }
        cmd::GET_BRIGHTNESS => vec![a(0), a(1), b(0)?],
        cmd::GET_BUTTON => {
            let (_, layers) = crate::ble::binding(d)?;
            let rec = &layers[(a(2) != 0) as usize];
            let mut v = vec![a(0), a(1), a(2)];
            v.extend(rec.iter().take(7));
            v.resize(10, 0);
            v
        }
        _ => return None,
    })
}

impl crate::transport::Transport for BleBridge {
    fn send_feature(&mut self, data: &[u8; crate::protocol::REPORT_LEN]) -> crate::protocol::Result<()> {
        use crate::protocol::{status, Error, Report};
        let req = Report::from_bytes(data);
        let usb = req.cmd();
        let args = req.arguments;
        let mut rep = Report::new(usb, &[], Some(req.data_size), req.transaction_id).map_err(|e| Error::Io(e.to_string()))?;
        rep.status = status::NOT_SUPPORTED;
        if !crate::ble::is_get(req.command_id) {
            let (class, cmd, sub, payload) = Self::ble_set(usb, &args).map_err(Error::Unsupported)?;
            let r = self.ask(|s| ble_send(s, class, cmd, sub, &payload, false, 800))?;
            log_debug!("usb", "BT set {class:02x} {cmd:02x} {} [{}] -> {}{}", r.sub, hex(&payload, 16), r.status_name, if r.error.is_empty() { String::new() } else { format!(" ({})", r.error) });
            // the cached bindings are stale now
            self.bulk = None;
            rep.status = if r.status == 0 { status::TIMEOUT } else { r.status };
            rep.arguments = args;
            self.reply = Some(rep.to_bytes());
            return Ok(());
        }
        if usb == crate::protocol::cmd::GET_BUTTON {
            // from the bulk read when it can be used: one request for a whole scan
            if let Some(recs) = self.bulk_records(args[0])? {
                rep.status = match recs.iter().find(|r| r[0] == args[1]).and_then(|r| usb_args(usb, &args, r)) {
                    Some(v) => {
                        rep.arguments[..v.len()].copy_from_slice(&v);
                        rep.data_size = rep.data_size.max(v.len() as u8);
                        status::OK
                    }
                    // the mouse has no such button
                    None => status::FAIL,
                };
                self.reply = Some(rep.to_bytes());
                return Ok(());
            }
        }
        if let Some((class, cmd, sub)) = ble_for(usb, &args) {
            let r = with_session(&self.address, |s| {
                let r = ble_ask(s, class, cmd, sub, 400);
                let ok = r.status != 0;
                (r, ok)
            })
            .map_err(|(e, _)| Error::Io(format!("Bluetooth: {e}")))?;
            log_debug!("usb", "BT {class:02x} {cmd:02x} {} -> {} [{}]{}", r.sub, r.status_name, r.data, if r.error.is_empty() { String::new() } else { format!(" ({})", r.error) });
            rep.status = match r.status {
                // no reply: asleep or out of range (the channel then pauses its requests)
                0 => status::TIMEOUT,
                status::OK => match usb_args(usb, &args, &r.raw) {
                    Some(v) => {
                        rep.arguments[..v.len()].copy_from_slice(&v);
                        rep.data_size = rep.data_size.max(v.len() as u8);
                        status::OK
                    }
                    None => status::FAIL,
                },
                s => s,
            };
        }
        self.reply = Some(rep.to_bytes());
        Ok(())
    }

    fn get_feature(&mut self) -> crate::protocol::Result<[u8; crate::protocol::REPORT_LEN]> {
        self.reply.ok_or_else(|| crate::protocol::Error::Io("no request".into()))
    }
}

/// A button over Bluetooth: id, normal record, Hypershift record (`[type, len, data…]`).
pub type BleBinding = (u8, Vec<u8>, Vec<u8>);

/// Every binding of the active profile over Bluetooth.
pub fn ble_bindings(address: &str) -> Result<Vec<BleBinding>, String> {
    let r = with_session(address, |s| {
        let r = ble_ask(s, 0x08, 0x83, [1, 0], 400);
        let ok = r.status != 0;
        (r, ok)
    })
    .map_err(|(e, _)| e)?;
    if r.status != crate::protocol::status::OK {
        return Err(if r.error.is_empty() { format!("status {}", r.status_name) } else { r.error });
    }
    Ok(r.raw.chunks(16).filter_map(crate::ble::binding).map(|(b, [n, h])| (b, n, h)).collect())
}

/// Name of a standard 16-bit GATT UUID.
pub fn uuid_name(short: u16) -> &'static str {
    match short {
        0x1800 => "Generic Access",
        0x1801 => "Generic Attribute",
        0x180A => "Device Information",
        0x180F => "Battery",
        0x1812 => "Human Interface Device",
        0x1813 => "Scan Parameters",
        0x2A00 => "Device Name",
        0x2A01 => "Appearance",
        0x2A04 => "Connection Parameters",
        0x2A05 => "Service Changed",
        0x2A19 => "Battery Level",
        0x2A23 => "System ID",
        0x2A24 => "Model Number",
        0x2A25 => "Serial Number",
        0x2A26 => "Firmware Revision",
        0x2A27 => "Hardware Revision",
        0x2A28 => "Software Revision",
        0x2A29 => "Manufacturer Name",
        0x2A4A => "HID Information",
        0x2A4B => "Report Map",
        0x2A4C => "HID Control Point",
        0x2A4D => "Report",
        0x2A4E => "Protocol Mode",
        0x2A22 => "Boot Keyboard Input",
        0x2A32 => "Boot Keyboard Output",
        0x2A33 => "Boot Mouse Input",
        0x2A50 => "PnP ID",
        0x2AA6 => "Central Address Resolution",
        _ => "",
    }
}

/// 128-bit UUIDs of the Bluetooth base (0000xxxx-0000-1000-8000-00805f9b34fb) are standard.
pub fn short_of(uuid: &str) -> Option<u16> {
    let u = uuid.to_ascii_lowercase();
    if u.len() == 4 {
        return u16::from_str_radix(&u, 16).ok();
    }
    if u.len() == 36 && u.starts_with("0000") && u.ends_with("-0000-1000-8000-00805f9b34fb") {
        return u16::from_str_radix(&u[4..8], 16).ok();
    }
    None
}

pub fn name_of(uuid: &str) -> String {
    match short_of(uuid).map(uuid_name) {
        Some(n) if !n.is_empty() => n.to_string(),
        Some(_) => String::new(),
        None => "vendor-specific".into(),
    }
}

/// The 12-hex-digit Bluetooth address inside a Windows BLE HID path
/// ("…_PID&0092_REV&0001_eac88650b167&Col01#…").
pub fn address_from_path(path: &str) -> Option<String> {
    let l = path.to_ascii_lowercase();
    let i = l.find("_rev&")?;
    let rest = &l[i + 5..];
    let after = rest.split_once('_')?.1;
    let addr: String = after.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    (addr.len() == 12).then_some(addr)
}

pub fn hex(b: &[u8], max: usize) -> String {
    let mut s: Vec<String> = b.iter().take(max).map(|x| format!("{x:02x}")).collect();
    if b.len() > max {
        s.push(format!("… ({} bytes)", b.len()));
    }
    s.join(" ")
}

/// The request every probe sends: "firmware version" (0x0081), read-only.
pub fn firmware_request(tid: u8) -> [u8; crate::protocol::REPORT_LEN] {
    crate::protocol::Report::new(0x0081, &[0, 0], None, tid).map(|r| r.to_bytes()).unwrap_or([0; crate::protocol::REPORT_LEN])
}

/// Does a reply look like a Razer report answering 0x0081 (status 02 = OK, command 00 81)?
pub fn looks_like_razer(reply: &[u8]) -> bool {
    // the reply may carry a leading report id
    [0usize, 1].iter().any(|&o| reply.len() >= o + 8 && reply[o + 6] == 0x00 && reply[o + 7] == 0x81 && reply[o] != 0)
}

/// Collect everything about the Bluetooth endpoints of Razer devices. `probe`: "" (no probe),
/// "vendor" (vendor-defined collections only) or "all" (every collection that has feature or
/// output reports) — the read-only firmware request is sent.
pub fn report(probe: &str) -> Report {
    let mut r = Report::default();
    let all: Vec<_> = crate::transport::enumerate(false).into_iter().filter(|d| d.bluetooth).collect();
    if all.is_empty() {
        r.notes.push("No Razer device is connected over Bluetooth.".into());
        return r;
    }
    r.address = all.iter().find_map(|d| address_from_path(&d.path)).unwrap_or_default();
    for d in &all {
        let mut c = imp::caps(&d.path);
        c.path = d.path.clone();
        if c.usage_page == 0 {
            c.usage_page = d.usage_page;
            c.usage = d.usage;
        }
        log_info!(
            "app",
            "BT HID {:04x}:{:04x} page {:04x}/{:04x} in {} out {} feature {} ids in {:?} out {:?} feature {:?} {}",
            d.vid,
            d.pid,
            c.usage_page,
            c.usage,
            c.input_len,
            c.output_len,
            c.feature_len,
            c.input_ids,
            c.output_ids,
            c.feature_ids,
            c.error
        );
        r.hid.push(c);
    }
    if !probe.is_empty() {
        let tid = all.first().and_then(|d| crate::devices::spec_for(d.pid)).map(|s| s.transaction_id).unwrap_or(0x1F);
        let everything = probe == "all";
        for c in r.hid.iter().filter(|c| everything || c.usage_page >= 0xFF00) {
            for p in imp::probe(c, &firmware_request(tid)) {
                log_info!("app", "BT probe {} {}: {} {}", short_path(&p.path), p.method, if p.ok { "OK" } else { "—" }, p.detail);
                r.probes.push(p);
            }
        }
        if !everything && !r.hid.iter().any(|c| c.usage_page >= 0xFF00) {
            r.notes.push("No vendor-defined HID collection: nothing was probed (try “all collections”).".into());
        }
    }
    match imp::gatt(&r.address) {
        Ok(services) => {
            for s in &services {
                log_info!("app", "BT GATT service {} {} (handle {})", s.uuid, s.name, s.handle);
                for ch in &s.chars {
                    log_info!("app", "BT GATT   char {} {} [{}] {}", ch.uuid, ch.name, ch.props, ch.value);
                    if short_of(&ch.uuid) == Some(0x2A19) {
                        r.battery = ch.value.split(' ').next().and_then(|v| u8::from_str_radix(v, 16).ok());
                    }
                }
            }
            r.gatt = services;
        }
        Err(e) => {
            log_info!("app", "BT GATT: {e}");
            r.gatt_error = e;
        }
    }
    r
}

fn short_path(p: &str) -> String {
    p.split('#').nth(1).unwrap_or(p).chars().take(80).collect()
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::raw::c_void;

    type Handle = *mut c_void;
    const INVALID: isize = -1;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(name: *const u16, access: u32, share: u32, sec: *mut c_void, disp: u32, flags: u32, tmpl: Handle) -> Handle;
        fn CloseHandle(h: Handle) -> i32;
        fn GetLastError() -> u32;
    }

    #[link(name = "hid")]
    extern "system" {
        fn HidD_GetPreparsedData(h: Handle, pp: *mut isize) -> u8;
        fn HidD_FreePreparsedData(pp: isize) -> u8;
        fn HidP_GetCaps(pp: isize, caps: *mut [u16; 32]) -> i32;
        fn HidP_GetValueCaps(kind: i32, caps: *mut u8, len: *mut u16, pp: isize) -> i32;
        fn HidP_GetButtonCaps(kind: i32, caps: *mut u8, len: *mut u16, pp: isize) -> i32;
        fn HidD_SetFeature(h: Handle, buf: *mut c_void, len: u32) -> u8;
        fn HidD_GetFeature(h: Handle, buf: *mut c_void, len: u32) -> u8;
        fn HidD_SetOutputReport(h: Handle, buf: *mut c_void, len: u32) -> u8;
        fn HidD_GetInputReport(h: Handle, buf: *mut c_void, len: u32) -> u8;
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        d1: u32,
        d2: u16,
        d3: u16,
        d4: [u8; 8],
    }

    #[repr(C)]
    struct SpDeviceInterfaceData {
        size: u32,
        class: Guid,
        flags: u32,
        reserved: usize,
    }

    #[link(name = "setupapi")]
    extern "system" {
        fn SetupDiGetClassDevsW(class: *const Guid, enumerator: *const u16, parent: Handle, flags: u32) -> Handle;
        fn SetupDiEnumDeviceInterfaces(set: Handle, info: *mut c_void, class: *const Guid, index: u32, data: *mut SpDeviceInterfaceData) -> i32;
        fn SetupDiGetDeviceInterfaceDetailW(
            set: Handle,
            data: *mut SpDeviceInterfaceData,
            detail: *mut u8,
            size: u32,
            required: *mut u32,
            info: *mut c_void,
        ) -> i32;
        fn SetupDiDestroyDeviceInfoList(set: Handle) -> i32;
    }

    /// BTH_LE_UUID: BOOLEAN IsShortUuid; union { USHORT; GUID } (GUID aligned to 4)
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct BthLeUuid {
        is_short: u8,
        value: Guid,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct GattServiceRaw {
        uuid: BthLeUuid,
        handle: u16,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct GattCharRaw {
        service_handle: u16,
        uuid: BthLeUuid,
        handle: u16,
        value_handle: u16,
        broadcastable: u8,
        readable: u8,
        writable: u8,
        write_no_response: u8,
        signed_writable: u8,
        notifiable: u8,
        indicatable: u8,
        extended: u8,
    }

    #[link(name = "bluetoothapis")]
    extern "system" {
        fn BluetoothGATTGetServices(h: Handle, count: u16, buf: *mut GattServiceRaw, actual: *mut u16, flags: u32) -> i32;
        fn BluetoothGATTGetCharacteristics(h: Handle, service: *mut GattServiceRaw, count: u16, buf: *mut GattCharRaw, actual: *mut u16, flags: u32) -> i32;
        fn BluetoothGATTGetCharacteristicValue(h: Handle, ch: *mut GattCharRaw, size: u32, value: *mut u8, required: *mut u16, flags: u32) -> i32;
        fn BluetoothGATTSetCharacteristicValue(h: Handle, ch: *mut GattCharRaw, value: *mut u8, reliable: u64, flags: u32) -> i32;
        fn BluetoothGATTGetDescriptors(h: Handle, ch: *mut GattCharRaw, count: u16, buf: *mut GattDescRaw, actual: *mut u16, flags: u32) -> i32;
        fn BluetoothGATTSetDescriptorValue(h: Handle, d: *mut GattDescRaw, value: *mut u8, flags: u32) -> i32;
        fn BluetoothGATTRegisterEvent(
            h: Handle,
            kind: i32,
            param: *mut c_void,
            callback: extern "system" fn(i32, *mut c_void, *mut c_void),
            context: *mut c_void,
            event: *mut isize,
            flags: u32,
        ) -> i32;
        fn BluetoothGATTUnregisterEvent(event: isize, flags: u32) -> i32;
    }

    /// BTH_LE_GATT_DESCRIPTOR
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct GattDescRaw {
        service_handle: u16,
        char_handle: u16,
        kind: i32,
        uuid: BthLeUuid,
        handle: u16,
    }

    /// BLUETOOTH_GATT_VALUE_CHANGED_EVENT_REGISTRATION (room for 4 characteristics)
    #[repr(C)]
    struct EventRegistration {
        count: u16,
        chars: [GattCharRaw; 4],
    }

    /// BLUETOOTH_GATT_VALUE_CHANGED_EVENT
    #[repr(C)]
    struct ValueChanged {
        handle: u16,
        size: usize,
        value: *const u8,
    }

    /// Notifications collected by the callback: (time, attribute handle, bytes).
    static SINK: std::sync::Mutex<Vec<(std::time::Instant, u16, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

    extern "system" fn on_value(_kind: i32, out: *mut c_void, _ctx: *mut c_void) {
        if out.is_null() {
            return;
        }
        let bytes = unsafe {
            let ev = &*(out as *const ValueChanged);
            if ev.value.is_null() {
                Vec::new()
            } else {
                // BTH_LE_GATT_CHARACTERISTIC_VALUE: ULONG DataSize; UCHAR Data[]
                let n = (std::ptr::read_unaligned(ev.value as *const u32) as usize).min(1024);
                std::slice::from_raw_parts(ev.value.add(4), n).to_vec()
            }
        };
        let handle = unsafe { (*(out as *const ValueChanged)).handle };
        if let Ok(mut v) = SINK.lock() {
            v.push((std::time::Instant::now(), handle, bytes));
        }
    }

    /// GUID_BLUETOOTHLE_DEVICE_INTERFACE {781aee18-7733-4ce4-add0-91f41c67b592}
    const BTLE_DEVICE: Guid = Guid { d1: 0x781aee18, d2: 0x7733, d3: 0x4ce4, d4: [0xad, 0xd0, 0x91, 0xf4, 0x1c, 0x67, 0xb5, 0x92] };
    const DIGCF_PRESENT: u32 = 0x2;
    const DIGCF_DEVICEINTERFACE: u32 = 0x10;
    const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const SHARE_RW: u32 = 0x1 | 0x2;
    const OPEN_EXISTING: u32 = 3;
    const FLAG_FORCE_READ_FROM_DEVICE: u32 = 0x4;
    const FLAG_WRITE_WITHOUT_RESPONSE: u32 = 0x20;
    const CCCD: i32 = 2; // ClientCharacteristicConfiguration
    const VALUE_CHANGED_EVENT: i32 = 0;

    fn hr_text(hr: i32) -> String {
        let name = match hr as u32 {
            0x8007_0001 => " (incorrect function)",
            0x8007_0005 => " (access denied)",
            0x8007_0057 => " (wrong parameter)",
            0x8007_0490 => " (not found)",
            0x8007_001F => " (device does not respond)",
            0x8007_0079 => " (timeout)",
            0x8065_0002 => " (ATT: read not permitted)",
            0x8065_0003 => " (ATT: write not permitted)",
            0x8065_0005 => " (ATT: insufficient authentication)",
            0x8065_000F => " (ATT: insufficient encryption)",
            0x8065_0001..=0x8065_00FF => " (ATT error)",
            _ => "",
        };
        format!("0x{:08x}{name}", hr as u32)
    }

    /// GUID of a GATT UUID ("180f" or the 128-bit text form).
    fn guid_of(uuid: &str) -> Option<Guid> {
        let full = if uuid.len() == 4 { format!("0000{uuid}-0000-1000-8000-00805f9b34fb") } else { uuid.to_string() };
        let h: String = full.chars().filter(|c| *c != '-').collect();
        if h.len() != 32 {
            return None;
        }
        let b = |i: usize| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok();
        let mut d4 = [0u8; 8];
        for (k, x) in d4.iter_mut().enumerate() {
            *x = b(8 + k)?;
        }
        Some(Guid {
            d1: u32::from_str_radix(&h[0..8], 16).ok()?,
            d2: u16::from_str_radix(&h[8..12], 16).ok()?,
            d3: u16::from_str_radix(&h[12..16], 16).ok()?,
            d4,
        })
    }

    /// Path of the GATT service interface of one service of one device (Windows registers a
    /// device interface per service, with the service UUID as its class).
    fn service_path(address: &str, uuid: &str) -> Option<String> {
        let g = guid_of(uuid)?;
        interface_paths(&g).into_iter().find(|p| p.to_ascii_lowercase().contains(address))
    }

    fn empty_char() -> GattCharRaw {
        GattCharRaw {
            service_handle: 0,
            uuid: BthLeUuid { is_short: 0, value: BTLE_DEVICE },
            handle: 0,
            value_handle: 0,
            broadcastable: 0,
            readable: 0,
            writable: 0,
            write_no_response: 0,
            signed_writable: 0,
            notifiable: 0,
            indicatable: 0,
            extended: 0,
        }
    }

    /// Read one characteristic: through the service handle (cache, then device), then the device
    /// handle. Returns the value in hex or every error.
    fn read_value(handles: &[(Handle, &str)], ch: &mut GattCharRaw) -> Result<Vec<u8>, String> {
        let mut errors = Vec::new();
        for (h, what) in handles {
            for (flags, fname) in [(0u32, "cache"), (FLAG_FORCE_READ_FROM_DEVICE, "device")] {
                let mut buf = vec![0u8; 4 + 512];
                let mut req = 0u16;
                let hr = unsafe { BluetoothGATTGetCharacteristicValue(*h, ch, buf.len() as u32, buf.as_mut_ptr(), &mut req, flags) };
                if hr == 0 {
                    let len = (u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize).min(512);
                    return Ok(buf[4..4 + len].to_vec());
                }
                errors.push(format!("{what}/{fname} {}", hr_text(hr)));
            }
        }
        Err(errors.join(", "))
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open(path: &str, access: u32) -> Option<Handle> {
        let h = unsafe { CreateFileW(wide(path).as_ptr(), access, SHARE_RW, std::ptr::null_mut(), OPEN_EXISTING, 0, std::ptr::null_mut()) };
        (h as isize != INVALID).then_some(h)
    }

    fn last_error() -> String {
        let e = unsafe { GetLastError() };
        match e {
            1 => "error 1: incorrect function (the collection has no such report)".into(),
            5 => "error 5: access denied".into(),
            87 => "error 87: wrong parameter (report length or id)".into(),
            31 => "error 31: the device does not respond".into(),
            121 => "error 121: timeout".into(),
            n => format!("error {n}"),
        }
    }

    /// Report ids of one report type (0 input, 1 output, 2 feature), from value and button caps.
    fn report_ids(pp: isize, kind: i32, n_buttons: u16, n_values: u16) -> Vec<u8> {
        let mut ids = Vec::new();
        // HIDP_VALUE_CAPS and HIDP_BUTTON_CAPS are 72 bytes; ReportID is the byte at offset 2
        for (n, values) in [(n_values, true), (n_buttons, false)] {
            if n == 0 {
                continue;
            }
            let mut buf = vec![0u8; 72 * n as usize];
            let mut len = n;
            let st = unsafe {
                if values {
                    HidP_GetValueCaps(kind, buf.as_mut_ptr(), &mut len, pp)
                } else {
                    HidP_GetButtonCaps(kind, buf.as_mut_ptr(), &mut len, pp)
                }
            };
            if st == HIDP_STATUS_SUCCESS {
                for i in 0..len as usize {
                    ids.push(buf[i * 72 + 2]);
                }
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn caps(path: &str) -> HidCaps {
        let mut c = HidCaps::default();
        let Some(h) = open(path, 0) else {
            c.error = format!("cannot open: {}", last_error());
            return c;
        };
        unsafe {
            let mut pp: isize = 0;
            if HidD_GetPreparsedData(h, &mut pp) == 0 {
                c.error = format!("no HID descriptor: {}", last_error());
                CloseHandle(h);
                return c;
            }
            let mut caps = [0u16; 32];
            if HidP_GetCaps(pp, &mut caps) == HIDP_STATUS_SUCCESS {
                // Usage, UsagePage, Input/Output/FeatureReportByteLength, Reserved[17],
                // NumberLinkCollectionNodes, then button/value/data-index counts per type
                c.usage = caps[0];
                c.usage_page = caps[1];
                c.input_len = caps[2];
                c.output_len = caps[3];
                c.feature_len = caps[4];
                c.input_ids = report_ids(pp, 0, caps[23], caps[24]);
                c.output_ids = report_ids(pp, 1, caps[26], caps[27]);
                c.feature_ids = report_ids(pp, 2, caps[29], caps[30]);
            }
            HidD_FreePreparsedData(pp);
            CloseHandle(h);
        }
        c
    }

    pub fn probe(c: &HidCaps, request: &[u8]) -> Vec<Probe> {
        let mut v = Vec::new();
        let p = |method: String, ok: bool, detail: String| Probe { path: c.path.clone(), method, ok, detail };
        let Some(h) = open(&c.path, GENERIC_READ | GENERIC_WRITE).or_else(|| open(&c.path, 0)) else {
            v.push(p("open".into(), false, last_error()));
            return v;
        };
        let frame = |id: u8, len: u16| {
            let mut b = vec![0u8; len.max(2) as usize];
            b[0] = id;
            let n = (b.len() - 1).min(request.len());
            b[1..1 + n].copy_from_slice(&request[..n]);
            b
        };
        unsafe {
            if c.feature_len > 1 {
                let ids = if c.feature_ids.is_empty() { vec![0] } else { c.feature_ids.clone() };
                for id in ids {
                    let mut b = frame(id, c.feature_len);
                    if HidD_SetFeature(h, b.as_mut_ptr() as *mut c_void, b.len() as u32) == 0 {
                        v.push(p(format!("feature id 0x{id:02x} set"), false, last_error()));
                        continue;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    let mut r = vec![0u8; c.feature_len as usize];
                    r[0] = id;
                    if HidD_GetFeature(h, r.as_mut_ptr() as *mut c_void, r.len() as u32) == 0 {
                        v.push(p(format!("feature id 0x{id:02x} get"), false, last_error()));
                    } else {
                        v.push(p(format!("feature id 0x{id:02x}"), looks_like_razer(&r[1..]) || looks_like_razer(&r), hex(&r, 24)));
                    }
                }
            }
            if c.output_len > 1 {
                let ids = if c.output_ids.is_empty() { vec![0] } else { c.output_ids.clone() };
                for id in ids {
                    let mut b = frame(id, c.output_len);
                    if HidD_SetOutputReport(h, b.as_mut_ptr() as *mut c_void, b.len() as u32) == 0 {
                        v.push(p(format!("output id 0x{id:02x}"), false, last_error()));
                        continue;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    let in_ids = if c.input_ids.is_empty() { vec![0] } else { c.input_ids.clone() };
                    for iid in in_ids {
                        let mut r = vec![0u8; c.input_len.max(2) as usize];
                        r[0] = iid;
                        if HidD_GetInputReport(h, r.as_mut_ptr() as *mut c_void, r.len() as u32) == 0 {
                            v.push(p(format!("output 0x{id:02x} → input 0x{iid:02x}"), false, last_error()));
                        } else {
                            v.push(p(format!("output 0x{id:02x} → input 0x{iid:02x}"), looks_like_razer(&r), hex(&r, 24)));
                        }
                    }
                }
            }
            if c.feature_len <= 1 && c.output_len <= 1 {
                v.push(p("—".into(), false, "no feature or output reports in this collection".into()));
            }
            CloseHandle(h);
        }
        v
    }

    fn uuid_text(u: &BthLeUuid) -> String {
        if u.is_short != 0 {
            // the short form sits in the first two bytes of the union
            format!("{:04x}", u.value.d1 as u16)
        } else {
            let g = u.value;
            format!(
                "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{}",
                g.d1,
                g.d2,
                g.d3,
                g.d4[0],
                g.d4[1],
                g.d4[2..].iter().map(|b| format!("{b:02x}")).collect::<String>()
            )
        }
    }

    /// Present device interface paths of one interface class.
    fn interface_paths(class: &Guid) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let set = SetupDiGetClassDevsW(class, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
            if set as isize == INVALID {
                return out;
            }
            for i in 0..64 {
                let mut d = SpDeviceInterfaceData { size: std::mem::size_of::<SpDeviceInterfaceData>() as u32, class: *class, flags: 0, reserved: 0 };
                if SetupDiEnumDeviceInterfaces(set, std::ptr::null_mut(), class, i, &mut d) == 0 {
                    break;
                }
                // SP_DEVICE_INTERFACE_DETAIL_DATA_W: DWORD cbSize (8 on 64-bit, 6 on 32-bit) + WCHAR path[]
                let mut buf = vec![0u8; 4096];
                let cb: u32 = if std::mem::size_of::<usize>() == 8 { 8 } else { 6 };
                buf[..4].copy_from_slice(&cb.to_ne_bytes());
                let mut req = 0u32;
                if SetupDiGetDeviceInterfaceDetailW(set, &mut d, buf.as_mut_ptr(), buf.len() as u32, &mut req, std::ptr::null_mut()) == 0 {
                    continue;
                }
                let w: Vec<u16> = buf[4..].chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).take_while(|c| *c != 0).collect();
                out.push(String::from_utf16_lossy(&w));
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        out
    }

    pub fn gatt(address: &str) -> Result<Vec<GattService>, String> {
        if address.is_empty() {
            return Err("Bluetooth address of the mouse not found in its HID path".into());
        }
        let devs = interface_paths(&BTLE_DEVICE);
        let path = devs
            .iter()
            .find(|p| p.to_ascii_lowercase().contains(address))
            .ok_or_else(|| format!("no Bluetooth LE device with address {address} ({} BLE devices present)", devs.len()))?;
        let h = open(path, GENERIC_READ | GENERIC_WRITE).or_else(|| open(path, GENERIC_READ)).ok_or_else(|| format!("cannot open {path}: {}", last_error()))?;
        let mut services = Vec::new();
        unsafe {
            let mut n = 0u16;
            BluetoothGATTGetServices(h, 0, std::ptr::null_mut(), &mut n, 0);
            if n == 0 {
                CloseHandle(h);
                return Err("the device reports no GATT services (is it connected?)".into());
            }
            let mut raw = vec![GattServiceRaw { uuid: BthLeUuid { is_short: 0, value: BTLE_DEVICE }, handle: 0 }; n as usize];
            let hr = BluetoothGATTGetServices(h, n, raw.as_mut_ptr(), &mut n, 0);
            if hr != 0 {
                CloseHandle(h);
                return Err(format!("BluetoothGATTGetServices: 0x{hr:08x}"));
            }
            for s in raw.iter_mut().take(n as usize) {
                let uuid = uuid_text(&s.uuid);
                let svc_handle = service_path(address, &uuid).and_then(|p| open(&p, GENERIC_READ | GENERIC_WRITE).or_else(|| open(&p, GENERIC_READ)));
                let mut svc = GattService { name: name_of(&uuid), uuid, handle: s.handle, chars: Vec::new() };
                let mut m = 0u16;
                BluetoothGATTGetCharacteristics(h, s, 0, std::ptr::null_mut(), &mut m, 0);
                if m > 0 {
                    let empty = empty_char();
                    let mut chars = vec![empty; m as usize];
                    if BluetoothGATTGetCharacteristics(h, s, m, chars.as_mut_ptr(), &mut m, 0) == 0 {
                        for ch in chars.iter_mut().take(m as usize) {
                            let uuid = uuid_text(&ch.uuid);
                            let mut props = Vec::new();
                            for (on, name) in [
                                (ch.readable, "read"),
                                (ch.writable, "write"),
                                (ch.write_no_response, "write-no-response"),
                                (ch.signed_writable, "signed-write"),
                                (ch.notifiable, "notify"),
                                (ch.indicatable, "indicate"),
                            ] {
                                if on != 0 {
                                    props.push(name);
                                }
                            }
                            let value = if ch.readable != 0 {
                                let mut handles: Vec<(Handle, &str)> = Vec::new();
                                if let Some(sh) = svc_handle {
                                    handles.push((sh, "service"));
                                }
                                handles.push((h, "device"));
                                match read_value(&handles, ch) {
                                    Ok(v) => hex(&v, 64),
                                    Err(e) => format!("read error: {e}"),
                                }
                            } else {
                                String::new()
                            };
                            svc.chars.push(GattChar { name: name_of(&uuid), uuid, handle: ch.handle, props: props.join(" "), value });
                        }
                    }
                }
                if let Some(sh) = svc_handle {
                    CloseHandle(sh);
                }
                services.push(svc);
            }
            CloseHandle(h);
        }
        Ok(services)
    }

    /// An open, subscribed connection to the Razer GATT service. Dropping it unsubscribes.
    pub struct Session {
        h: Handle,
        chars: Vec<GattCharRaw>,
        names: std::collections::HashMap<u16, String>,
        event: isize,
        pub start: std::time::Instant,
        pub steps: Vec<String>,
    }

    // the handle is only used under GATT_LOCK
    unsafe impl Send for Session {}

    impl Session {
        pub fn open(address: &str) -> Result<Session, (String, Vec<String>)> {
            let mut steps = Vec::new();
            let Some(path) = service_path(address, RAZER_SERVICE) else {
                return Err((format!("the Razer GATT service {RAZER_SERVICE} of {address} was not found"), steps));
            };
            steps.push(format!("service interface {}", path.split('#').take(2).collect::<Vec<_>>().join("#")));
            let Some(h) = open(&path, GENERIC_READ | GENERIC_WRITE).or_else(|| open(&path, GENERIC_READ)) else {
                return Err((format!("cannot open the service: {}", last_error()), steps));
            };
            unsafe {
                // the service as this handle sees it
                let mut n = 0u16;
                BluetoothGATTGetServices(h, 0, std::ptr::null_mut(), &mut n, 0);
                let mut svcs = vec![GattServiceRaw { uuid: BthLeUuid { is_short: 0, value: BTLE_DEVICE }, handle: 0 }; n.max(1) as usize];
                let hr = BluetoothGATTGetServices(h, n.max(1), svcs.as_mut_ptr(), &mut n, 0);
                let Some(svc) = svcs.iter_mut().take(n as usize).find(|s| uuid_text(&s.uuid) == RAZER_SERVICE) else {
                    CloseHandle(h);
                    return Err((format!("BluetoothGATTGetServices on the service handle: {} ({n} services)", hr_text(hr)), steps));
                };
                let mut m = 0u16;
                BluetoothGATTGetCharacteristics(h, svc, 0, std::ptr::null_mut(), &mut m, 0);
                let mut chars = vec![empty_char(); m.max(1) as usize];
                let hr = BluetoothGATTGetCharacteristics(h, svc, m.max(1), chars.as_mut_ptr(), &mut m, 0);
                if hr != 0 || m == 0 {
                    CloseHandle(h);
                    return Err((format!("BluetoothGATTGetCharacteristics: {}", hr_text(hr)), steps));
                }
                chars.truncate(m as usize);
                // the value-changed event gives the characteristic's own handle (62 for …1525 on the
                // Naga Pro), not its value handle (63): name both
                let names: std::collections::HashMap<u16, String> =
                    chars.iter().flat_map(|c| [(c.handle, uuid_text(&c.uuid)), (c.value_handle, uuid_text(&c.uuid))]).collect();
                for c in &chars {
                    steps.push(format!(
                        "char {} handle {} value handle {} write {} write-no-response {} notify {} indicate {}",
                        uuid_text(&c.uuid),
                        c.handle,
                        c.value_handle,
                        c.writable,
                        c.write_no_response,
                        c.notifiable,
                        c.indicatable
                    ));
                }
                // subscribe: enable notifications in the CCCD of every notifying characteristic
                let mut reg = EventRegistration { count: 0, chars: [empty_char(); 4] };
                for c in chars.iter_mut().filter(|c| c.notifiable != 0 || c.indicatable != 0) {
                    let id = uuid_text(&c.uuid);
                    let mut k = 0u16;
                    BluetoothGATTGetDescriptors(h, c, 0, std::ptr::null_mut(), &mut k, 0);
                    let empty = GattDescRaw { service_handle: 0, char_handle: 0, kind: -1, uuid: BthLeUuid { is_short: 0, value: BTLE_DEVICE }, handle: 0 };
                    let mut ds = vec![empty; k.max(1) as usize];
                    let hr = BluetoothGATTGetDescriptors(h, c, k.max(1), ds.as_mut_ptr(), &mut k, 0);
                    match ds.iter_mut().take(k as usize).find(|d| d.kind == CCCD) {
                        Some(d) => {
                            // BTH_LE_GATT_DESCRIPTOR_VALUE: type, uuid (20), then the CCCD flags
                            let mut v = [0u8; 128];
                            v[0..4].copy_from_slice(&CCCD.to_ne_bytes());
                            v[4] = 1;
                            v[8..10].copy_from_slice(&0x2902u16.to_ne_bytes());
                            v[24] = (c.notifiable != 0) as u8;
                            v[25] = (c.notifiable == 0 && c.indicatable != 0) as u8;
                            let hr = BluetoothGATTSetDescriptorValue(h, d, v.as_mut_ptr(), 0);
                            steps.push(format!("subscribe {id}: {}", if hr == 0 { "OK".into() } else { hr_text(hr) }));
                        }
                        None => steps.push(format!("subscribe {id}: no CCCD descriptor ({} descriptors, {})", k, hr_text(hr))),
                    }
                    if (reg.count as usize) < reg.chars.len() {
                        reg.chars[reg.count as usize] = *c;
                        reg.count += 1;
                    }
                }
                if let Ok(mut s) = SINK.lock() {
                    s.clear();
                }
                let mut event: isize = 0;
                let hr = if reg.count > 0 {
                    BluetoothGATTRegisterEvent(h, VALUE_CHANGED_EVENT, &mut reg as *mut _ as *mut c_void, on_value, std::ptr::null_mut(), &mut event, 0)
                } else {
                    -1
                };
                steps.push(format!("listen to {} characteristics: {}", reg.count, if hr == 0 { "OK".into() } else { hr_text(hr) }));
                Ok(Session { h, chars, names, event, start: std::time::Instant::now(), steps })
            }
        }

        /// Write to the request characteristic (…1524).
        pub fn write(&mut self, request: &[u8]) -> Result<(), String> {
            let Some(c) = self.chars.iter_mut().find(|c| c.writable != 0 || c.write_no_response != 0) else {
                return Err("no writable characteristic".into());
            };
            let mut v = vec![0u8; 4 + request.len()];
            v[0..4].copy_from_slice(&(request.len() as u32).to_ne_bytes());
            v[4..].copy_from_slice(request);
            let flags = if c.writable != 0 { 0 } else { FLAG_WRITE_WITHOUT_RESPONSE };
            let hr = unsafe { BluetoothGATTSetCharacteristicValue(self.h, c, v.as_mut_ptr(), 0, flags) };
            if hr == 0 {
                Ok(())
            } else {
                Err(hr_text(hr))
            }
        }

        /// Notifications received so far (time, value handle, uuid, bytes), removed from the queue.
        pub fn take(&self) -> Vec<(std::time::Instant, u16, String, Vec<u8>)> {
            SINK.lock()
                .map(|mut s| s.drain(..).map(|(t, h, b)| (t, h, self.names.get(&h).cloned().unwrap_or_default(), b)).collect())
                .unwrap_or_default()
        }

        /// Current values of the readable characteristics.
        pub fn read_all(&mut self) -> Vec<String> {
            let h = self.h;
            self.chars
                .iter_mut()
                .filter(|c| c.readable != 0)
                .map(|c| {
                    let r = read_value(&[(h, "service")], c);
                    format!(
                        "read {}: {}",
                        uuid_text(&c.uuid),
                        match r {
                            Ok(v) => hex(&v, 128),
                            Err(e) => e,
                        }
                    )
                })
                .collect()
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                if self.event != 0 {
                    BluetoothGATTUnregisterEvent(self.event, 0);
                }
                CloseHandle(self.h);
            }
        }
    }
}

// ------------------------------------------------------------------ Linux and others

#[cfg(not(windows))]
mod imp {
    use super::*;

    /// Linux: the HID report descriptor from sysfs (lengths and ids are left to the reader).
    pub fn caps(path: &str) -> HidCaps {
        let mut c = HidCaps::default();
        let node = path.trim_start_matches("/dev/");
        match std::fs::read(format!("/sys/class/hidraw/{node}/device/report_descriptor")) {
            Ok(d) => {
                c.descriptor = hex(&d, 512);
                // report ids: item 0x85 <id>
                let mut ids: Vec<u8> = d.windows(2).filter(|w| w[0] == 0x85).map(|w| w[1]).collect();
                ids.sort();
                ids.dedup();
                c.feature_ids = ids;
                // the first usage page item (0x05 / 0x06)
                c.usage_page = match d.first() {
                    Some(0x05) if d.len() > 1 => d[1] as u16,
                    Some(0x06) if d.len() > 2 => u16::from_le_bytes([d[1], d[2]]),
                    _ => 0,
                };
            }
            Err(e) => c.error = format!("report descriptor: {e}"),
        }
        c
    }

    pub fn probe(c: &HidCaps, request: &[u8]) -> Vec<Probe> {
        // our hidraw transport sends feature reports with id 0
        let mut v = Vec::new();
        let mut req = [0u8; crate::protocol::REPORT_LEN];
        req.copy_from_slice(&request[..crate::protocol::REPORT_LEN]);
        let info = crate::transport::DeviceInfo { path: c.path.clone(), backend: "hidraw".into(), ..crate::transport::DeviceInfo::mock() };
        match crate::transport::open(&info) {
            Ok(mut t) => match t.send_feature(&req).and_then(|_| {
                std::thread::sleep(std::time::Duration::from_millis(60));
                t.get_feature()
            }) {
                Ok(r) => v.push(Probe { path: c.path.clone(), method: "feature id 0x00".into(), ok: looks_like_razer(&r), detail: hex(&r, 24) }),
                Err(e) => v.push(Probe { path: c.path.clone(), method: "feature id 0x00".into(), ok: false, detail: e.to_string() }),
            },
            Err(e) => v.push(Probe { path: c.path.clone(), method: "open".into(), ok: false, detail: e.to_string() }),
        }
        v
    }

    /// Not available yet on Linux (BlueZ over D-Bus would be needed).
    pub struct Session {
        pub start: std::time::Instant,
        pub steps: Vec<String>,
    }

    impl Session {
        pub fn open(_address: &str) -> Result<Session, (String, Vec<String>)> {
            Err((
                "Not available on Linux yet. With bluetoothctl: menu gatt → select-attribute 52401525-f97c-7f90-0e7f-6c6f4e36db1c → notify on; then select-attribute 52401524-… → write \"<bytes>\"".into(),
                Vec::new(),
            ))
        }
        pub fn write(&mut self, _request: &[u8]) -> Result<(), String> {
            Err("not available".into())
        }
        pub fn take(&self) -> Vec<(std::time::Instant, u16, String, Vec<u8>)> {
            Vec::new()
        }
        pub fn read_all(&mut self) -> Vec<String> {
            Vec::new()
        }
    }

    pub fn gatt(_address: &str) -> Result<Vec<GattService>, String> {
        Err("GATT listing is available on Windows; on Linux use: bluetoothctl → menu gatt → list-attributes".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_writes() {
        use crate::protocol::cmd;
        // USB set_button: profile 1, side button 12 (4b), normal layer, key 7 (24)
        let (c, k, sub, d) = BleBridge::ble_set(cmd::SET_BUTTON, &[1, 0x4b, 0, 2, 2, 0, 0x24, 0, 0, 0]).unwrap();
        assert_eq!((c, k, sub), (0x08, 0x04, [1, 0x4b]));
        assert_eq!(d, vec![1, 0x4b, 0, 2, 2, 0, 0x24, 0, 0, 0]); // as the self-test wrote it
        // Hypershift layer: layer byte 01
        assert_eq!(BleBridge::ble_set(cmd::SET_BUTTON, &[2, 0x40, 1, 1, 1, 4, 0, 0, 0, 0]).unwrap().3[..3], [2, 0x40, 1]);
        // the temporary layer (0) is the read-only mirror over Bluetooth
        assert!(BleBridge::ble_set(cmd::SET_BUTTON, &[0, 0x4b, 0, 2, 2, 0, 0x24, 0, 0, 0]).is_err());
        // sleep timer: USB big-endian seconds -> Bluetooth little-endian (360 s = 68 01)
        assert_eq!(BleBridge::ble_set(cmd::SET_IDLE_TIME, &[0x01, 0x68]).unwrap(), (0x05, 0x04, [0, 0], vec![0x68, 0x01]));
        assert!(BleBridge::ble_set(cmd::SET_DPI, &[0; 7]).is_err());
        // profile list 03 80 -> 01 02
        assert_eq!(usb_args(cmd::GET_PROFILE_COUNT, &[], &[1, 2]), Some(vec![2]));
        assert_eq!(usb_args(cmd::GET_PROFILE_LIST, &[], &[1, 2]), Some(vec![2, 1, 2]));
        assert_eq!(ble_for(cmd::GET_BUTTON, &[2, 0x4b, 0]), Some((0x08, 0x84, [2, 0x4b])));
    }

    #[test]
    fn target_payload_matches_opensnek() {
        // OpenSnek, Basilisk V3 X BT: slot 02 → right click: 01 02 00 01 01 02 00 00 00 00
        assert_eq!(target_payload(1, 0x02, 0, &[0x01, 0x01, 0x02]), vec![1, 2, 0, 1, 1, 2, 0, 0, 0, 0]);
        // slot 02 → Command+[ : 01 02 00 02 02 08 2f 00 00 00
        assert_eq!(target_payload(1, 0x02, 0, &[0x02, 0x02, 0x08, 0x2f]), vec![1, 2, 0, 2, 2, 8, 0x2f, 0, 0, 0]);
    }

    #[test]
    fn helpers() {
        let p = r"\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&021532_PID&0092_REV&0001_eac88650b167&Col06#9&32d7df60&0&0005#{4d1e55b2}";
        assert_eq!(address_from_path(p).as_deref(), Some("eac88650b167"));
        assert_eq!(address_from_path(r"\\?\HID#VID_1532&PID_008F&MI_00#9&2"), None);
        assert_eq!(short_of("2a19"), Some(0x2A19));
        assert_eq!(short_of("00002a19-0000-1000-8000-00805f9b34fb"), Some(0x2A19));
        assert_eq!(name_of("180f"), "Battery");
        assert_eq!(name_of("52401523-f97c-7f90-0e7f-6c6f4e36db1c"), "vendor-specific");
        let req = firmware_request(0x1f);
        assert_eq!((req[1], req[6], req[7]), (0x1f, 0x00, 0x81));
        let mut reply = req;
        reply[0] = 2;
        assert!(looks_like_razer(&reply));
        assert!(!looks_like_razer(&[0u8; 10]));
        assert_eq!(hex(&[1, 2, 3], 2), "01 02 … (3 bytes)");
        assert_eq!(parse_hex("1f 00 81").unwrap(), vec![0x1f, 0, 0x81]);
        assert_eq!(parse_hex("0x1f,0x00\n1f0081").unwrap(), vec![0x1f, 0, 0x1f, 0, 0x81]);
        assert!(parse_hex("1f0").is_err());
        assert_eq!(parse_hex("").unwrap(), Vec::<u8>::new());
        assert_eq!(parse_byte_list("0b").unwrap(), vec![0x0b]);
        assert_eq!(parse_byte_list("80-83, a0").unwrap(), vec![0x80, 0x81, 0x82, 0x83, 0xa0]);
        // USB replies made from Bluetooth data (recorded from a Naga Pro)
        use crate::protocol::cmd;
        assert_eq!(usb_args(cmd::GET_FIRMWARE, &[], &[2, 1, 0, 0]), Some(vec![2, 1]));
        assert_eq!(usb_args(cmd::GET_DPI, &[0], &[0x80, 0x0c, 0x80, 0x0c, 0, 0]), Some(vec![0, 0x0c, 0x80, 0x0c, 0x80, 0, 0]));
        let st = parse_hex("02 03 01 20 03 20 03 00 00 02 80 0c 80 0c 00 00 03 00 19 00 19 00").unwrap();
        let v = usb_args(cmd::GET_DPI_STAGES, &[1], &st).unwrap();
        assert_eq!(&v[..10], &[1, 2, 3, 1, 0x03, 0x20, 0x03, 0x20, 0, 0]);
        assert_eq!(&v[17..22], &[3, 0x19, 0x00, 0x19, 0x00]);
        assert_eq!(usb_args(cmd::GET_IDLE_TIME, &[], &[0x2c, 0x01]), Some(vec![0x01, 0x2c]));
        assert_eq!(usb_args(cmd::GET_BATTERY, &[], &[0xf4]), Some(vec![0, 0xf4]));
        let rec = parse_hex("41 01 01 02 01 02 04 00 00 1f 00 00 00 00 00 00").unwrap();
        assert_eq!(usb_args(cmd::GET_BUTTON, &[1, 0x41, 0], &rec), Some(vec![1, 0x41, 0, 1, 1, 4, 0, 0, 0, 0]));
        assert_eq!(usb_args(cmd::GET_BUTTON, &[1, 0x41, 1], &rec), Some(vec![1, 0x41, 1, 2, 2, 0, 0x1f, 0, 0, 0]));
        assert_eq!(usb_args(cmd::GET_DPI_STAGES, &[1], &[2]), None);
        let rec = parse_hex("4b 01 02 02 02 02 00 00 2e 2e 00 00 00 00 00 00").unwrap();
        assert_eq!(with_key_f(&rec), parse_hex("4b 01 02 02 02 02 00 00 09 2e 00 00 00 00 00 00").unwrap());
        let rec = parse_hex("41 01 01 02 01 02 04 00 00 1f 00 00 00 00 00 00").unwrap();
        assert_eq!(with_key_f(&rec), parse_hex("41 01 02 02 02 02 00 00 09 1f 00 00 00 00 00 00").unwrap());
        let (n, h) = (parse_hex("02 02 00 2e").unwrap(), parse_hex("02 02 00 2e").unwrap());
        assert_eq!(interleave(0x4b, &n, &h), parse_hex("4b 01 02 02 02 02 00 00 2e 2e 00 00 00 00 00 00").unwrap());
        assert_eq!(button_layout(3, 0x4b, &[2, 2, 0, 9], &h), ([1, 0x4b], parse_hex("01 4b 00 02 02 00 09 00 00 00").unwrap()));
        assert_eq!(button_layout(6, 0x4b, &n, &h).1.len(), 14);
        assert!(parse_byte_list("83-80").is_err() && parse_byte_list("").is_err() && parse_byte_list("zz").is_err());
    }
}
