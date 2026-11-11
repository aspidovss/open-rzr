//! Capabilities: what a device can do, as separate traits instead of one large type.
//!
//! `RzrDevice` is what every device has. Each capability is reached through an accessor that
//! returns `None` when the device (or the way it is connected) does not have it, so callers can
//! build their pages from what is there:
//!
//! ```ignore
//! if let Some(b) = dev.battery() { println!("{} %", b.level()?); }
//! if let Some(d) = dev.dpi() { d.set_dpi(1600, 1600)?; }
//! ```
//!
//! A new capability is a new trait plus an accessor with a `None` default here; devices that do
//! not have it need no change. Laptop power and cooling (`BladePower…`) and docks will come with
//! hardware to test them on.

use crate::device::{DpiStages, RazerMouse};
use crate::devices::feat;
use crate::lighting::ZoneState;
use crate::protocol::Result;

/// How the device is connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Wired,
    /// through a USB receiver (HyperSpeed)
    Receiver,
    Bluetooth,
}

impl Connection {
    pub fn code(self) -> &'static str {
        match self {
            Connection::Wired => "wired",
            Connection::Receiver => "receiver",
            Connection::Bluetooth => "bluetooth",
        }
    }
}

/// What every device has.
pub trait RzrDevice {
    fn name(&self) -> &str;
    fn vid(&self) -> u16;
    fn pid(&self) -> u16;
    fn connection(&self) -> Connection;
    fn firmware_version(&mut self) -> Result<String>;
    fn serial_number(&mut self) -> Result<String>;
    /// Names of the capabilities available now ("battery", "dpi", "polling_rate", "lighting").
    fn capabilities(&mut self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.battery().is_some() {
            v.push("battery");
        }
        if self.dpi().is_some() {
            v.push("dpi");
        }
        if self.polling_rate().is_some() {
            v.push("polling_rate");
        }
        if self.lighting().is_some() {
            v.push("lighting");
        }
        v
    }

    fn battery(&mut self) -> Option<&mut dyn BatteryCapability> {
        None
    }
    fn dpi(&mut self) -> Option<&mut dyn DpiCapability> {
        None
    }
    fn polling_rate(&mut self) -> Option<&mut dyn PollingRateCapability> {
        None
    }
    fn lighting(&mut self) -> Option<&mut dyn LightingCapability> {
        None
    }
}

pub trait BatteryCapability {
    /// 0..=100 %
    fn level(&mut self) -> Result<u8>;
    /// `Ok(false)` when the device cannot tell
    fn charging(&mut self) -> Result<bool>;
}

pub trait DpiCapability {
    fn dpi(&mut self) -> Result<(u16, u16)>;
    fn set_dpi(&mut self, x: u16, y: u16) -> Result<()>;
    /// (min, max)
    fn dpi_range(&self) -> (u16, u16);
    /// `None` when the device has no DPI stages
    fn max_stages(&self) -> Option<usize>;
    fn stages(&mut self) -> Result<DpiStages>;
    fn set_stages(&mut self, stages: &[(u16, u16)], active: u8) -> Result<()>;
}

pub trait PollingRateCapability {
    fn rate(&mut self) -> Result<u32>;
    fn set_rate(&mut self, hz: u32) -> Result<()>;
    fn supported_rates(&self) -> &'static [u32];
}

pub trait LightingCapability {
    /// zone ids: "scroll", "logo", "side", …
    fn zones(&self) -> Vec<&'static str>;
    /// effect names of a zone: "static", "spectrum", "breath_single", …
    fn effects(&self, zone: &str) -> Vec<&'static str>;
    fn set_effect(&mut self, zone: &str, state: &ZoneState) -> Result<()>;
    fn brightness(&mut self, zone: &str) -> Result<u8>;
    fn set_brightness(&mut self, zone: &str, percent: u8) -> Result<()>;
}

// ------------------------------------------------------------------ mice

/// Rates of the legacy polling command (125 / 500 / 1000 Hz).
const RATES: &[u32] = &[125, 500, 1000];

impl RzrDevice for RazerMouse {
    fn name(&self) -> &str {
        self.spec.name
    }
    fn vid(&self) -> u16 {
        self.info.vid
    }
    fn pid(&self) -> u16 {
        self.info.pid
    }
    fn connection(&self) -> Connection {
        if self.info.bluetooth || self.spec.bluetooth {
            Connection::Bluetooth
        } else if self.spec.wireless {
            Connection::Receiver
        } else {
            Connection::Wired
        }
    }
    fn firmware_version(&mut self) -> Result<String> {
        self.firmware()
    }
    fn serial_number(&mut self) -> Result<String> {
        self.serial()
    }
    fn battery(&mut self) -> Option<&mut dyn BatteryCapability> {
        if self.has(feat::BATTERY) {
            Some(self)
        } else {
            None
        }
    }
    fn dpi(&mut self) -> Option<&mut dyn DpiCapability> {
        if self.has(feat::DPI) {
            Some(self)
        } else {
            None
        }
    }
    fn polling_rate(&mut self) -> Option<&mut dyn PollingRateCapability> {
        if self.has(feat::POLLING) {
            Some(self)
        } else {
            None
        }
    }
    fn lighting(&mut self) -> Option<&mut dyn LightingCapability> {
        if self.has(feat::LIGHTING) && !self.zones.is_empty() {
            Some(self)
        } else {
            None
        }
    }
}

