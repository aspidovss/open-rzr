//! A connected Razer mouse: opening, feature checks, logged requests.
//!
//! The operations themselves live in `features/` (one file per feature), the description of each
//! model in `devices/`. Every operation is logged (target "device").

use std::path::PathBuf;
use std::time::Instant;

use crate::bindings::Action;
use crate::compat;
use crate::devices::{self, feat, fx, DeviceSpec, Zone};
use crate::protocol::{Error, Result};
use crate::transport::{self, Channel, DeviceInfo, Transport};

pub use crate::features::backup::{Backup, ProfileBackup};
pub use crate::features::buttons::ButtonInfo;
pub use crate::features::dpi::DpiStages;

pub const DIRECT: u8 = 0; // "profile" 0: applies immediately, not saved
pub const VARSTORE: u8 = 1;

/// Naga side keypad factory defaults: keys 1..9, 0, -, =
pub const NAGA_SIDE_KEYS: [u8; 12] = devices::naga_pro::SIDE_KEYS;

/// Candidate control endpoints of known mice, expected interface first.
pub fn find_devices(include_mock: bool) -> Vec<DeviceInfo> {
    let all = transport::enumerate(include_mock);
    for d in &all {
        log_debug!("app", "HID found: {} {}", d.label(), d.path);
    }
    let mut v: Vec<DeviceInfo> = all
        .into_iter()
        .filter(|d| d.backend == "mock" || devices::spec_for(d.pid).is_some())
        .collect();
    v.sort_by_key(|d| {
        let spec = devices::spec_for(d.pid);
        let want = spec.map(|s| s.interface).unwrap_or(0);
        let verified = spec.is_some_and(|s| s.verified());
        // over Bluetooth the control reports may only be on a vendor-defined collection: try those first
        let vendor_first = spec.is_some_and(|s| s.bluetooth) && d.usage_page < 0xFF00;
        (d.backend == "mock", !verified, d.pid, d.interface != want, vendor_first, d.path.clone())
    });
    v
}

pub struct RazerMouse {
    pub info: DeviceInfo,
    pub spec: &'static DeviceSpec,
    pub(crate) ch: Channel,
    /// Features usable right now (`feat::*`): the model's direct ones, plus the OpenRazer ones when bridged.
    pub features: u32,
    /// Lighting zones usable right now.
    pub zones: Vec<&'static Zone>,
    /// sysfs directory when the OpenRazer driver serves this mouse.
    pub openrazer: Option<PathBuf>,
    /// Features turned on by probing a model without an own module (`probe`): not tested on it.
    pub experimental: u32,
    /// Zones whose brightness the mouse cannot report (it answers FAIL): not asked again.
    pub(crate) brightness_unreadable: Vec<&'static str>,
}

