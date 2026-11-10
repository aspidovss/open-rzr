//! In-memory emulator of a Naga Pro, for tests and for trying the app without hardware.
//! Button ids match what a real Naga Pro reports (12-button plate 0x40.., 6-button plate 0x50..).

use std::collections::{BTreeMap, HashMap};

use super::Transport;
use crate::protocol::{cmd, status, Report, Result, REPORT_LEN};

const SIDE_KEYS: [u8; 12] = [0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E];

pub struct MockNagaPro {
    reply: Report,
    pub dpi: (u16, u16),
    pub stages: HashMap<u8, (u8, Vec<(u16, u16)>)>,
    pub poll: u8,
    pub battery: u8,
    pub idle: u16,
    pub low_batt: u8,
    pub macros: BTreeMap<u16, Vec<u8>>,
    pub buttons: HashMap<(u8, u8, u8), [u8; 7]>,
    pub brightness: HashMap<u8, u8>,
    pub effects: HashMap<u8, Vec<u8>>,
    pub side_custom: bool,
    pub side_frame: Vec<u8>,
    /// 0x00B9 code: 3 = 12-button plate
    pub plate_code: u8,
}

fn rec(bytes: &[u8]) -> [u8; 7] {
    let mut r = [0u8; 7];
    r[..bytes.len()].copy_from_slice(bytes);
    r
}

impl MockNagaPro {
    pub fn new() -> Self {
        let mut buttons = HashMap::new();
        let defaults: &[(u8, &[u8])] = &[
            (0x01, &[1, 1, 1]), (0x02, &[1, 1, 2]), (0x03, &[1, 1, 3]), (0x04, &[1, 1, 4]),
            (0x05, &[1, 1, 5]), (0x09, &[1, 1, 9]), (0x0A, &[1, 1, 0x0A]), (0x0B, &[6, 1, 1]),
            (0x0C, &[6, 1, 2]), (0x0E, &[7, 1, 4]), (0x34, &[1, 1, 0x09]), (0x35, &[1, 1, 0x0A]),
        ];
        for prof in 0..=5u8 {
            for hs in 0..=1u8 {
                for (id, r) in defaults {
                    buttons.insert((prof, *id, hs), rec(r));
                }
                for (i, k) in SIDE_KEYS.iter().enumerate() {
                    buttons.insert((prof, 0x40 + i as u8, hs), rec(&[2, 2, 0, *k]));
                }
                for (i, k) in SIDE_KEYS[..6].iter().enumerate() {
                    buttons.insert((prof, 0x50 + i as u8, hs), rec(&[2, 2, 0, *k]));
                }
            }
        }
        let mut stages = HashMap::new();
        stages.insert(1, (2, vec![(400, 400), (800, 800), (1600, 1600), (3200, 3200), (6400, 6400)]));
        MockNagaPro {
            reply: Report::from_bytes(&[]),
            dpi: (1600, 1600),
            stages,
            poll: 0x01,
            battery: 200,
            idle: 300,
            low_batt: 0x26,
            macros: BTreeMap::new(),
            buttons,
            brightness: [(0x00, 255), (0x01, 255), (0x04, 255)].into_iter().collect(),
            effects: HashMap::new(),
            side_custom: false,
            side_frame: Vec::new(),
            plate_code: 3,
        }
    }

