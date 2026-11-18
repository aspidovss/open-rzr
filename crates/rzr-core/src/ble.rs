//! Razer control protocol over Bluetooth LE (found on the Naga Pro, 0x0092; the BlackWidow V3
//! Mini uses the same service). It is not the 90-byte USB report:
//!
//! ```text
//! GATT service 52401523-f97c-7f90-0e7f-6c6f4e36db1c
//!   …1524  write    request header (8 bytes), then — for a "set" — its data in a second write
//!   …1525  notify   reply header (20-byte notification), then the data in 20-byte notifications
//!   …1526  notify   (8 bytes, meaning unknown)
//!
//! request header   [tid, len, 00, 00, class, cmd, sub1, sub2]   len = bytes of data that follow
//! reply header     [tid, len lo, len hi, 00, 00, 00, 00, status] status as on USB (01 busy, 02 OK, 05 not supported…)
//! reply data       len bytes, split over 20-byte notifications (the rest of the last one is stale)
//! ```
//!
//! Numbers are little-endian (USB: big-endian), and class / command numbers are their own:
//! "get" commands have bit 7 of `cmd` set, like on USB. Known on the Naga Pro:
//!
//! `<class> 80` lists the commands of a class: one byte each, low 6 bits = command number,
//! 0x40 = can be read (as 0x80 | n), 0x80 = can be set (as n).
//!
//! | class cmd sub      | reply                                                   |
//! |--------------------|---------------------------------------------------------|
//! | 00 81 00 00        | firmware: major, minor, 00, 00                          |
//! | 01 83 00 00        | serial number (text)                                    |
//! | 03 84 02 00        | profile slot: id (16), name, hash (text)                |
//! | 05 81 00 01        | battery 0..255                                          |
//! | 05 84 00 00        | time until sleep, seconds LE                            |
//! | 08 80 00 00        | button ids                                              |
//! | 08 83 01 00        | every binding: 16 bytes per button (see [`binding`])    |
//! | 08 84 00 <button>  | the binding of one button                               |
//! | 0b 81 00 00        | current DPI: x LE, y LE                                 |
//! | 0b 82 00 00        | the 5 stage slots: x LE, y LE, 00 00 each               |
//! | 0b 83 00 00        | active stage                                            |
//! | 0b 84 01 00        | DPI stages: active, count, count × [id, x LE, y LE, 00 00] |
//! | 10 83 <zone> 00    | lighting: effect (03 = spectrum), …                     |
//! | 10 85 00 01        | brightness 0..255                                       |

/// The 8-byte request header.
pub fn header(tid: u8, len: u8, class: u8, cmd: u8, sub: [u8; 2]) -> [u8; 8] {
    [tid, len, 0, 0, class, cmd, sub[0], sub[1]]
}

/// Bit 7 of the command: a "get" (read-only by Razer's convention).
pub fn is_get(cmd: u8) -> bool {
    cmd & 0x80 != 0
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Reply {
    pub tid: u8,
    pub status: u8,
    /// announced length of the data
    pub len: usize,
    /// the data (shorter than `len` if notifications were missing)
    pub data: Vec<u8>,
}

impl Reply {
    pub fn complete(&self) -> bool {
        self.data.len() >= self.len
    }
    pub fn status_name(&self) -> &'static str {
        crate::protocol::status::label(self.status).unwrap_or("?")
    }
}

/// Is this notification the reply header of request `tid`? (length: bytes 1–2, little-endian)
fn is_header(n: &[u8], tid: u8) -> bool {
    n.len() >= 8 && n[0] == tid && n[3..7].iter().all(|&b| b == 0) && n[7] != 0
}

/// Put the reply of request `tid` together from the notifications of …1525, in order.
/// `None` while its header has not arrived.
pub fn assemble(notes: &[Vec<u8>], tid: u8) -> Option<Reply> {
    let i = notes.iter().position(|n| is_header(n, tid))?;
    let h = &notes[i];
    let mut r = Reply { tid, status: h[7], len: u16::from_le_bytes([h[1], h[2]]) as usize, data: Vec::new() };
    for n in &notes[i + 1..] {
        if r.data.len() >= r.len {
            break;
        }
        let take = (r.len - r.data.len()).min(n.len());
        r.data.extend_from_slice(&n[..take]);
    }
    Some(r)
}

/// One button record of `08 83` / `08 84` (16 bytes): button id, 01, then the normal and the
/// Hypershift action interleaved byte by byte — type, type, length, length, data, data, …
/// Types as over USB: 00 off, 01 mouse button, 02 key (modifiers, key), 06 DPI, 07 profile,
/// 0e mouse turbo (button, interval BE).
pub fn binding(rec: &[u8]) -> Option<(u8, [Vec<u8>; 2])> {
    if rec.len() < 6 {
        return None;
    }
    let mut out = [Vec::new(), Vec::new()];
    for (k, o) in out.iter_mut().enumerate() {
        let len = rec[4 + k] as usize;
        o.push(rec[2 + k]);
        o.push(rec[4 + k]);
        for i in 0..len {
            o.push(*rec.get(6 + 2 * i + k)?);
        }
    }
    Some((rec[0], out))
}

