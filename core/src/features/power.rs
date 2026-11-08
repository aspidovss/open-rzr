//! Battery, charging, sleep timer, low power mode.

use crate::device::RazerMouse;
use crate::devices::feat;
use crate::protocol::{cmd, Result};

impl RazerMouse {
    pub fn get_battery(&mut self) -> Result<u8> {
        self.need(feat::BATTERY)?;
        let a = self.req(cmd::GET_BATTERY, &[], Some(2))?;
        Ok(((a[1] as u32 * 100 + 127) / 255) as u8)
    }

    pub fn is_charging(&mut self) -> Result<bool> {
        self.need(feat::CHARGING)?;
        Ok(self.req(cmd::GET_CHARGING, &[], Some(2))?[1] != 0)
    }

    pub fn get_idle_time(&mut self) -> Result<u16> {
        self.need(feat::IDLE)?;
        let a = self.req(cmd::GET_IDLE_TIME, &[], Some(2))?;
        Ok(u16::from_be_bytes([a[0], a[1]]))
    }

    pub fn set_idle_time(&mut self, seconds: u16) -> Result<()> {
        self.need(feat::IDLE)?;
        let s = seconds.clamp(60, 900);
        self.op(format!("set_idle_time {s} s"), |m| m.req(cmd::SET_IDLE_TIME, &s.to_be_bytes(), Some(2)).map(|_| ()))
    }

    pub fn get_low_battery_threshold(&mut self) -> Result<u8> {
        self.need(feat::LOW_BATTERY)?;
        let a = self.req(cmd::GET_LOW_BATTERY, &[], Some(1))?;
        Ok(((a[0] as u32 * 100 + 127) / 255) as u8)
    }

    pub fn set_low_battery_threshold(&mut self, percent: u8) -> Result<()> {
        self.need(feat::LOW_BATTERY)?;
        // Synapse calls this "Low power mode below N %", range 5..100
        let percent = percent.clamp(5, 100);
        let raw = (percent as u32 * 255 / 100) as u8;
        self.op(format!("set_low_power_threshold {percent}%"), |m| {
            m.req(cmd::SET_LOW_BATTERY, &[raw], Some(1)).map(|_| ())
        })
    }
}