    /// Returns false for "command failed".
    fn handle(&mut self, q: &Report, out: &mut [u8; 80]) -> Option<bool> {
        let a = &q.arguments;
        let be16 = |i: usize| u16::from_be_bytes([a[i], a[i + 1]]);
        let be32 = |i: usize| u32::from_be_bytes([a[i], a[i + 1], a[i + 2], a[i + 3]]);
        match q.cmd() {
            cmd::GET_FIRMWARE => out[..2].copy_from_slice(&[1, 5]),
            cmd::GET_SERIAL => out[..15].copy_from_slice(b"PM2041H00000000"),
            cmd::GET_SIDE_PLATE => out[0] = self.plate_code,
            cmd::GET_DPI => {
                out[1..3].copy_from_slice(&self.dpi.0.to_be_bytes());
                out[3..5].copy_from_slice(&self.dpi.1.to_be_bytes());
            }
            cmd::SET_DPI => self.dpi = (be16(1), be16(3)),
            cmd::GET_DPI_STAGES => {
                let (active, st) = self.stages.get(&a[0]).or_else(|| self.stages.get(&1)).cloned()?;
                out[1] = active;
                out[2] = st.len() as u8;
                for (i, (x, y)) in st.iter().enumerate() {
                    let o = 3 + i * 7;
                    out[o] = i as u8;
                    out[o + 1..o + 3].copy_from_slice(&x.to_be_bytes());
                    out[o + 3..o + 5].copy_from_slice(&y.to_be_bytes());
                }
            }
            cmd::SET_DPI_STAGES => {
                let n = a[2] as usize;
                let st: Vec<(u16, u16)> = (0..n).map(|i| (be16(3 + i * 7 + 1), be16(3 + i * 7 + 3))).collect();
                let active = a[1];
                if let Some(s) = st.get(active.saturating_sub(1) as usize) {
                    self.dpi = *s;
                }
                self.stages.insert(a[0], (active, st));
            }
            cmd::GET_POLLING => out[0] = self.poll,
            cmd::SET_POLLING => self.poll = a[0],
            cmd::GET_BATTERY => out[1] = self.battery,
            cmd::GET_CHARGING => out[1] = 0,
            cmd::GET_IDLE_TIME => out[..2].copy_from_slice(&self.idle.to_be_bytes()),
            cmd::SET_IDLE_TIME => self.idle = be16(0),
            cmd::GET_LOW_BATTERY => out[0] = self.low_batt,
            cmd::SET_LOW_BATTERY => self.low_batt = a[0],
            cmd::GET_PROFILE_COUNT => out[0] = 5,
            cmd::GET_PROFILE_LIST => out[..6].copy_from_slice(&[5, 1, 2, 3, 4, 5]),
            cmd::GET_BUTTON => {
                let r = self.buttons.get(&(a[0], a[1], a[2]))?;
                out[3..10].copy_from_slice(r);
            }
            cmd::SET_BUTTON => {
                let k = (a[0], a[1], a[2]);
                if !self.buttons.contains_key(&k) {
                    return Some(false);
                }
                let mut r = [0u8; 7];
                r.copy_from_slice(&a[3..10]);
                self.buttons.insert(k, r);
            }
            cmd::GET_MACRO_COUNT => out[..2].copy_from_slice(&(self.macros.len() as u16).to_be_bytes()),
            cmd::GET_MACRO_LIST => {
                let start = be16(0) as usize;
                let ids: Vec<u16> = self.macros.keys().copied().collect();
                out[..2].copy_from_slice(&(ids.len() as u16).to_be_bytes());
                for (i, id) in ids.iter().skip(start).take(32).enumerate() {
                    out[2 + i * 2..4 + i * 2].copy_from_slice(&id.to_be_bytes());
                }
            }
            cmd::GET_FLASH_USAGE => {
                let used: usize = self.macros.values().map(Vec::len).sum();
                out[..2].copy_from_slice(&0x64u16.to_be_bytes());
                out[2..6].copy_from_slice(&92160u32.to_be_bytes());
                out[6..10].copy_from_slice(&(92160 - used as u32).to_be_bytes());
            }
            cmd::SET_MACRO_SIZE => {
                self.macros.insert(be16(0), vec![0; be32(2) as usize]);
            }
            cmd::GET_MACRO_SIZE => {
                let m = self.macros.get(&be16(0))?;
                out[2..6].copy_from_slice(&(m.len() as u32).to_be_bytes());
            }
            cmd::SET_MACRO_DATA => {
                let (id, off, n) = (be16(0), be32(2) as usize, a[6] as usize);
                let m = self.macros.get_mut(&id)?;
                if off + n > m.len() {
                    return Some(false);
                }
                m[off..off + n].copy_from_slice(&a[7..7 + n]);
            }
            cmd::GET_MACRO_DATA => {
                let (id, off, n) = (be16(0), be32(2) as usize, a[6] as usize);
                let m = self.macros.get(&id)?;
                if off + n > m.len() {
                    return Some(false);
                }
                out[7..7 + n].copy_from_slice(&m[off..off + n]);
            }
            cmd::DELETE_MACRO => {
                self.macros.remove(&be16(0))?;
            }
            cmd::SET_EFFECT => {
                let led = a[1];
                if a[2] == 0x08 && led == 0 {
                    self.side_custom = true;
                } else if led == 0x01 || led == 0x04 {
                    self.effects.insert(led, a[..q.data_size as usize].to_vec());
                } else {
                    return Some(false);
                }
            }
            cmd::SET_FRAME => {
                if !self.side_custom {
                    return Some(false);
                }
                let n = (a[4] - a[3] + 1) as usize;
                self.side_frame = a[5..5 + 3 * n].to_vec();
            }
            cmd::SET_MOUSE_EFFECT => {
                if a[1] != 0x05 {
                    return Some(false);
                }
                self.side_custom = false;
                self.effects.insert(0x05, a[..q.data_size as usize].to_vec());
            }
            cmd::SET_BRIGHTNESS => {
                if !self.brightness.contains_key(&a[1]) {
                    return Some(false);
                }
                self.brightness.insert(a[1], a[2]);
            }
            cmd::GET_BRIGHTNESS => {
                let v = *self.brightness.get(&a[1])?;
                out[..3].copy_from_slice(&[a[0], a[1], v]);
            }
            _ => return None,
        }
        Some(true)
    }
}

impl Default for MockNagaPro {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for MockNagaPro {
    fn send_feature(&mut self, data: &[u8; REPORT_LEN]) -> Result<()> {
        let q = Report::from_bytes(data);
        let mut rep = q.clone();
        let mut out = [0u8; 80];
        rep.status = match self.handle(&q, &mut out) {
            Some(true) => status::OK,
            Some(false) => status::FAIL,
            None if [cmd::GET_BUTTON, cmd::GET_MACRO_SIZE, cmd::GET_MACRO_DATA, cmd::DELETE_MACRO,
                     cmd::GET_BRIGHTNESS, cmd::SET_MACRO_DATA, cmd::GET_DPI_STAGES]
                .contains(&q.cmd()) => status::FAIL,
            None => status::NOT_SUPPORTED,
        };
        rep.arguments = out;
        self.reply = rep;
        Ok(())
    }

    fn get_feature(&mut self) -> Result<[u8; REPORT_LEN]> {
        Ok(self.reply.to_bytes())
    }
}