pub(crate) fn hexs(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

impl RazerMouse {
    pub fn open(info: &DeviceInfo) -> Result<RazerMouse> {
        let (t, openrazer) =
            if info.backend == "mock" && std::env::var_os("OPENRZR_OPENRAZER_SYSFS").is_none() {
                (transport::open(info)?, None)
            } else {
                compat::open_transport(info)?
            };
        Ok(RazerMouse::with_transport(info, t, openrazer))
    }

    /// A mouse over a given transport (`openrazer` = sysfs dir when `t` is the OpenRazer bridge).
    pub fn with_transport(info: &DeviceInfo, t: Box<dyn Transport>, openrazer: Option<PathBuf>) -> RazerMouse {
        let spec = devices::spec_for(info.pid).unwrap_or(&devices::naga_pro::DEVICES[1]);
        log_info!("device", "opening {} ({}) — {} module", info.label(), info.path, spec.source.code());
        let bridged = openrazer.is_some();
        let features = spec.direct | if bridged { spec.openrazer } else { 0 };
        let zones: Vec<&'static Zone> = spec.zones.iter().filter(|z| z.direct || (bridged && z.openrazer)).collect();
        log_info!(
            "device",
            "features: {}; lighting: {}",
            devices::feat::names(features).join(", "),
            zones.iter().map(|z| z.id).collect::<Vec<_>>().join(", ")
        );
        RazerMouse {
            info: info.clone(),
            spec,
            ch: Channel::new(t, spec.transaction_id, if bridged { 0 } else { spec.reply_delay_ms }).with_wireless(spec.wireless),
            features,
            zones,
            openrazer,
            experimental: 0,
            brightness_unreadable: Vec::new(),
        }
    }

    /// Database models of the current generation may answer the button, profile and macro
    /// commands of the Naga Pro. Ask with read-only requests; what answers is turned on and
    /// recorded in `experimental`.
    pub fn probe(&mut self) {
        if !matches!(self.spec.source, devices::Source::OpenRazer | devices::Source::User) || !devices::generic::may_have_onboard(self.spec.transaction_id) {
            return;
        }
        let saved = self.features;
        self.features |= feat::BUTTONS | feat::PROFILES | feat::MACROS;
        let mut found = 0;
        if self.get_button(0x01, 1, false).map(|a| a.kind == "mouse").unwrap_or(false) {
            found |= feat::BUTTONS;
            if self.req(crate::protocol::cmd::GET_PROFILE_COUNT, &[], Some(1)).map(|a| a[0] >= 1).unwrap_or(false) {
                found |= feat::PROFILES;
            }
            if self.macro_ids().is_ok() {
                found |= feat::MACROS;
            }
        }
        self.features = saved | found;
        self.experimental = found & !saved;
        if self.experimental != 0 {
            log_info!("device", "probe: {} answer — experimental on this model", feat::names(self.experimental).join(", "));
        } else {
            log_info!("device", "probe: no on-board buttons/macros/profiles");
        }
    }

    /// Open the first endpoint of `candidates` that actually answers.
    pub fn open_first(candidates: &[DeviceInfo]) -> Result<RazerMouse> {
        let mut errors = Vec::new();
        for info in candidates {
            match RazerMouse::open(info).and_then(|mut m| m.firmware().map(|fw| (m, fw))) {
                Ok((mut m, fw)) => {
                    log_info!("device", "connected: {} — firmware {fw}", m.spec.name);
                    m.probe();
                    return Ok(m);
                }
                Err(e) => {
                    log_warn!("device", "{} does not answer: {e}", info.path);
                    errors.push(format!("{}: {e}", info.path));
                }
            }
        }
        if candidates.is_empty() {
            errors.push("no Razer devices found".into());
        }
        if candidates.iter().any(|c| c.bluetooth || devices::spec_for(c.pid).is_some_and(|s| s.bluetooth)) {
            log_warn!("device", "Bluetooth: no HID collection answered the control reports");
            return Err(Error::Unsupported(format!(
                "Over Bluetooth this mouse does not answer setting commands. Connect it by cable or through the HyperSpeed receiver.\n{}",
                errors.join("\n")
            )));
        }
        Err(Error::NotFound(format!("Mouse not found or not answering.\n{}", errors.join("\n"))))
    }

    pub fn name(&self) -> &'static str {
        self.spec.name
    }

    pub fn has(&self, f: u32) -> bool {
        self.features & f != 0
    }

    /// Error unless feature `f` is usable.
    pub(crate) fn need(&self, f: u32) -> Result<()> {
        if self.has(f) {
            return Ok(());
        }
        let name = feat::names(f).join(", ");
        let how = if self.spec.openrazer & f != 0 && self.openrazer.is_none() {
            " (available through the OpenRazer driver on Linux)"
        } else {
            ""
        };
        Err(Error::Unsupported(format!("{}: \"{name}\" is not supported{how}", self.spec.name)))
    }

    pub fn zone(&self, id: &str) -> Result<&'static Zone> {
        self.zones
            .iter()
            .copied()
            .find(|z| z.id == id)
            .ok_or_else(|| Error::Unsupported(format!("zone {id} is not available on this mouse")))
    }

    /// Lighting effects a zone supports (open-rzr effect names).
    pub fn zone_effects(&self, id: &str) -> Vec<&'static str> {
        let Ok(z) = self.zone(id) else { return Vec::new() };
        let mut v: Vec<&'static str> =
            crate::lighting::zone_effects(id).iter().copied().filter(|e| fx::names(z.effects).contains(e)).collect();
        if v.is_empty() {
            v = fx::names(z.effects);
        }
        v
    }

    /// Factory binding of a button, if the model module knows it.
    pub fn default_action(&self, id: u8) -> Option<Action> {
        (self.spec.defaults)(id)
    }

    /// Raw request, returns the 80 argument bytes.
    pub fn req(&mut self, c: u16, args: &[u8], size: Option<u8>) -> Result<[u8; 80]> {
        Ok(self.ch.request(c, args, size)?.arguments)
    }

    /// Log a write-type operation with its duration and result.
    pub(crate) fn op<T, F: FnOnce(&mut Self) -> Result<T>>(&mut self, what: String, f: F) -> Result<T> {
        let t0 = Instant::now();
        let r = f(self);
        let ms = t0.elapsed().as_millis();
        match &r {
            Ok(_) => log_info!("device", "{what} — ok [{ms} ms]"),
            Err(e) => log_error!("device", "{what} — ERROR [{ms} ms]: {e}"),
        }
        r
    }
}
