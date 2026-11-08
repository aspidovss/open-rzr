//! Firmware, serial number, side plate.

use crate::device::{hexs, RazerMouse};
use crate::devices::{self, feat};
use crate::protocol::{cmd, Error, Result};

impl RazerMouse {
    pub fn firmware(&mut self) -> Result<String> {
        self.need(feat::FIRMWARE)?;
        let a = self.req(cmd::GET_FIRMWARE, &[], Some(2))?;
        Ok(format!("v{}.{}", a[0], a[1]))
    }

    pub fn serial(&mut self) -> Result<String> {
        self.need(feat::SERIAL)?;
        let a = self.req(cmd::GET_SERIAL, &[], Some(0x16))?;
        let end = a[..22].iter().position(|&b| b == 0).unwrap_or(22);
        Ok(String::from_utf8_lossy(&a[..end]).trim().to_string())
    }

    /// Attached side plate: Some(12|6|2), Some(0) = none attached, None = mouse has no swappable plates.
    pub fn side_plate(&mut self) -> Result<Option<u8>> {
        if !self.spec.side_plates || !self.has(feat::SIDE_PLATE) {
            return Ok(None);
        }
        let a = self.req(cmd::GET_SIDE_PLATE, &[], Some(0x02))?;
        // The byte position is not documented; take the first byte that is a valid plate code.
        let plate = a[..4].iter().find_map(|b| devices::plate_from_code(*b).filter(|p| *p != 0));
        let plate = plate.or_else(|| if a[..4].iter().all(|b| *b == 0) { Some(0) } else { None });
        log_debug!("device", "side_plate: reply {} -> {:?}", hexs(&a[..4]), plate);
        match plate {
            Some(p) => Ok(Some(p)),
            None => Err(Error::Io(format!("unexpected side plate reply: {}", hexs(&a[..4])))),
        }
    }
}
