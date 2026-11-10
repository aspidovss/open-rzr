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
    /// connected over Bluetooth (LE): the mouse has another product id and other HID collections
    #[cfg_attr(feature = "json", serde(default))]
    pub bluetooth: bool,
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
            vid: crate::RAZER_VID,
            pid: 0x0090,
            interface: 0,
            name: "Naga Pro emulator".into(),
            backend: "mock".into(),
            usage_page: 0,
            usage: 0,
            bluetooth: false,
        }
    }
}

/// Every HID endpoint of every vendor (Diagnostics → All HID devices).
pub fn enumerate_all() -> Vec<DeviceInfo> {
    #[cfg(target_os = "linux")]
    return linux::enumerate(true);
    #[cfg(windows)]
    return windows::enumerate(true).unwrap_or_default();
    #[cfg(not(any(target_os = "linux", windows)))]
    Vec::new()
}

/// All Razer HID endpoints the OS reports (unfiltered).
pub fn enumerate(include_mock: bool) -> Vec<DeviceInfo> {
    #[cfg(target_os = "linux")]
    let mut v = linux::enumerate(false);
    #[cfg(windows)]
    let mut v = match windows::enumerate(false) {
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

/// How long a request may take and how the reply is polled.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// wait before the first read of the reply
    pub first_delay: Duration,
    /// the wait grows ×1.6 per poll up to this
    pub max_delay: Duration,
    /// the whole request (send + polls) must finish within this
    pub deadline: Duration,
    /// a device found asleep is not asked again for this long (requests fail at once)
    pub asleep_backoff: Duration,
}

impl Timing {
    pub fn new(first_delay_ms: u64) -> Timing {
        Timing {
            first_delay: Duration::from_millis(first_delay_ms),
            max_delay: Duration::from_millis(250),
            deadline: Duration::from_millis(2000),
            asleep_backoff: Duration::from_millis(3000),
        }
    }
}

/// Serialises request/response pairs: strict deadline, BUSY retries, foreign-reply detection,
/// sleeping wireless devices (`Error::Asleep`, logged once, not on every request).
pub struct Channel {
    t: Box<dyn Transport>,
    pub tid: u8,
    /// delay before the first read (kept for callers that adjust it)
    pub delay: Duration,
    pub timing: Timing,
    /// the device is wireless: no reply / TIMEOUT status means asleep, not a fault
    pub wireless: bool,
    /// when the device was last found asleep
    asleep_at: Option<Instant>,
}

/// What came back while polling for a reply.
enum Outcome {
    Reply(Report),
    /// only BUSY / NEW until the deadline (or nothing at all)
    Busy(Option<Report>),
    /// only replies to other commands
    Foreign(Report, u32),
}

impl Channel {
    pub fn new(t: Box<dyn Transport>, tid: u8, delay_ms: u64) -> Channel {
        let timing = Timing::new(delay_ms);
        Channel { t, tid, delay: timing.first_delay, timing, wireless: false, asleep_at: None }
    }

    /// Mark the device as wireless (receiver / Bluetooth).
    pub fn with_wireless(mut self, wireless: bool) -> Channel {
        self.wireless = wireless;
        self
    }

    /// Was the device found asleep by the last requests?
    pub fn is_asleep(&self) -> bool {
        self.asleep_at.is_some()
    }

    fn mark_asleep(&mut self) -> Error {
        if self.asleep_at.is_none() {
            log_info!("usb", "the mouse does not answer: asleep or out of range — requests pause until it wakes up");
        }
        self.asleep_at = Some(Instant::now());
        Error::Asleep
    }

    fn poll(&mut self, req: &Report, t0: Instant) -> Result<(Outcome, u32)> {
        let mut delay = self.delay;
        let (mut busy, mut foreign) = (0u32, 0u32);
        let (mut last_busy, mut last_foreign) = (None, None);
        loop {
            let left = self.timing.deadline.saturating_sub(t0.elapsed());
            if left.is_zero() {
                break;
            }
            std::thread::sleep(delay.min(left));
            let r = Report::from_bytes(&self.t.get_feature()?);
            delay = (delay * 8 / 5).clamp(Duration::from_millis(1), self.timing.max_delay);
            if r.status == status::BUSY || r.status == status::NEW {
                busy += 1;
                last_busy = Some(r);
            } else if r.cmd() != req.cmd() {
                foreign += 1;
                last_foreign = Some(r);
            } else {
                return Ok((Outcome::Reply(r), busy));
            }
        }
        Ok(match last_foreign {
            // another program is talking to the device (Synapse / OpenRazer)
            Some(r) if foreign > 0 => (Outcome::Foreign(r, foreign), busy),
            _ => (Outcome::Busy(last_busy), busy),
        })
    }

    pub fn request(&mut self, cmd: u16, args: &[u8], data_size: Option<u8>) -> Result<Report> {
        let req = Report::new(cmd, args, data_size, self.tid)?;
        // a sleeping device is left alone for a while: no 2-second wait on every request
        if let Some(at) = self.asleep_at {
            if at.elapsed() < self.timing.asleep_backoff {
                return Err(Error::Asleep);
            }
        }
        let t0 = Instant::now();
        log_debug!("usb", ">> {}", req);
        self.t.send_feature(&req.to_bytes())?;
        let (outcome, busy) = self.poll(&req, t0)?;
        let ms = t0.elapsed().as_millis() as u64;
        let busy_note = if busy > 0 { format!(", BUSY×{busy}") } else { String::new() };
        let rep = match outcome {
            Outcome::Reply(r) => r,
            Outcome::Foreign(r, n) => {
                log_debug!("usb", "<< {} [{ms} ms{busy_note}, foreign reply×{n}]", r);
                log_warn!("usb", "reply to another command: expected {:04x}, got {:04x}", req.cmd(), r.cmd());
                return Err(Error::Conflict);
            }
            Outcome::Busy(r) => {
                match &r {
                    Some(r) => log_debug!("usb", "<< {} [{ms} ms{busy_note}, no final reply]", r),
                    None => log_debug!("usb", "<< nothing [{ms} ms]"),
                }
                if self.wireless {
                    return Err(self.mark_asleep());
                }
                log_warn!("usb", "no reply to {:04x} within {ms} ms", req.cmd());
                return Err(Error::Timeout { cmd, ms });
            }
        };
        log_debug!("usb", "<< {} [{ms} ms{busy_note}]", rep);
        // the receiver answers for a mouse that is off: status TIMEOUT
        if rep.status == status::TIMEOUT && self.wireless {
            return Err(self.mark_asleep());
        }
        if self.asleep_at.take().is_some() {
            log_info!("usb", "the mouse answers again");
        }
        if rep.status != status::OK {
            return Err(Error::Failed { cmd, status: rep.status });
        }
        Ok(rep)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Answers every request with a fixed status (or with another command).
    struct Stub {
        status: Arc<Mutex<u8>>,
        foreign: bool,
        last: [u8; REPORT_LEN],
        sent: Arc<Mutex<u32>>,
    }

    impl Transport for Stub {
        fn send_feature(&mut self, data: &[u8; REPORT_LEN]) -> Result<()> {
            self.last = *data;
            *self.sent.lock().unwrap() += 1;
            Ok(())
        }
        fn get_feature(&mut self) -> Result<[u8; REPORT_LEN]> {
            let mut r = self.last;
            r[0] = *self.status.lock().unwrap();
            if self.foreign {
                r[7] ^= 0x01;
            }
            Ok(r)
        }
    }

    fn channel(st: u8, foreign: bool, wireless: bool) -> (Channel, Arc<Mutex<u8>>, Arc<Mutex<u32>>) {
        let status = Arc::new(Mutex::new(st));
        let sent = Arc::new(Mutex::new(0));
        let t = Stub { status: status.clone(), foreign, last: [0; REPORT_LEN], sent: sent.clone() };
        let mut ch = Channel::new(Box::new(t), 0x1F, 1).with_wireless(wireless);
        ch.timing.deadline = Duration::from_millis(120);
        ch.timing.asleep_backoff = Duration::from_millis(200);
        (ch, status, sent)
    }

    #[test]
    fn ok_reply() {
        let (mut ch, _, _) = channel(status::OK, false, false);
        assert_eq!(ch.request(0x0081, &[0, 0], None).unwrap().cmd(), 0x0081);
    }

    #[test]
    fn failed_status() {
        let (mut ch, _, _) = channel(status::NOT_SUPPORTED, false, false);
        assert!(matches!(ch.request(0x0081, &[], None), Err(Error::Failed { status: 5, .. })));
    }

    #[test]
    fn busy_wired_is_timeout_within_deadline() {
        let (mut ch, _, _) = channel(status::BUSY, false, false);
        let t0 = Instant::now();
        assert!(matches!(ch.request(0x0081, &[], None), Err(Error::Timeout { cmd: 0x0081, .. })));
        assert!(t0.elapsed() < Duration::from_millis(400));
    }

    #[test]
    fn foreign_reply_is_conflict() {
        let (mut ch, _, _) = channel(status::OK, true, false);
        assert!(matches!(ch.request(0x0081, &[], None), Err(Error::Conflict)));
    }

    #[test]
    fn wireless_asleep_then_awake() {
        let (mut ch, st, sent) = channel(status::BUSY, false, true);
        assert!(matches!(ch.request(0x0780, &[], Some(2)), Err(Error::Asleep)));
        assert!(ch.is_asleep());
        // while asleep: fail at once, nothing is sent
        let n = *sent.lock().unwrap();
        assert!(matches!(ch.request(0x0780, &[], Some(2)), Err(Error::Asleep)));
        assert_eq!(*sent.lock().unwrap(), n);
        // the receiver reports TIMEOUT for a mouse that is off
        std::thread::sleep(Duration::from_millis(220));
        *st.lock().unwrap() = status::TIMEOUT;
        assert!(matches!(ch.request(0x0780, &[], Some(2)), Err(Error::Asleep)));
        // wakes up
        std::thread::sleep(Duration::from_millis(220));
        *st.lock().unwrap() = status::OK;
        assert!(ch.request(0x0780, &[], Some(2)).is_ok());
        assert!(!ch.is_asleep());
    }

    #[test]
    fn wired_timeout_status_is_failure() {
        let (mut ch, _, _) = channel(status::TIMEOUT, false, false);
        assert!(matches!(ch.request(0x0081, &[], None), Err(Error::Failed { status: 4, .. })));
    }
}