/// "key 1e", "mouse 04", … for a record `[type, len, data…]`.
pub fn action_text(a: &[u8]) -> String {
    let d = a.get(2..).unwrap_or(&[]);
    match (a.first().copied().unwrap_or(0), d) {
        (0x00, _) => "off".into(),
        (0x01, [b, ..]) => format!("mouse {b:02x}"),
        (0x02, [m, k, ..]) if *m == 0 => format!("key {k:02x}"),
        (0x02, [m, k, ..]) => format!("key {m:02x}+{k:02x}"),
        (0x06, [b, ..]) => format!("DPI {}", match b { 1 => "up", 2 => "down", _ => "?" }),
        (0x07, [b, ..]) => format!("profile {b:02x}"),
        (0x0e, [b, hi, lo, ..]) => format!("turbo mouse {b:02x} {} ms", u16::from_be_bytes([*hi, *lo])),
        (t, d) => format!("type {t:02x} [{}]", d.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")),
    }
}

fn text_of(d: &[u8]) -> String {
    d.split(|b| !(0x20..0x7f).contains(b)).filter(|w| w.len() >= 3).map(|w| String::from_utf8_lossy(w).into_owned()).collect::<Vec<_>>().join(" · ")
}

/// What a reply means, for the known commands ("" when unknown).
pub fn describe(class: u8, cmd: u8, sub: [u8; 2], r: &Reply) -> String {
    if r.status != crate::protocol::status::OK {
        return String::new();
    }
    let d = &r.data;
    let le = |i: usize| d.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    match (class, cmd) {
        (_, 0x80) if !d.is_empty() => {
            let list = |bit: u8, set: bool| d.iter().filter(|b| *b & bit != 0).map(|b| format!("{:02x}", (b & 0x3f) | if set { 0 } else { 0x80 })).collect::<Vec<_>>().join(" ");
            if class == 0x08 {
                format!("buttons: {}", d.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "))
            } else {
                format!("commands — read: {}; set: {}", list(0x40, false), list(0x80, true))
            }
        }
        (0x00, 0x81) if d.len() >= 2 => format!("firmware v{}.{:02}", d[0], d[1]),
        (0x01, 0x83) => format!("serial {}", text_of(d)),
        (0x03, 0x84) => format!("profile: {}", text_of(d)),
        (0x05, 0x81) if !d.is_empty() => format!("battery {} % ({})", (d[0] as u32 * 100 + 127) / 255, d[0]),
        (0x05, 0x84) => le(0).map(|s| format!("sleep after {s} s")).unwrap_or_default(),
        (0x08, 0x83 | 0x84) => d
            .chunks(16)
            .filter_map(binding)
            .map(|(b, [n, h])| format!("{b:02x}: {} / {}", action_text(&n), action_text(&h)))
            .collect::<Vec<_>>()
            .join("; "),
        (0x0b, 0x81) => match (le(0), le(2)) {
            (Some(x), Some(y)) => format!("DPI {x}×{y}"),
            _ => String::new(),
        },
        (0x0b, 0x82) => d.chunks(6).filter_map(|c| Some(format!("{}×{}", le_of(c, 0)?, le_of(c, 2)?))).collect::<Vec<_>>().join(", "),
        (0x0b, 0x83) if !d.is_empty() => format!("active stage {}", d[0]),
        (0x0b, 0x84) if d.len() >= 2 => {
            let mut stages = Vec::new();
            let mut p = 2;
            for _ in 0..d[1] {
                match (d.get(p), le(p + 1), le(p + 3)) {
                    (Some(id), Some(x), Some(y)) => stages.push(format!("{id}: {x}×{y}")),
                    _ => break,
                }
                p += 7;
            }
            format!("DPI stages, active {}: {}", d[0], stages.join(", "))
        }
        (0x10, 0x83) if !d.is_empty() => {
            let effect = match d[0] {
                0 => "off",
                1 => "static",
                2 => "breathing",
                3 => "spectrum",
                4 => "wave",
                5 => "reactive",
                _ => "?",
            };
            format!("lighting effect {} ({effect})", d[0])
        }
        (0x10, 0x85) if !d.is_empty() && sub[1] != 0 => format!("brightness {} % ({})", (d[0] as u32 * 100 + 127) / 255, d[0]),
        _ => String::new(),
    }
}

fn le_of(c: &[u8], i: usize) -> Option<u16> {
    c.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn n(s: &str) -> Vec<u8> {
        crate::wireless::parse_hex(s).unwrap()
    }

    /// Replies recorded from a Naga Pro over Bluetooth (2026-11-11).
    #[test]
    fn naga_pro_replies() {
        assert_eq!(header(3, 0, 0x0b, 0x84, [1, 0]), [3, 0, 0, 0, 0x0b, 0x84, 1, 0]);
        let stale = "65 e3 91 6b 8a 6a ff 7e 75 05 ea 2c";
        // not supported
        let r = assemble(&[n(&format!("01 00 00 00 00 00 00 05 {stale}"))], 1).unwrap();
        assert_eq!((r.status, r.len, r.complete()), (5, 0, true));
        // battery: header, then 1 byte of data (an older reply of tid 02 arrives first)
        let notes = [n(&format!("02 01 00 00 00 00 00 02 {stale}")), n(&format!("fc 01 00 00 00 00 00 02 {stale}"))];
        let r = assemble(&notes, 2).unwrap();
        assert_eq!(r.data, vec![0xfc]);
        assert_eq!(describe(0x05, 0x81, [0, 1], &r), "battery 99 % (252)");
        // DPI stages: 22 bytes in two notifications
        let notes = [
            n(&format!("03 16 00 00 00 00 00 02 {stale}")),
            n("02 03 01 20 03 20 03 00 00 02 80 0c 80 0c 00 00 03 00 19 00"),
            n("19 00 01 20 03 20 03 00 00 02 80 0c 80 0c 00 00 03 00 19 00"),
        ];
        let r = assemble(&notes, 3).unwrap();
        assert_eq!(r.data.len(), 22);
        assert_eq!(describe(0x0b, 0x84, [1, 0], &r), "DPI stages, active 2: 1: 800×800, 2: 3200×3200, 3: 6400×6400");
        // lighting, firmware
        let r = assemble(&[n("04 0a 00 00 00 00 00 02 00 02 80 0c 80 0c 00 00 03 00 19 00"), n("03 00 00 00 00 00 00 00 00 00 80 0c")], 4).unwrap();
        assert_eq!(describe(0x10, 0x83, [0, 0], &r), "lighting effect 3 (spectrum)");
        let r = assemble(&[n("05 04 00 00 00 00 00 02 00 00 80 0c"), n("02 01 00 00 00 00 00 02")], 5).unwrap();
        assert_eq!(describe(0x00, 0x81, [0, 0], &r), "firmware v2.01");
        // header still missing
        assert!(assemble(&[n("02 03 01 20 03 20 03 00")], 7).is_none());
        assert!(is_get(0x84) && !is_get(0x03));
        // a reply longer than 255 bytes: length in bytes 1–2 (0x01e0 = 480, every binding)
        let h = n("46 e0 01 00 00 00 00 02 00 00 00 00 00 00 00 00 00 00 00 00");
        let r = assemble(&[h, n("01 01 01 01 01 01 01 01 00 00 00 00 00 00 00 00 02 01 01 01")], 0x46).unwrap();
        assert_eq!((r.len, r.complete()), (480, false));
        // bindings: normal / Hypershift interleaved
        let rec = n("40 01 02 02 02 02 00 00 35 1e 00 00 00 00 00 00");
        let r = Reply { tid: 1, status: 2, len: 16, data: rec.clone() };
        assert_eq!(describe(0x08, 0x84, [0, 0x40], &r), "40: key 35 / key 1e");
        let r = Reply { tid: 1, status: 2, len: 16, data: n("34 01 0e 0e 03 03 09 09 00 00 64 14 00 00 00 00") };
        assert_eq!(describe(0x08, 0x84, [0, 0x34], &r), "34: turbo mouse 09 100 ms / turbo mouse 09 20 ms");
        let r = Reply { tid: 1, status: 2, len: 16, data: n("41 01 01 02 01 02 04 00 00 1f 00 00 00 00 00 00") };
        assert_eq!(describe(0x08, 0x84, [0, 0x41], &r), "41: mouse 04 / key 1f");
        // capability list, idle, serial
        let r = Reply { tid: 1, status: 2, len: 6, data: n("40 41 42 c3 84 c5") };
        assert_eq!(describe(0x10, 0x80, [0, 0], &r), "commands — read: 80 81 82 83 85; set: 03 04 05");
        let r = Reply { tid: 1, status: 2, len: 2, data: n("2c 01") };
        assert_eq!(describe(0x05, 0x84, [0, 0], &r), "sleep after 300 s");
        let r = Reply { tid: 1, status: 2, len: 22, data: n("50 4d 32 31 30 37 48 31 36 33 30 30 34 36 30 00 00 00 00 00 00 00") };
        assert_eq!(describe(0x01, 0x83, [0, 0], &r), "serial PM2107H16300460");
    }
}
