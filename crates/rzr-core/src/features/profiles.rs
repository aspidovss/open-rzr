//! Whole profiles: read an on-board slot into a `Profile`, write a `Profile` into a slot
//! or into the temporary layer (slot 0: applies at once, kept in RAM, no flash wear).

use std::time::Instant;

use crate::bindings::Action;
use crate::device::RazerMouse;
use crate::devices::feat;
use crate::profile::{key, parse_key, ButtonPair, Profile};
use crate::protocol::{Error, Result};

/// Slot 0 = temporary layer.
pub const TEMPORARY: u8 = 0;

impl RazerMouse {
    /// Read on-board slot `slot` (1..=5) — or the temporary layer (0) — as a profile.
    pub fn read_profile(&mut self, slot: u8, name: &str) -> Result<Profile> {
        self.need(feat::BUTTONS)?;
        let t0 = Instant::now();
        let mut p = Profile { name: name.into(), device: self.spec.name.into(), pid: self.info.pid, ..Default::default() };
        // the temporary layer has the same buttons as slot 1; scan there, read values from slot 0
        let scan_slot = if slot == TEMPORARY { 1 } else { slot };
        for b in self.scan_buttons(scan_slot, false, true)? {
            let normal = if slot == TEMPORARY { self.get_button(b.id, 0, false)? } else { b.action };
            let hypershift = self.get_button(b.id, slot, true).ok();
            p.buttons.insert(key(b.id), ButtonPair { normal, hypershift });
        }
        if self.has(feat::DPI_STAGES) && slot != TEMPORARY {
            p.dpi_stages = self.get_dpi_stages(slot).ok();
        }
        if self.has(feat::POLLING) {
            p.polling = self.get_polling_rate().ok();
        }
        if self.has(feat::MACROS) {
            for id in p.used_macros() {
                if let Ok(steps) = self.read_macro(id) {
                    p.macros.insert(id.to_string(), steps);
                }
            }
        }
        log_info!(
            "device",
            "read_profile slot {slot}: {} buttons, {} macros [{} ms]",
            p.buttons.len(),
            p.macros.len(),
            t0.elapsed().as_millis()
        );
        Ok(p)
    }

    /// Write `p` into slot `slot` (0 = temporary layer). With `previous` (what the slot holds now)
    /// only the differences are written. Returns the number of records written.
    pub fn write_profile(&mut self, p: &Profile, slot: u8, previous: Option<&Profile>) -> Result<usize> {
        self.need(feat::BUTTONS)?;
        let t0 = Instant::now();
        let mut n = 0;
        // macros first: bindings may refer to them
        if self.has(feat::MACROS) && !p.macros.is_empty() {
            let on_mouse = self.macro_ids().unwrap_or_default();
            for (k, steps) in &p.macros {
                let id: u16 = k.parse().map_err(|_| Error::Invalid(format!("macro id {k}")))?;
                let same = on_mouse.contains(&id) && self.read_macro(id).map(|s| &s == steps).unwrap_or(false);
                if !same {
                    self.write_macro(id, steps)?;
                    n += 1;
                }
            }
        }
        for (k, pair) in &p.buttons {
            let id = parse_key(k).ok_or_else(|| Error::Invalid(format!("button id {k}")))?;
            let old = previous.and_then(|q| q.buttons.get(k));
            if old.map(|o| o.normal != pair.normal).unwrap_or(true) {
                self.set_button(id, &pair.normal, slot, false)?;
                n += 1;
            }
            if let Some(h) = &pair.hypershift {
                if old.map(|o| o.hypershift.as_ref() != Some(h)).unwrap_or(true) {
                    self.set_button(id, h, slot, true)?;
                    n += 1;
                }
            }
        }
        if let Some(st) = &p.dpi_stages {
            if self.has(feat::DPI_STAGES) && !st.stages.is_empty() && previous.map(|q| q.dpi_stages.as_ref() != Some(st)).unwrap_or(true) {
                if slot == TEMPORARY {
                    // no stage storage for the temporary layer: set the active stage's DPI
                    if let Some(&(x, y)) = st.stages.get(st.active.saturating_sub(1) as usize) {
                        self.set_dpi(x, y)?;
                    }
                } else {
                    self.set_dpi_stages(&st.stages, st.active, slot)?;
                }
                n += 1;
            }
        }
        log_info!("device", "write_profile \"{}\" -> slot {slot}: {n} records [{} ms]", p.name, t0.elapsed().as_millis());
        Ok(n)
    }

    /// Global settings of a profile that the mouse does not keep per slot: polling rate and lighting.
    pub fn apply_profile_globals(&mut self, p: &Profile) -> Result<()> {
        if let (Some(hz), true) = (p.polling, self.has(feat::POLLING)) {
            self.set_polling_rate(hz)?;
        }
        for (zone, st) in &p.lighting {
            if self.zone(zone).is_ok() {
                self.set_lighting(zone, st)?;
                self.set_brightness(zone, st.brightness)?;
            }
        }
        Ok(())
    }

    /// Factory profile for this mouse: every known button with its default binding.
    pub fn factory_profile(&mut self, name: &str) -> Result<Profile> {
        let mut p = self.read_profile(1, name)?;
        p.reset_to(self.spec.defaults);
        p.dpi_stages = Some(crate::features::dpi::DpiStages {
            active: 2,
            stages: vec![(400, 400), (800, 800), (1600, 1600), (3200, 3200), (6400, 6400)],
        });
        Ok(p)
    }

    /// Hypershift layer operation ("copy" | "clear", see `Profile::layer_plan`) in a profile and on the mouse.
    pub fn apply_layer(&mut self, p: &mut Profile, slot: u8, op: &str) -> Result<usize> {
        self.need(feat::BUTTONS)?;
        let plan = p.layer_plan(op).map_err(Error::Invalid)?;
        for (id, a) in &plan {
            self.set_button(*id, a, slot, true)?;
            p.set(*id, true, a.clone());
        }
        log_info!("device", "Hypershift layer {op} \"{}\" -> slot {slot}: {} buttons", p.name, plan.len());
        Ok(plan.len())
    }

    /// Set one binding in a profile and on the mouse at once.
    pub fn set_profile_button(&mut self, p: &mut Profile, slot: u8, id: u8, hypershift: bool, a: &Action) -> Result<Action> {
        self.set_button(id, a, slot, hypershift)?;
        let back = self.get_button(id, slot, hypershift).unwrap_or_else(|_| a.clone());
        p.set(id, hypershift, back.clone());
        Ok(back)
    }
}
