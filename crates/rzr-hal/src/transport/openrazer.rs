//! OpenRazer compatibility bridge.
//!
//! When the OpenRazer kernel driver (`razermouse`) is bound to the mouse, requests it can serve are
//! sent through its sysfs files (`/sys/bus/hid/drivers/razermouse/0003:1532:PID.N/…`) instead of raw HID.
//! The driver then serialises them with the OpenRazer daemon, and picks the right packet for every
//! model. Requests the driver has no file for (buttons, macros, profiles, side plate) go to the
//! inner HID transport unchanged.
//!
//! The bridge works at the report level: open-rzr builds its usual 90-byte request, the bridge
//! translates it to a file read/write and answers with a normal reply. The rest of open-rzr does not
//! know the difference. File formats follow the OpenRazer driver (driver/razermouse_driver.c).

use std::path::{Path, PathBuf};

use super::Transport;
use crate::protocol::{cmd, polling_code, polling_hz, status, Report, Result, REPORT_LEN};

pub struct OpenRazerBridge {
    dir: PathBuf,
    inner: Option<Box<dyn Transport>>,
    pending: Option<[u8; REPORT_LEN]>,
}

enum Fail {
    /// the driver has no such file for this model
    NotSupported,
    /// the driver rejected the value
    Failed,
}

type Answer = std::result::Result<Vec<u8>, Fail>;

const VARSTORE: u8 = 0x01;

impl OpenRazerBridge {
    /// `dir` — sysfs directory of the mouse; `inner` — HID transport for everything else.
    pub fn new(dir: PathBuf, inner: Option<Box<dyn Transport>>) -> OpenRazerBridge {
        OpenRazerBridge { dir, inner, pending: None }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn has(&self, name: &str) -> bool {
        self.dir.join(name).exists()
    }

    fn read(&self, name: &str) -> std::result::Result<Vec<u8>, Fail> {
        let p = self.dir.join(name);
        match std::fs::read(&p) {
            Ok(v) => {
                log_debug!("usb", "openrazer: read {name} -> {}", show(&v));
                Ok(v)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Fail::NotSupported),
            Err(e) => {
                log_warn!("usb", "openrazer: read {name}: {e}");
                Err(Fail::Failed)
            }
        }
    }

    fn text(&self, name: &str) -> std::result::Result<String, Fail> {
        Ok(String::from_utf8_lossy(&self.read(name)?).trim().to_string())
    }

    fn num(&self, name: &str) -> std::result::Result<u32, Fail> {
        self.text(name)?.parse().map_err(|_| Fail::Failed)
    }

    fn write(&self, name: &str, data: &[u8]) -> Answer {
        let p = self.dir.join(name);
        if !p.exists() {
            return Err(Fail::NotSupported);
        }
        // one write() call: the driver parses exactly what one write delivers
        let r = std::fs::OpenOptions::new().write(true).truncate(true).open(&p).and_then(|mut f| {
            use std::io::Write;
            f.write(data).map(|_| ())
        });
        match r {
            Ok(()) => {
                log_debug!("usb", "openrazer: write {name} = {}", show(data));
                Ok(Vec::new())
            }
            Err(e) => {
                log_warn!("usb", "openrazer: write {name} = {}: {e}", show(data));
                Err(Fail::Failed)
            }
        }
    }

    fn write_num(&self, name: &str, v: u32) -> Answer {
        self.write(name, v.to_string().as_bytes())
    }

    /// sysfs prefix of a lighting LED; falls back to the plain "matrix_" files.
    fn prefix(&self, led: u8, probe: &str) -> &'static str {
        let p = match led {
            0x01 => "scroll_",
            0x04 => "logo_",
            0x05 => "backlight_",
            0x10 => "right_",
            0x11 => "left_",
            _ => "",
        };
        if !p.is_empty() && !self.has(&format!("{p}{probe}")) {
            return "";
        }
        p
    }

    fn brightness_file(&self, led: u8) -> String {
        let p = self.prefix(led, "led_brightness");
        if p.is_empty() {
            "matrix_brightness".into()
        } else {
            format!("{p}led_brightness")
        }
    }

