//! Macros stored in the mouse memory (format in crate::macros).

use crate::device::RazerMouse;
use crate::devices::feat;
use crate::macros::{self, MacroStep};
use crate::protocol::{cmd, Error, Result};

impl RazerMouse {
    pub fn macro_ids(&mut self) -> Result<Vec<u16>> {
        self.need(feat::MACROS)?;
        let a = self.req(cmd::GET_MACRO_COUNT, &[], Some(2))?;
        let count = u16::from_be_bytes([a[0], a[1]]) as usize;
        let mut ids = Vec::new();
        let mut start: u16 = 0;
        while ids.len() < count {
            let a = self.req(cmd::GET_MACRO_LIST, &start.to_be_bytes(), Some(2 + 64))?;
            let before = ids.len();
            for i in 0..32 {
                if ids.len() >= count {
                    break;
                }
                ids.push(u16::from_be_bytes([a[2 + i * 2], a[3 + i * 2]]));
            }
            if ids.len() == before {
                break;
            }
            start += 32;
        }
        Ok(ids)
    }

    pub fn flash_usage(&mut self) -> Result<(u32, u32, u32)> {
        self.need(feat::MACROS)?;
        let a = self.req(cmd::GET_FLASH_USAGE, &[], Some(14))?;
        let be32 = |i: usize| u32::from_be_bytes([a[i], a[i + 1], a[i + 2], a[i + 3]]);
        Ok((be32(2), be32(6), be32(10)))
    }

    pub fn read_macro_raw(&mut self, id: u16) -> Result<Vec<u8>> {
        self.need(feat::MACROS)?;
        let a = self.req(cmd::GET_MACRO_SIZE, &id.to_be_bytes(), Some(6))?;
        let size = u32::from_be_bytes([a[2], a[3], a[4], a[5]]) as usize;
        let mut data = Vec::with_capacity(size);
        while data.len() < size {
            let n = (size - data.len()).min(64);
            let mut args = id.to_be_bytes().to_vec();
            args.extend((data.len() as u32).to_be_bytes());
            args.push(n as u8);
            let a = self.req(cmd::GET_MACRO_DATA, &args, Some(7 + 64))?;
            data.extend_from_slice(&a[7..7 + n]);
        }
        Ok(data)
    }

    pub fn read_macro(&mut self, id: u16) -> Result<Vec<MacroStep>> {
        Ok(macros::decode(&self.read_macro_raw(id)?))
    }

    pub fn write_macro_raw(&mut self, id: u16, data: &[u8]) -> Result<usize> {
        self.need(feat::MACROS)?;
        if data.is_empty() {
            return Err(Error::Invalid("empty macro".into()));
        }
        let data = data.to_vec();
        self.op(format!("write_macro #{id} ({} bytes)", data.len()), |m| {
            if m.macro_ids()?.contains(&id) {
                m.req(cmd::DELETE_MACRO, &id.to_be_bytes(), Some(2))?;
            }
            let mut a = id.to_be_bytes().to_vec();
            a.extend((data.len() as u32).to_be_bytes());
            m.req(cmd::SET_MACRO_SIZE, &a, Some(6))?;
            let mut off = 0;
            while off < data.len() {
                let chunk = &data[off..(off + 64).min(data.len())];
                let mut a = id.to_be_bytes().to_vec();
                a.extend((off as u32).to_be_bytes());
                a.push(chunk.len() as u8);
                a.extend_from_slice(chunk);
                m.req(cmd::SET_MACRO_DATA, &a, Some(7 + chunk.len() as u8))?;
                off += chunk.len();
            }
            if m.read_macro_raw(id)? != data {
                return Err(Error::Io("macro verification failed: data on the mouse differs".into()));
            }
            Ok(data.len())
        })
    }

    pub fn write_macro(&mut self, id: u16, steps: &[MacroStep]) -> Result<usize> {
        let data = macros::encode(steps).map_err(Error::Invalid)?;
        self.write_macro_raw(id, &data)
    }

    pub fn delete_macro(&mut self, id: u16) -> Result<()> {
        self.need(feat::MACROS)?;
        self.op(format!("delete_macro #{id}"), |m| m.req(cmd::DELETE_MACRO, &id.to_be_bytes(), Some(2)).map(|_| ()))
    }

    pub fn free_macro_id(&mut self) -> Result<u16> {
        let used = self.macro_ids()?;
        Ok((1..=u16::MAX).find(|i| !used.contains(i)).unwrap_or(1))
    }
}
