//! Backup and restore of bindings, DPI stages and macros.

use crate::bindings::Action;
use crate::device::RazerMouse;
use crate::devices::feat;
use crate::features::dpi::DpiStages;
use crate::protocol::{Error, Result};

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ProfileBackup {
    pub profile: u8,
    /// (button id, normal record, hypershift record) — 7 bytes each
    pub buttons: Vec<(u8, Vec<u8>, Option<Vec<u8>>)>,
    pub dpi_stages: Option<DpiStages>,
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct Backup {
    pub device: String,
    pub pid: u16,
    pub firmware: String,
    pub profiles: Vec<ProfileBackup>,
    /// (macro id, raw encoded bytes)
    pub macros: Vec<(u16, Vec<u8>)>,
}

impl RazerMouse {
    pub fn backup(&mut self, profiles: &[u8]) -> Result<Backup> {
        self.need(feat::BUTTONS)?;
        let mut b = Backup {
            device: self.spec.name.into(),
            pid: self.info.pid,
            firmware: self.firmware()?,
            ..Default::default()
        };
        for &p in profiles {
            let mut pb = ProfileBackup { profile: p, ..Default::default() };
            for btn in self.scan_buttons(p, false, true)? {
                let normal = btn.action.encode().map_err(Error::Invalid)?.to_vec();
                let hs = self.get_button_raw(btn.id, p, true).ok().map(|r| r.to_vec());
                pb.buttons.push((btn.id, normal, hs));
            }
            pb.dpi_stages = if self.has(feat::DPI_STAGES) { self.get_dpi_stages(p).ok() } else { None };
            b.profiles.push(pb);
        }
        if self.has(feat::MACROS) {
            if let Ok(ids) = self.macro_ids() {
                for id in ids {
                    b.macros.push((id, self.read_macro_raw(id)?));
                }
            }
        }
        log_info!("device", "backup: profiles {profiles:?}, macros {}", b.macros.len());
        Ok(b)
    }

    pub fn restore(&mut self, b: &Backup) -> Result<usize> {
        if b.pid != self.info.pid && b.device != self.spec.name {
            log_warn!("device", "restore: backup of {} ({:04x}) onto {}", b.device, b.pid, self.spec.name);
        }
        let mut n = 0;
        for (id, data) in &b.macros {
            self.write_macro_raw(*id, data)?;
            n += 1;
        }
        for p in &b.profiles {
            for (id, normal, hs) in &p.buttons {
                self.set_button(*id, &Action::decode(normal), p.profile, false)?;
                n += 1;
                if let Some(h) = hs {
                    self.set_button(*id, &Action::decode(h), p.profile, true)?;
                    n += 1;
                }
            }
            if let Some(st) = &p.dpi_stages {
                if !st.stages.is_empty() {
                    self.set_dpi_stages(&st.stages, st.active, p.profile)?;
                    n += 1;
                }
            }
        }
        log_info!("device", "restore: {n} records written");
        Ok(n)
    }
}
