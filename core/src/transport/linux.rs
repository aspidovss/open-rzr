//! Linux hidraw transport: ioctl(HIDIOCSFEATURE / HIDIOCGFEATURE), no external crates.

use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::raw::{c_int, c_ulong};
use std::path::Path;

use super::{DeviceInfo, Transport};
use crate::devices::RAZER_VID;
use crate::protocol::{Error, Result, REPORT_LEN};

const FEATURE_LEN: usize = REPORT_LEN + 1; // + report id 0

const fn ioc_rw(nr: c_ulong, size: c_ulong) -> c_ulong {
    (3 << 30) | (size << 16) | ((b'H' as c_ulong) << 8) | nr
}
const HIDIOCSFEATURE: c_ulong = ioc_rw(0x06, FEATURE_LEN as c_ulong);
const HIDIOCGFEATURE: c_ulong = ioc_rw(0x07, FEATURE_LEN as c_ulong);

extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}

pub struct Hidraw {
    f: File,
}

impl Hidraw {
    pub fn open(path: &str) -> Result<Hidraw> {
        match OpenOptions::new().read(true).write(true).open(path) {
            Ok(f) => Ok(Hidraw { f }),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(Error::NoAccess(format!(
                "No access to {path}. A udev rule is needed: open-rzr-cli install-udev, \
                 then re-plug the mouse."
            ))),
            Err(e) => Err(Error::Io(format!("{path}: {e}"))),
        }
    }
}

impl Transport for Hidraw {
    fn send_feature(&mut self, data: &[u8; REPORT_LEN]) -> Result<()> {
        let mut buf = [0u8; FEATURE_LEN];
        buf[1..].copy_from_slice(data);
        let r = unsafe { ioctl(self.f.as_raw_fd(), HIDIOCSFEATURE, buf.as_mut_ptr()) };
        if r < 0 {
            return Err(Error::Io(format!("HIDIOCSFEATURE: {}", std::io::Error::last_os_error())));
        }
        Ok(())
    }

    fn get_feature(&mut self) -> Result<[u8; REPORT_LEN]> {
        let mut buf = [0u8; FEATURE_LEN];
        let r = unsafe { ioctl(self.f.as_raw_fd(), HIDIOCGFEATURE, buf.as_mut_ptr()) };
        if r < 0 {
            return Err(Error::Io(format!("HIDIOCGFEATURE: {}", std::io::Error::last_os_error())));
        }
        let mut out = [0u8; REPORT_LEN];
        out.copy_from_slice(&buf[1..]);
        Ok(out)
    }
}

pub fn enumerate() -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else { return out };
    let mut nodes: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    nodes.sort();
    for node in nodes {
        let Ok(uevent) = fs::read_to_string(node.join("device/uevent")) else { continue };
        let mut hid_id = "";
        let mut name = "";
        for line in uevent.lines() {
            if let Some(v) = line.strip_prefix("HID_ID=") {
                hid_id = v;
            } else if let Some(v) = line.strip_prefix("HID_NAME=") {
                name = v;
            }
        }
        let parts: Vec<&str> = hid_id.split(':').collect();
        if parts.len() != 3 {
            continue;
        }
        let (Ok(vid), Ok(pid)) = (u32::from_str_radix(parts[1], 16), u32::from_str_radix(parts[2], 16)) else {
            continue;
        };
        if vid as u16 != RAZER_VID {
            continue;
        }
        // .../1-3:1.0/0003:1532:008F.0005 -> interface 0
        let iface = fs::canonicalize(node.join("device"))
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .and_then(|s| if s.contains(':') { s.rsplit('.').next().and_then(|x| x.parse().ok()) } else { None })
            .unwrap_or(-1);
        let dev = node.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        out.push(DeviceInfo {
            path: format!("/dev/{dev}"),
            vid: vid as u16,
            pid: pid as u16,
            interface: iface,
            name: name.to_string(),
            backend: "hidraw".into(),
            usage_page: 0,
            usage: 0,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn ioctl_numbers() {
        // HIDIOCSFEATURE(91) / HIDIOCGFEATURE(91) from <linux/hidraw.h>
        assert_eq!(super::HIDIOCSFEATURE, 0xC05B4806);
        assert_eq!(super::HIDIOCGFEATURE, 0xC05B4807);
    }
}
