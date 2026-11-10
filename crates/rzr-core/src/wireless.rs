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
    imp::exchange(&address, &bytes, listen_ms.clamp(100, 10_000), &mut x);
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

    pub fn exchange(address: &str, request: &[u8], listen_ms: u32, x: &mut Exchange) {
        let Some(path) = service_path(address, RAZER_SERVICE) else {
            x.error = format!("the Razer GATT service {RAZER_SERVICE} of {address} was not found");
            return;
        };
        x.steps.push(format!("service interface {}", path.split('#').take(2).collect::<Vec<_>>().join("#")));
        let Some(h) = open(&path, GENERIC_READ | GENERIC_WRITE).or_else(|| open(&path, GENERIC_READ)) else {
            x.error = format!("cannot open the service: {}", last_error());
            return;
        };
        unsafe {
            // the service as this handle sees it
            let mut n = 0u16;
            BluetoothGATTGetServices(h, 0, std::ptr::null_mut(), &mut n, 0);
            let mut svcs = vec![GattServiceRaw { uuid: BthLeUuid { is_short: 0, value: BTLE_DEVICE }, handle: 0 }; n.max(1) as usize];
            let hr = BluetoothGATTGetServices(h, n.max(1), svcs.as_mut_ptr(), &mut n, 0);
            let Some(svc) = svcs.iter_mut().take(n as usize).find(|s| uuid_text(&s.uuid) == RAZER_SERVICE) else {
                x.error = format!("BluetoothGATTGetServices on the service handle: {} ({n} services)", hr_text(hr));
                CloseHandle(h);
                return;
            };
            let mut m = 0u16;
            BluetoothGATTGetCharacteristics(h, svc, 0, std::ptr::null_mut(), &mut m, 0);
            let mut chars = vec![empty_char(); m.max(1) as usize];
            let hr = BluetoothGATTGetCharacteristics(h, svc, m.max(1), chars.as_mut_ptr(), &mut m, 0);
            if hr != 0 || m == 0 {
                x.error = format!("BluetoothGATTGetCharacteristics: {}", hr_text(hr));
                CloseHandle(h);
                return;
            }
            chars.truncate(m as usize);
            let names: std::collections::HashMap<u16, String> = chars.iter().map(|c| (c.value_handle, uuid_text(&c.uuid))).collect();
            for c in &chars {
                x.steps.push(format!(
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
            // 1. subscribe: enable notifications in the CCCD of every notifying characteristic
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
                        x.steps.push(format!("subscribe {id}: {}", if hr == 0 { "OK".into() } else { hr_text(hr) }));
                    }
                    None => x.steps.push(format!("subscribe {id}: no CCCD descriptor ({} descriptors, {})", k, hr_text(hr))),
                }
                if (reg.count as usize) < reg.chars.len() {
                    reg.chars[reg.count as usize] = *c;
                    reg.count += 1;
                }
            }
            if let Ok(mut s) = SINK.lock() {
                s.clear();
            }
            let start = std::time::Instant::now();
            let mut event: isize = 0;
            let hr = if reg.count > 0 {
                BluetoothGATTRegisterEvent(h, VALUE_CHANGED_EVENT, &mut reg as *mut _ as *mut c_void, on_value, std::ptr::null_mut(), &mut event, 0)
            } else {
                -1
            };
            x.steps.push(format!("listen to {} characteristics: {}", reg.count, if hr == 0 { "OK".into() } else { hr_text(hr) }));
            // 2. write the request
            if !request.is_empty() {
                match chars.iter_mut().find(|c| c.writable != 0 || c.write_no_response != 0) {
                    Some(c) => {
                        let mut v = vec![0u8; 4 + request.len()];
                        v[0..4].copy_from_slice(&(request.len() as u32).to_ne_bytes());
                        v[4..].copy_from_slice(request);
                        let flags = if c.writable != 0 { 0 } else { FLAG_WRITE_WITHOUT_RESPONSE };
                        let hr = BluetoothGATTSetCharacteristicValue(h, c, v.as_mut_ptr(), 0, flags);
                        x.steps.push(format!("write {} bytes to {}: {}", request.len(), uuid_text(&c.uuid), if hr == 0 { "OK".into() } else { hr_text(hr) }));
                    }
                    None => x.steps.push("no writable characteristic".into()),
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(listen_ms as u64));
            if event != 0 {
                BluetoothGATTUnregisterEvent(event, 0);
            }
            if let Ok(mut s) = SINK.lock() {
                for (t, handle, bytes) in s.drain(..) {
                    x.notifications.push(Notification {
                        t_ms: t.saturating_duration_since(start).as_millis() as u64,
                        handle,
                        uuid: names.get(&handle).cloned().unwrap_or_default(),
                        hex: hex(&bytes, 128),
                        len: bytes.len(),
                    });
                }
            }
            // 3. the current values of the readable characteristics
            for c in chars.iter_mut().filter(|c| c.readable != 0) {
                let r = read_value(&[(h, "service")], c);
                x.steps.push(format!(
                    "read {}: {}",
                    uuid_text(&c.uuid),
                    match r {
                        Ok(v) => hex(&v, 128),
                        Err(e) => e,
                    }
                ));
            }
            CloseHandle(h);
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

    pub fn exchange(_address: &str, _request: &[u8], _listen_ms: u32, x: &mut Exchange) {
        x.error = "Not available on Linux yet. With bluetoothctl: menu gatt → select-attribute 52401525-f97c-7f90-0e7f-6c6f4e36db1c → notify on; then select-attribute 52401524-… → write \"<bytes>\"".into();
    }

    pub fn gatt(_address: &str) -> Result<Vec<GattService>, String> {
        Err("GATT listing is available on Windows; on Linux use: bluetoothctl → menu gatt → list-attributes".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