impl BatteryCapability for RazerMouse {
    fn level(&mut self) -> Result<u8> {
        self.get_battery()
    }
    fn charging(&mut self) -> Result<bool> {
        if self.has(feat::CHARGING) {
            self.is_charging()
        } else {
            Ok(false)
        }
    }
}

impl DpiCapability for RazerMouse {
    fn dpi(&mut self) -> Result<(u16, u16)> {
        self.get_dpi()
    }
    fn set_dpi(&mut self, x: u16, y: u16) -> Result<()> {
        RazerMouse::set_dpi(self, x, y)
    }
    fn dpi_range(&self) -> (u16, u16) {
        (self.spec.min_dpi, self.spec.max_dpi)
    }
    fn max_stages(&self) -> Option<usize> {
        self.has(feat::DPI_STAGES).then_some(self.spec.max_dpi_stages)
    }
    fn stages(&mut self) -> Result<DpiStages> {
        self.get_dpi_stages(crate::device::VARSTORE)
    }
    fn set_stages(&mut self, stages: &[(u16, u16)], active: u8) -> Result<()> {
        self.set_dpi_stages(stages, active, crate::device::VARSTORE)
    }
}

impl PollingRateCapability for RazerMouse {
    fn rate(&mut self) -> Result<u32> {
        self.get_polling_rate()
    }
    fn set_rate(&mut self, hz: u32) -> Result<()> {
        self.set_polling_rate(hz)
    }
    fn supported_rates(&self) -> &'static [u32] {
        RATES
    }
}

impl LightingCapability for RazerMouse {
    fn zones(&self) -> Vec<&'static str> {
        self.zones.iter().map(|z| z.id).collect()
    }
    fn effects(&self, zone: &str) -> Vec<&'static str> {
        self.zone_effects(zone)
    }
    fn set_effect(&mut self, zone: &str, state: &ZoneState) -> Result<()> {
        self.set_lighting(zone, state)
    }
    fn brightness(&mut self, zone: &str) -> Result<u8> {
        self.get_brightness(zone)
    }
    fn set_brightness(&mut self, zone: &str, percent: u8) -> Result<()> {
        RazerMouse::set_brightness(self, zone, percent)
    }
}

// ------------------------------------------------------------------ description files

/// Load `<config dir>/devices/*.toml|*.json` and log what was found.
pub fn load_device_files() -> Vec<crate::devices::external::Loaded> {
    let dir = crate::devices::external::folder(&crate::logs::config_dir());
    let report = crate::devices::external::load_dir(&dir);
    for l in &report {
        if l.error.is_empty() {
            log_info!("app", "device file {}: 0x{:04x} {} (based on {})", l.file, l.pid, l.name, l.base);
        } else {
            log_warn!("app", "device file {}: {}", l.file, l.error);
        }
        for w in &l.warnings {
            log_warn!("app", "device file {}: {w}", l.file);
        }
    }
    report
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::transport::DeviceInfo;

    #[test]
    fn mouse_capabilities() {
        let mut m = RazerMouse::open(&DeviceInfo::mock()).unwrap();
        let d: &mut dyn RzrDevice = &mut m;
        assert_eq!((d.vid(), d.pid(), d.connection()), (0x1532, 0x0090, Connection::Receiver));
        assert!(d.capabilities().contains(&"battery"));
        assert!(d.firmware_version().unwrap().starts_with('v'));
        let caps = d.capabilities();
        assert!(caps.contains(&"dpi") && caps.contains(&"lighting") && caps.contains(&"polling_rate"));
        let dpi = d.dpi().unwrap();
        dpi.set_dpi(1600, 1600).unwrap();
        assert_eq!(dpi.dpi().unwrap(), (1600, 1600));
        assert!(dpi.dpi_range().1 >= 16000);
        let p = d.polling_rate().unwrap();
        p.set_rate(500).unwrap();
        assert_eq!(p.rate().unwrap(), 500);
        assert!(p.set_rate(333).is_err());
        let l = d.lighting().unwrap();
        assert!(l.zones().contains(&"logo"));
        assert!(l.effects("logo").contains(&"static"));
    }
}