    /// None = not handled here (sent to the HID transport).
    fn translate(&self, r: &Report) -> Option<Answer> {
        let a = &r.arguments;
        let be16 = |i: usize| u16::from_be_bytes([a[i], a[i + 1]]);
        Some(match r.cmd() {
            cmd::GET_FIRMWARE => self.text("firmware_version").and_then(|t| {
                let mut it = t.trim_start_matches('v').split('.').map(|x| x.parse::<u8>().unwrap_or(0));
                Ok(vec![it.next().unwrap_or(0), it.next().unwrap_or(0)])
            }),
            cmd::GET_SERIAL => self.text("device_serial").map(|s| s.bytes().take(22).collect()),

            cmd::GET_DPI => self.text("dpi").and_then(|t| {
                let mut it = t.split(':').map(|x| x.trim().parse::<u16>().unwrap_or(0));
                let x = it.next().unwrap_or(0);
                let y = it.next().unwrap_or(x);
                let mut v = vec![a[0]];
                v.extend(x.to_be_bytes());
                v.extend(y.to_be_bytes());
                Ok(v)
            }),
            cmd::SET_DPI => self.write("dpi", &a[1..5]),

            // the driver keeps stages in the active storage only; other profiles go over HID
            cmd::GET_DPI_STAGES if a[0] == VARSTORE => self.read("dpi_stages").map(|b| {
                let active = b.first().copied().unwrap_or(1);
                let stages: Vec<&[u8]> = b.get(1..).unwrap_or(&[]).chunks_exact(4).collect();
                let mut v = vec![VARSTORE, active, stages.len() as u8];
                for (i, s) in stages.iter().enumerate() {
                    v.push(i as u8 + 1);
                    v.extend_from_slice(s);
                    v.extend([0, 0]);
                }
                v
            }),
            cmd::SET_DPI_STAGES if a[0] == VARSTORE => {
                let n = (a[2] as usize).min(5);
                let mut b = vec![a[1]];
                for i in 0..n {
                    let o = 3 + i * 7;
                    b.extend_from_slice(&a[o + 1..o + 5]);
                }
                self.write("dpi_stages", &b)
            }

            cmd::GET_POLLING => self.num("poll_rate").map(|hz| vec![polling_code(hz).unwrap_or(0x01)]),
            cmd::SET_POLLING => self.write_num("poll_rate", polling_hz(a[0])),

            cmd::GET_BATTERY => self.num("charge_level").map(|v| vec![0, v.min(255) as u8]),
            cmd::GET_CHARGING => self.num("charge_status").map(|v| vec![0, v.min(255) as u8]),
            cmd::GET_IDLE_TIME => self.num("device_idle_time").map(|v| (v.min(0xFFFF) as u16).to_be_bytes().to_vec()),
            cmd::SET_IDLE_TIME => self.write_num("device_idle_time", be16(0) as u32),
            cmd::GET_LOW_BATTERY => self.num("charge_low_threshold").map(|v| vec![v.min(255) as u8]),
            cmd::SET_LOW_BATTERY => self.write_num("charge_low_threshold", a[0] as u32),

            cmd::GET_BRIGHTNESS => self.num(&self.brightness_file(a[1])).map(|v| vec![a[0], a[1], v.min(255) as u8]),
            cmd::SET_BRIGHTNESS => self.write_num(&self.brightness_file(a[1]), a[2] as u32),

            cmd::SET_EFFECT => self.effect(a),
            cmd::SET_FRAME => {
                // [00 00 row start stop RGB…] -> row start stop RGB…
                let (start, stop) = (a[3] as usize, a[4] as usize);
                let len = 3 + (stop.saturating_sub(start) + 1) * 3;
                self.write("matrix_custom_frame", &a[2..(2 + len).min(80)])
            }
            cmd::SET_MOUSE_EFFECT => self.mouse_effect(a),
            _ => return None,
        })
    }

