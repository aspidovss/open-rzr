//! Sensitivity (DPI), DPI stages, polling rate.

use crate::device::{RazerMouse, VARSTORE};
use crate::devices::feat;
use crate::protocol::{cmd, polling_code, polling_hz, Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct DpiStages {
    /// 1-based
    pub active: u8,
    pub stages: Vec<(u16, u16)>,
}

impl RazerMouse {
    pub fn get_dpi(&mut self) -> Result<(u16, u16)> {
        self.need(feat::DPI)?;
        let a = self.req(cmd::GET_DPI, &[0], Some(7))?;
        Ok((u16::from_be_bytes([a[1], a[2]]), u16::from_be_bytes([a[3], a[4]])))
    }

    fn clamp_dpi(&self, v: u16) -> u16 {
        v.clamp(self.spec.min_dpi, self.spec.max_dpi)
    }

    pub fn set_dpi(&mut self, x: u16, y: u16) -> Result<()> {
        self.need(feat::DPI)?;
        let (x, y) = (self.clamp_dpi(x), self.clamp_dpi(y));
        self.op(format!("set_dpi {x}x{y}"), |m| {
            let mut a = vec![VARSTORE];
            a.extend(x.to_be_bytes());
            a.extend(y.to_be_bytes());
            a.extend([0, 0]);
            m.req(cmd::SET_DPI, &a, Some(7)).map(|_| ())
        })
    }

    pub fn get_dpi_stages(&mut self, store: u8) -> Result<DpiStages> {
        self.need(feat::DPI_STAGES)?;
        let a = self.req(cmd::GET_DPI_STAGES, &[store], Some(0x26))?;
        let n = (a[2] as usize).min(self.spec.max_dpi_stages);
        let stages = (0..n)
            .map(|i| {
                let o = 3 + i * 7;
                (u16::from_be_bytes([a[o + 1], a[o + 2]]), u16::from_be_bytes([a[o + 3], a[o + 4]]))
            })
            .collect();
        Ok(DpiStages { active: a[1], stages })
    }

    pub fn set_dpi_stages(&mut self, stages: &[(u16, u16)], active: u8, store: u8) -> Result<()> {
        self.need(feat::DPI_STAGES)?;
        if stages.is_empty() || stages.len() > self.spec.max_dpi_stages {
            return Err(Error::Invalid(format!("1 to {} stages", self.spec.max_dpi_stages)));
        }
        let active = active.clamp(1, stages.len() as u8);
        let mut a = vec![store, active, stages.len() as u8];
        for (i, (x, y)) in stages.iter().enumerate() {
            a.push(i as u8);
            a.extend(self.clamp_dpi(*x).to_be_bytes());
            a.extend(self.clamp_dpi(*y).to_be_bytes());
            a.extend([0, 0]);
        }
        self.op(format!("set_dpi_stages {stages:?} active {active} (profile {store})"), |m| {
            m.req(cmd::SET_DPI_STAGES, &a, Some(0x26)).map(|_| ())
        })
    }

    pub fn get_polling_rate(&mut self) -> Result<u32> {
        self.need(feat::POLLING)?;
        Ok(polling_hz(self.req(cmd::GET_POLLING, &[], Some(1))?[0]))
    }

    pub fn set_polling_rate(&mut self, hz: u32) -> Result<()> {
        self.need(feat::POLLING)?;
        let code = polling_code(hz).ok_or_else(|| Error::Invalid("supported: 125, 500, 1000 Hz".into()))?;
        self.op(format!("set_polling_rate {hz} Hz"), |m| m.req(cmd::SET_POLLING, &[code], Some(1)).map(|_| ()))
    }
}
