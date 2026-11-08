//! Notifications when a Razer device is plugged in or removed, instead of polling.
//!
//! * Windows — `CM_Register_Notification` (cfgmgr32) for the HID device interface class;
//!   no window or message loop is needed, the callback runs on a system thread.
//! * Linux   — kernel uevents over a netlink socket (subsystem hidraw), read on a thread.
//!
//! `watch` returns None when notifications are not available; callers then keep polling.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Added,
    Removed,
}

impl Change {
    pub fn code(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Removed => "removed",
        }
    }
}

pub type Callback = Box<dyn Fn(Change) + Send + Sync + 'static>;

/// Keeps the notification registered while alive.
pub struct Watcher {
    #[cfg(windows)]
    #[allow(dead_code)] // unregisters on drop
    inner: windows::Registration,
    #[cfg(not(windows))]
    _p: (),
}

/// Start watching for Razer HID devices (vendor 1532).
pub fn watch(f: impl Fn(Change) + Send + Sync + 'static) -> Option<Watcher> {
    let cb: Callback = Box::new(f);
    #[cfg(windows)]
    {
        match windows::register(cb) {
            Ok(r) => {
                log_info!("app", "USB notifications: on (cfgmgr32)");
                Some(Watcher { inner: r })
            }
            Err(e) => {
                log_warn!("app", "USB notifications unavailable ({e}); polling instead");
                None
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        match linux::start(cb) {
            Ok(()) => {
                log_info!("app", "USB notifications: on (netlink uevent)");
                Some(Watcher { _p: () })
            }
            Err(e) => {
                log_warn!("app", "USB notifications unavailable ({e}); polling instead");
                None
            }
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = cb;
        None
    }
}

impl Watcher {
    /// Keep the registration for the whole life of the program.
    pub fn keep(self) {
        std::mem::forget(self);
    }
}

/// Linux uevent ("add@/devices/…/0003:1532:008F.0005/hidraw/hidraw3\0ACTION=add\0SUBSYSTEM=hidraw\0…").
pub fn parse_uevent(msg: &[u8]) -> Option<Change> {
    let text = String::from_utf8_lossy(msg);
    let mut parts = text.split('\0');
    let head = parts.next()?;
    let (action, path) = head.split_once('@')?;
    let mut subsystem = "";
    for kv in parts {
        if let Some(v) = kv.strip_prefix("SUBSYSTEM=") {
            subsystem = v;
        }
    }
    if subsystem != "hidraw" || !path.to_ascii_uppercase().contains(":1532:") {
        return None;
    }
    match action {
        "add" => Some(Change::Added),
        "remove" => Some(Change::Removed),
        _ => None,
    }
}

/// Windows device interface path, e.g. `\\?\HID#VID_1532&PID_008F&MI_00#…`.
pub fn is_razer_interface(link: &str) -> bool {
    link.to_ascii_uppercase().contains("VID_1532")
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{parse_uevent, Callback};
    use std::os::raw::{c_int, c_void};

    #[repr(C)]
    struct SockaddrNl {
        nl_family: u16,
        nl_pad: u16,
        nl_pid: u32,
        nl_groups: u32,
    }

    extern "C" {
        fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int;
        fn bind(fd: c_int, addr: *const SockaddrNl, len: u32) -> c_int;
        fn recv(fd: c_int, buf: *mut c_void, len: usize, flags: c_int) -> isize;
    }

    const AF_NETLINK: c_int = 16;
    const SOCK_DGRAM: c_int = 2;
    const SOCK_CLOEXEC: c_int = 0o2000000;
    const NETLINK_KOBJECT_UEVENT: c_int = 15;

    pub fn start(cb: Callback) -> Result<(), String> {
        let fd = unsafe { socket(AF_NETLINK, SOCK_DGRAM | SOCK_CLOEXEC, NETLINK_KOBJECT_UEVENT) };
        if fd < 0 {
            return Err(format!("socket: {}", std::io::Error::last_os_error()));
        }
        // group 1: events straight from the kernel
        let addr = SockaddrNl { nl_family: AF_NETLINK as u16, nl_pad: 0, nl_pid: 0, nl_groups: 1 };
        if unsafe { bind(fd, &addr, std::mem::size_of::<SockaddrNl>() as u32) } < 0 {
            return Err(format!("bind: {}", std::io::Error::last_os_error()));
        }
        std::thread::Builder::new()
            .name("usb-hotplug".into())
            .spawn(move || {
                let mut buf = vec![0u8; 16 * 1024];
                loop {
                    let n = unsafe { recv(fd, buf.as_mut_ptr() as *mut c_void, buf.len(), 0) };
                    if n < 0 {
                        let e = std::io::Error::last_os_error();
                        if e.kind() == std::io::ErrorKind::Interrupted {
                            continue;
                        }
                        log_warn!("app", "USB notifications stopped: {e}");
                        return;
                    }
                    if let Some(c) = parse_uevent(&buf[..n as usize]) {
                        log_debug!("app", "USB: Razer device {}", c.code());
                        cb(c);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::{is_razer_interface, Callback, Change};
    use std::os::raw::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        d1: u32,
        d2: u16,
        d3: u16,
        d4: [u8; 8],
    }

    /// CM_NOTIFY_FILTER: header (16 bytes) + union of which the largest member is
    /// WCHAR InstanceId[200] (400 bytes); 8-byte aligned because of the HANDLE member.
    #[repr(C, align(8))]
    struct CmNotifyFilter {
        cb_size: u32,
        flags: u32,
        filter_type: u32,
        reserved: u32,
        class_guid: Guid,
        _rest: [u8; 400 - 16],
    }

    type CmCallback = unsafe extern "system" fn(
        notify: *mut c_void,
        context: *mut c_void,
        action: u32,
        data: *const u8,
        size: u32,
    ) -> u32;

    #[link(name = "cfgmgr32")]
    extern "system" {
        fn CM_Register_Notification(
            filter: *const CmNotifyFilter,
            context: *mut c_void,
            callback: CmCallback,
            notify: *mut *mut c_void,
        ) -> u32;
        fn CM_Unregister_Notification(notify: *mut c_void) -> u32;
    }

    const CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE: u32 = 0;
    const CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL: u32 = 0;
    const CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL: u32 = 1;
    /// GUID_DEVINTERFACE_HID {4D1E55B2-F16F-11CF-88CB-001111000030}
    const GUID_DEVINTERFACE_HID: Guid =
        Guid { d1: 0x4D1E_55B2, d2: 0xF16F, d3: 0x11CF, d4: [0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30] };

    pub struct Registration {
        handle: *mut c_void,
        context: *mut Callback,
    }

    // the handle is only passed back to CM_Unregister_Notification
    unsafe impl Send for Registration {}

    impl Drop for Registration {
        fn drop(&mut self) {
            unsafe {
                CM_Unregister_Notification(self.handle);
                drop(Box::from_raw(self.context));
            }
        }
    }

    /// CM_NOTIFY_EVENT_DATA: FilterType (4) + Reserved (4) + ClassGuid (16) + WCHAR SymbolicLink[]
    unsafe extern "system" fn on_change(_n: *mut c_void, ctx: *mut c_void, action: u32, data: *const u8, size: u32) -> u32 {
        let change = match action {
            CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL => Change::Added,
            CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL => Change::Removed,
            _ => return 0,
        };
        if data.is_null() || ctx.is_null() || size < 26 {
            return 0;
        }
        let max = ((size as usize) - 24) / 2;
        let p = data.add(24) as *const u16;
        let mut link = Vec::with_capacity(max.min(512));
        for i in 0..max {
            let c = std::ptr::read_unaligned(p.add(i));
            if c == 0 {
                break;
            }
            link.push(c);
        }
        let link = String::from_utf16_lossy(&link);
        if is_razer_interface(&link) {
            let cb = &*(ctx as *const Callback);
            cb(change);
        }
        0
    }

    pub fn register(cb: Callback) -> Result<Registration, String> {
        let filter = CmNotifyFilter {
            cb_size: std::mem::size_of::<CmNotifyFilter>() as u32,
            flags: 0,
            filter_type: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            reserved: 0,
            class_guid: GUID_DEVINTERFACE_HID,
            _rest: [0; 400 - 16],
        };
        let context = Box::into_raw(Box::new(cb));
        let mut handle: *mut c_void = std::ptr::null_mut();
        let r = unsafe { CM_Register_Notification(&filter, context as *mut c_void, on_change, &mut handle) };
        if r != 0 {
            unsafe { drop(Box::from_raw(context)) };
            return Err(format!("CM_Register_Notification: CONFIGRET {r}"));
        }
        Ok(Registration { handle, context })
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn filter_size() {
            assert_eq!(std::mem::size_of::<super::CmNotifyFilter>(), 416);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uevents() {
        let add = b"add@/devices/pci0000:00/0000:00:14.0/usb1/1-3/1-3:1.0/0003:1532:008F.0005/hidraw/hidraw3\0ACTION=add\0DEVPATH=/x\0SUBSYSTEM=hidraw\0DEVNAME=hidraw3\0SEQNUM=4242\0";
        assert_eq!(parse_uevent(add), Some(Change::Added));
        let rm = b"remove@/devices/x/0003:1532:0090.0007/hidraw/hidraw5\0ACTION=remove\0SUBSYSTEM=hidraw\0";
        assert_eq!(parse_uevent(rm), Some(Change::Removed));
        let other = b"add@/devices/x/0003:046D:C52B.0001/hidraw/hidraw1\0ACTION=add\0SUBSYSTEM=hidraw\0";
        assert_eq!(parse_uevent(other), None);
        let usb = b"add@/devices/x/1-3\0ACTION=add\0SUBSYSTEM=usb\0";
        assert_eq!(parse_uevent(usb), None);
        assert!(is_razer_interface(r"\\?\HID#VID_1532&PID_008F&MI_00#7&2a5b&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"));
        assert!(!is_razer_interface(r"\\?\HID#VID_046D&PID_C52B"));
    }
}
