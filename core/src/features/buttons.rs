//! Button bindings, on-board profiles, button scan, profile copy.

use std::time::Instant;

use crate::bindings::Action;
use crate::device::{hexs, RazerMouse};
use crate::devices::{self, feat};
use crate::protocol::{cmd, Error, Result};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ButtonInfo {
    pub id: u8,
    pub name: String,
    pub action: Action,
}

impl RazerMouse {
    /// On-board profiles (always at least [1]).
    pub fn list_profiles(&mut self) -> Vec<u8> {
        if !self.has(feat::PROFILES) {
            return vec![1];
        }
        let r = (|| -> Result<Vec<u8>> {
            let n = self.req(cmd::GET_PROFILE_COUNT, &[], Some(1))?[0] as usize;
            let a = self.req(cmd::GET_PROFILE_LIST, &[], Some(1 + n as u8))?;
            Ok(a[1..1 + n.min(79)].iter().copied().filter(|p| *p != 0).collect())
        })();
        match r {
            Ok(v) if !v.is_empty() => v,
            _ => vec![1],
        }
    }

    pub fn get_button_raw(&mut self, id: u8, profile: u8, hypershift: bool) -> Result<[u8; 7]> {
        self.need(feat::BUTTONS)?;
        let a = self.req(cmd::GET_BUTTON, &[profile, id, hypershift as u8], Some(10))?;
        let mut r = [0u8; 7];
        r.copy_from_slice(&a[3..10]);
        Ok(r)
    }

    pub fn get_button(&mut self, id: u8, profile: u8, hypershift: bool) -> Result<Action> {
        Ok(Action::decode(&self.get_button_raw(id, profile, hypershift)?))
    }

    pub fn set_button(&mut self, id: u8, action: &Action, profile: u8, hypershift: bool) -> Result<()> {
        self.need(feat::BUTTONS)?;
        let rec = action.encode().map_err(Error::Invalid)?;
        let mut a = vec![profile, id, hypershift as u8];
        a.extend(rec);
        let layer = if hypershift { " Hypershift" } else { "" };
        self.op(
            format!("set_button 0x{id:02x} profile {profile}{layer} -> {} [{}]", action.describe(), hexs(&rec)),
            |m| m.req(cmd::SET_BUTTON, &a, Some(10)).map(|_| ()),
        )
    }

    /// Probe button ids; the mouse answers OK only for the ones it has.
    pub fn scan_buttons(&mut self, profile: u8, hypershift: bool, include_disabled: bool) -> Result<Vec<ButtonInfo>> {
        self.need(feat::BUTTONS)?;
        let t0 = Instant::now();
        let mut found = Vec::new();
        for id in 0x01u8..0x80 {
            let act = match self.get_button(id, profile, hypershift) {
                Ok(a) => a,
                Err(e) if e.is_failed() => continue,
                Err(e) => return Err(e),
            };
            let known = devices::button_name(self.spec, id);
            if known.is_none() && act.kind == "disabled" && !include_disabled {
                continue;
            }
            let name = known.unwrap_or_else(|| {
                if self.spec.side_plates && act.kind == "keyboard" && act.modifiers == 0 {
                    if let Some(i) = devices::naga_pro::SIDE_KEYS.iter().position(|k| *k == act.key) {
                        return format!("Side {}", i + 1);
                    }
                }
                format!("Button 0x{id:02x}")
            });
            found.push(ButtonInfo { id, name, action: act });
        }
        log_info!(
            "device",
            "scan_buttons profile {profile}{}: found {} [{} ms]",
            if hypershift { " Hypershift" } else { "" },
            found.len(),
            t0.elapsed().as_millis()
        );
        Ok(found)
    }

    /// Copy every binding (both layers) and DPI stages from one profile to another.
    pub fn copy_profile(&mut self, from: u8, to: u8) -> Result<usize> {
        self.need(feat::PROFILES)?;
        let t0 = Instant::now();
        let mut n = 0;
        for hs in [false, true] {
            for b in self.scan_buttons(from, hs, true)? {
                self.set_button(b.id, &b.action, to, hs)?;
                n += 1;
            }
        }
        if self.has(feat::DPI_STAGES) {
            if let Ok(st) = self.get_dpi_stages(from) {
                self.set_dpi_stages(&st.stages, st.active, to)?;
            }
        }
        log_info!("device", "copy_profile {from} -> {to}: {n} bindings [{} ms]", t0.elapsed().as_millis());
        Ok(n)
    }
}
