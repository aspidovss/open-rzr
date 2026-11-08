//! How 90-byte feature reports reach the mouse.
//!
//! * Linux   — native hidraw via ioctl (no libraries; the kernel mouse driver stays attached)
//! * Windows — hidapi (opens the mouse collection; feature reports need no extra rights)
//! * Mock    — in-memory emulator for tests and for trying the app without a mouse
//! * OpenRazer bridge — sysfs files of the OpenRazer kernel driver (Linux, compatibility mode)

use std::time::{Duration, Instant};

use crate::protocol::{status, Error, Report, Result, REPORT_LEN};

#[cfg(target_os = "linux")]
mod linux;
pub mod mock;
pub mod openrazer;
#[cfg(windows)]
mod windows;

pub trait Transport: Send {
    fn send_feature(&mut self, data: &[u8; REPORT_LEN]) -> Result<()>;
    fn get_feature(&mut self) -> Result<[u8; REPORT_LEN]>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceInfo {
    pub path: String,
    pub vid: u16,
    pub pid: u16,
    pub interface: i32,
    pub name: String,
    /// "hidraw" | "hidapi" | "mock"
    pub backend: String,
    pub usage_page: u16,
    pub usage: u16,
}

impl DeviceInfo {
    pub fn label(&self) -> String {
        format!(
            "{} [{:04x}:{:04x} if{} {}]",
            if self.name.is_empty() { "Razer" } else { &self.name },
            self.vid,
            self.pid,
            self.interface,
            self.backend
        )
    }

    pub fn mock() -> DeviceInfo {
        DeviceInfo {
            path: "mock://naga-pro".into(),
            vid: crate::devices::RAZER_VID,
            pid: 0x0090,
            interface: 0,
            name: "Naga Pro emulator".into(),
            backend: "mock".into(),
            usage_page: 0,
            usage: 0,
        }
    }
}

/// All Razer HID endpoints the OS reports (unfiltered).
pub fn enumerate(include_mock: bool) -> Vec<DeviceInfo> {
    #[cfg(target_os = "linux")]
    let mut v = linux::enumerate();
    #[cfg(windows)]
    let mut v = match windows::enumerate() {
        Ok(d) => d,
        Err(e) => {
            log_warn!("app", "hidapi: {e}");
            Vec::new()
        }
    };
    #[cfg(not(any(target_os = "linux", windows)))]
    let mut v: Vec<DeviceInfo> = Vec::new();
    if include_mock || std::env::var_os("OPENRZR_MOCK").is_some() {
        v.push(DeviceInfo::mock());
    }
    v
}

pub fn open(info: &DeviceInfo) -> Result<Box<dyn Transport>> {
    match info.backend.as_str() {
        "mock" => Ok(Box::new(mock::MockNagaPro::new())),
        #[cfg(target_os = "linux")]
        "hidraw" => Ok(Box::new(linux::Hidraw::open(&info.path)?)),
        #[cfg(windows)]
        "hidapi" => Ok(Box::new(windows::Hidapi::open(&info.path)?)),
        b => Err(Error::NotFound(format!("transport {b} is not available on this OS"))),
    }
}

/// Serialises request/response pairs, handles BUSY retries, logs every packet.
pub struct Channel {
    t: Box<dyn Transport>,
    pub tid: u8,
    pub delay: Duration,
}

impl Channel {
    pub fn new(t: Box<dyn Transport>, tid: u8, delay_ms: u64) -> Channel {
        Channel { t, tid, delay: Duration::from_millis(delay_ms) }
    }

    pub fn request(&mut self, cmd: u16, args: &[u8], data_size: Option<u8>) -> Result<Report> {
        let req = Report::new(cmd, args, data_size, self.tid)?;
        let t0 = Instant::now();
        log_debug!("usb", ">> {}", req.hexdump());
        self.t.send_feature(&req.to_bytes())?;
        let mut delay = self.delay;
        let mut rep = None;
        let mut busy = 0;
        let mut foreign = 0;
        for _ in 0..12 {
            std::thread::sleep(delay);
            let r = Report::from_bytes(&self.t.get_feature()?);
            if r.status == status::BUSY || r.status == status::NEW {
                busy += 1;
                delay = (delay * 8 / 5).min(Duration::from_millis(250));
                rep = Some(r);
                continue;
            }
            if (r.command_class, r.command_id) != (req.command_class, req.command_id) {
                foreign += 1;
                delay = (delay * 8 / 5).min(Duration::from_millis(250));
                rep = Some(r);
                continue;
            }
            rep = Some(r);
            break;
        }
        let rep = rep.ok_or(Error::Io("no reply".into()))?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        log_debug!(
            "usb",
            "<< {} [{:.0} ms{}{}]",
            rep.hexdump(),
            ms,
            if busy > 0 { format!(", BUSY×{busy}") } else { String::new() },
            if foreign > 0 { format!(", foreign reply×{foreign}") } else { String::new() }
        );
        if (rep.command_class, rep.command_id) != (req.command_class, req.command_id) {
            log_warn!("usb", "reply to another command: expected {:04x}, got {:04x}", req.cmd(), rep.cmd());
            return Err(Error::Conflict);
        }
        if rep.status != status::OK {
            return Err(Error::Failed { cmd, status: rep.status });
        }
        Ok(rep)
    }
}
