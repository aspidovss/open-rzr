//! Windows transport through hidapi.

use std::ffi::CString;

use hidapi::{HidApi, HidDevice};

use super::{DeviceInfo, Transport};
use crate::devices::RAZER_VID;
use crate::protocol::{Error, Result, REPORT_LEN};

const FEATURE_LEN: usize = REPORT_LEN + 1;

pub struct Hidapi {
    dev: HidDevice,
}

impl Hidapi {
    pub fn open(path: &str) -> Result<Hidapi> {
        let api = HidApi::new().map_err(|e| Error::Io(format!("hidapi: {e}")))?;
        let c = CString::new(path).map_err(|_| Error::Invalid("device path".into()))?;
        let dev = api.open_path(&c).map_err(|e| Error::Io(format!("{path}: {e}")))?;
        Ok(Hidapi { dev })
    }
}

impl Transport for Hidapi {
    fn send_feature(&mut self, data: &[u8; REPORT_LEN]) -> Result<()> {
        let mut buf = [0u8; FEATURE_LEN];
        buf[1..].copy_from_slice(data);
        self.dev.send_feature_report(&buf).map_err(|e| Error::Io(format!("send_feature_report: {e}")))
    }

    fn get_feature(&mut self) -> Result<[u8; REPORT_LEN]> {
        let mut buf = [0u8; FEATURE_LEN];
        buf[0] = 0;
        let n = self
            .dev
            .get_feature_report(&mut buf)
            .map_err(|e| Error::Io(format!("get_feature_report: {e}")))?;
        let mut out = [0u8; REPORT_LEN];
        // hidapi on Windows returns the report id as the first byte
        if n >= FEATURE_LEN {
            out.copy_from_slice(&buf[1..FEATURE_LEN]);
        } else {
            let m = n.min(REPORT_LEN);
            out[..m].copy_from_slice(&buf[..m]);
        }
        Ok(out)
    }
}

pub fn enumerate() -> std::result::Result<Vec<DeviceInfo>, String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for d in api.device_list() {
        if d.vendor_id() != RAZER_VID {
            continue;
        }
        out.push(DeviceInfo {
            path: d.path().to_string_lossy().to_string(),
            vid: d.vendor_id(),
            pid: d.product_id(),
            interface: d.interface_number(),
            name: d.product_string().unwrap_or("").to_string(),
            backend: "hidapi".into(),
            usage_page: d.usage_page(),
            usage: d.usage(),
        });
    }
    Ok(out)
}
