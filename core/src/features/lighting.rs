//! Lighting zones: effects and brightness (packet formats in crate::lighting).

use crate::device::{RazerMouse, VARSTORE};
use crate::lighting::{self, ZoneState};
use crate::protocol::{cmd, Error, Result};

impl RazerMouse {
    pub fn set_lighting(&mut self, zone: &str, st: &ZoneState) -> Result<()> {
        let z = self.zone(zone)?;
        if !self.zone_effects(zone).contains(&st.effect.as_str()) {
            return Err(Error::Unsupported(format!("effect {} is not available for zone {zone}", st.effect)));
        }
        let packets = lighting::zone_commands_led(zone, z.led, st).map_err(Error::Invalid)?;
        self.op(format!("set_lighting {zone}: {} {}", st.effect, lighting::hex_color(st.color1)), |m| {
            for (c, a, s) in &packets {
                m.req(*c, a, Some(*s))?;
            }
            Ok(())
        })
    }

    pub fn get_brightness(&mut self, zone: &str) -> Result<u8> {
        let z = self.zone(zone)?;
        let led = lighting::brightness_led_of(zone, z.led);
        let a = self.req(cmd::GET_BRIGHTNESS, &[VARSTORE, led], Some(3))?;
        Ok(((a[2] as u32 * 100 + 127) / 255) as u8)
    }

    pub fn set_brightness(&mut self, zone: &str, percent: u8) -> Result<()> {
        let z = self.zone(zone)?;
        let raw = ((percent.min(100) as u32 * 255 + 50) / 100) as u8;
        let led = lighting::brightness_led_of(zone, z.led);
        self.op(format!("set_brightness {zone} {percent}%"), |m| {
            m.req(cmd::SET_BRIGHTNESS, &[VARSTORE, led, raw], Some(3)).map(|_| ())
        })
    }
}