    /// Extended matrix effect 0x0F02 [store, led, effect, …]
    fn effect(&self, a: &[u8; 80]) -> Answer {
        let led = a[1];
        if led == 0 && a[2] == 0x08 {
            return self.write("matrix_effect_custom", b"1");
        }
        let p = self.prefix(led, "matrix_effect_static");
        let f = |e: &str| format!("{p}matrix_effect_{e}");
        match a[2] {
            0x00 => self.write(&f("none"), b"1"),
            0x01 => self.write(&f("static"), &a[6..9]),
            0x02 => match a[3] {
                0x01 => self.write(&f("breath"), &a[6..9]),
                0x02 => self.write(&f("breath"), &a[6..12]),
                _ => self.write(&f("breath"), b"1"),
            },
            0x03 => self.write(&f("spectrum"), b"1"),
            0x04 => self.write(&f("wave"), a[3].max(1).to_string().as_bytes()),
            0x05 => {
                let mut b = vec![a[4]];
                b.extend_from_slice(&a[6..9]);
                self.write(&f("reactive"), &b)
            }
            _ => Err(Fail::NotSupported),
        }
    }

    /// Mouse-extended effect 0x030D [store, led, effect, …] (Naga side panel)
    fn mouse_effect(&self, a: &[u8; 80]) -> Answer {
        let p = self.prefix(a[1], "matrix_effect_spectrum");
        let f = |e: &str| format!("{p}matrix_effect_{e}");
        match a[2] {
            0x00 => self.write(&f("none"), b"1"),
            0x04 => self.write(&f("spectrum"), b"1"),
            0x03 => match a[3] {
                0x01 => self.write(&f("breath"), &a[4..7]),
                0x02 => self.write(&f("breath"), &a[4..10]),
                _ => self.write(&f("breath"), b"1"),
            },
            0x02 => {
                let mut b = vec![a[3]];
                b.extend_from_slice(&a[4..7]);
                self.write(&f("reactive"), &b)
            }
            _ => Err(Fail::NotSupported),
        }
    }
}

fn show(v: &[u8]) -> String {
    if !v.is_empty() && v.iter().all(|b| b.is_ascii_graphic() || *b == b' ' || *b == b'\n') {
        format!("\"{}\"", String::from_utf8_lossy(v).trim())
    } else {
        v.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
    }
}

impl Transport for OpenRazerBridge {
    fn send_feature(&mut self, data: &[u8; REPORT_LEN]) -> Result<()> {
        let req = Report::from_bytes(data);
        match self.translate(&req) {
            Some(ans) => {
                let mut rep = req.clone();
                match ans {
                    Ok(args) => {
                        rep.status = status::OK;
                        let n = args.len().min(80);
                        rep.arguments[..n].copy_from_slice(&args[..n]);
                    }
                    Err(Fail::NotSupported) => rep.status = status::NOT_SUPPORTED,
                    Err(Fail::Failed) => rep.status = status::FAIL,
                }
                self.pending = Some(rep.to_bytes());
                Ok(())
            }
            None => {
                self.pending = None;
                match self.inner.as_mut() {
                    Some(t) => t.send_feature(data),
                    None => {
                        let mut rep = req;
                        rep.status = status::NOT_SUPPORTED;
                        self.pending = Some(rep.to_bytes());
                        Ok(())
                    }
                }
            }
        }
    }

    fn get_feature(&mut self) -> Result<[u8; REPORT_LEN]> {
        if let Some(p) = self.pending.take() {
            return Ok(p);
        }
        match self.inner.as_mut() {
            Some(t) => t.get_feature(),
            None => Err(crate::protocol::Error::Io("no HID transport behind the OpenRazer bridge".into())),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::transport::Channel;
    use std::fs;

    fn sysfs() -> PathBuf {
        let d = std::env::temp_dir().join(format!("openrzr-or-test-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        for (f, v) in [
            ("firmware_version", "v2.1\n"),
            ("device_serial", "PM2049H12345678\n"),
            ("dpi", "1600:1600\n"),
            ("poll_rate", "500\n"),
            ("charge_level", "204\n"),
            ("charge_status", "1\n"),
            ("device_idle_time", "300\n"),
            ("charge_low_threshold", "38\n"),
            ("logo_led_brightness", "255\n"),
            ("logo_matrix_effect_static", ""),
            ("logo_matrix_effect_breath", ""),
            ("matrix_effect_reactive", ""),
            ("matrix_brightness", "128\n"),
        ] {
            fs::write(d.join(f), v).unwrap();
        }
        fs::write(d.join("dpi_stages"), [2u8, 0x03, 0x20, 0x03, 0x20, 0x06, 0x40, 0x06, 0x40]).unwrap();
        d
    }

    #[test]
    fn translates_requests_to_sysfs() {
        let d = sysfs();
        let mut ch = Channel::new(Box::new(OpenRazerBridge::new(d.clone(), None)), 0x1F, 0);
        let fw = ch.request(cmd::GET_FIRMWARE, &[], Some(2)).unwrap().arguments;
        assert_eq!(&fw[..2], &[2, 1]);
        let dpi = ch.request(cmd::GET_DPI, &[0], Some(7)).unwrap().arguments;
        assert_eq!(&dpi[..5], &[0, 0x06, 0x40, 0x06, 0x40]);
        ch.request(cmd::SET_DPI, &[1, 0x0C, 0x80, 0x0C, 0x80, 0, 0], Some(7)).unwrap();
        assert_eq!(fs::read(d.join("dpi")).unwrap(), vec![0x0C, 0x80, 0x0C, 0x80]);
        assert_eq!(ch.request(cmd::GET_POLLING, &[], Some(1)).unwrap().arguments[0], 0x02);
        ch.request(cmd::SET_POLLING, &[0x01], Some(1)).unwrap();
        assert_eq!(fs::read_to_string(d.join("poll_rate")).unwrap(), "1000");
        assert_eq!(ch.request(cmd::GET_BATTERY, &[], Some(2)).unwrap().arguments[1], 204);

        let st = ch.request(cmd::GET_DPI_STAGES, &[1], Some(0x26)).unwrap().arguments;
        assert_eq!(&st[..3], &[1, 2, 2]);
        assert_eq!(&st[3..10], &[1, 0x03, 0x20, 0x03, 0x20, 0, 0]);
        ch.request(cmd::SET_DPI_STAGES, &[1, 1, 1, 0, 0x01, 0x90, 0x01, 0x90, 0, 0], Some(0x26)).unwrap();
        assert_eq!(fs::read(d.join("dpi_stages")).unwrap(), vec![1, 0x01, 0x90, 0x01, 0x90]);

        // logo static red, side panel reactive, brightness of the side panel
        ch.request(cmd::SET_EFFECT, &[1, 0x04, 0x01, 0, 0, 0x01, 255, 0, 0], Some(9)).unwrap();
        assert_eq!(fs::read(d.join("logo_matrix_effect_static")).unwrap(), vec![255, 0, 0]);
        ch.request(cmd::SET_MOUSE_EFFECT, &[1, 0x05, 0x02, 3, 0, 255, 0], Some(7)).unwrap();
        assert_eq!(fs::read(d.join("matrix_effect_reactive")).unwrap(), vec![3, 0, 255, 0]);
        ch.request(cmd::SET_BRIGHTNESS, &[1, 0x00, 64], Some(3)).unwrap();
        assert_eq!(fs::read_to_string(d.join("matrix_brightness")).unwrap(), "64");

        // no file for it and no HID behind: "not supported", not a crash
        let e = ch.request(cmd::SET_EFFECT, &[1, 0x01, 0x03], Some(6)).unwrap_err();
        assert_eq!(e.code(), "failed");
        let e = ch.request(cmd::GET_BUTTON, &[1, 0x40, 0], Some(10)).unwrap_err();
        assert!(e.to_string().contains("NOT_SUPPORTED"));
        let _ = fs::remove_dir_all(&d);
    }
}
