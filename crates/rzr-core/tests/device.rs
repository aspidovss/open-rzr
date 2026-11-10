//! Device-level tests against the built-in Naga Pro emulator.

use openrzr::device::RazerMouse;
use openrzr::lighting::ZoneState;
use openrzr::macros::{self, MacroStep};
use openrzr::{Action, DeviceInfo};

fn mouse() -> RazerMouse {
    RazerMouse::open(&DeviceInfo::mock()).unwrap()
}

#[test]
fn settings() {
    let mut m = mouse();
    assert_eq!(m.firmware().unwrap(), "v1.5");
    assert_eq!(m.serial().unwrap(), "PM2041H00000000");
    m.set_dpi(2400, 2400).unwrap();
    assert_eq!(m.get_dpi().unwrap(), (2400, 2400));
    m.set_dpi(60000, 60000).unwrap();
    assert_eq!(m.get_dpi().unwrap(), (20000, 20000));
    m.set_dpi_stages(&[(800, 800), (1600, 1600)], 2, 1).unwrap();
    let st = m.get_dpi_stages(1).unwrap();
    assert_eq!((st.active, st.stages), (2, vec![(800, 800), (1600, 1600)]));
    for hz in [125, 500, 1000] {
        m.set_polling_rate(hz).unwrap();
        assert_eq!(m.get_polling_rate().unwrap(), hz);
    }
    assert!(m.set_polling_rate(300).is_err());
    m.set_idle_time(30).unwrap();
    assert_eq!(m.get_idle_time().unwrap(), 60);
    assert!(m.get_battery().unwrap() <= 100);
    m.set_low_battery_threshold(15).unwrap();
    assert_eq!(m.get_low_battery_threshold().unwrap(), 15);
    assert_eq!(m.list_profiles(), vec![1, 2, 3, 4, 5]);
}

#[test]
fn buttons_scan_and_bind() {
    let mut m = mouse();
    let btns = m.scan_buttons(1, false, false).unwrap();
    let names: Vec<&str> = btns.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"12-plate: 1"));
    assert!(names.contains(&"12-plate: 12"));
    assert!(names.contains(&"6-plate: 6"));
    assert!(names.contains(&"Profile button (bottom)"));
    m.set_button(0x40, &Action::keyboard(1, 0x06), 1, false).unwrap();
    assert_eq!(m.get_button(0x40, 1, false).unwrap().describe(), "Key: LCtrl+C");
    assert_eq!(m.get_button(0x40, 2, false).unwrap().describe(), "Key: 1");
    assert!(m.set_button(0x7E, &Action::kind("disabled"), 1, false).is_err());
}

#[test]
fn copy_profile() {
    let mut m = mouse();
    m.set_button(0x41, &Action::mouse(4), 1, false).unwrap();
    m.set_button(0x42, &Action::keyboard(0, 0x3E), 1, true).unwrap();
    m.set_dpi_stages(&[(500, 500), (900, 900)], 1, 1).unwrap();
    let n = m.copy_profile(1, 3).unwrap();
    assert!(n > 40);
    assert_eq!(m.get_button(0x41, 3, false).unwrap().describe(), "Mouse: Back");
    assert_eq!(m.get_button(0x42, 3, true).unwrap().describe(), "Key: F5");
    assert_eq!(m.get_dpi_stages(3).unwrap().stages, vec![(500, 500), (900, 900)]);
}

#[test]
fn macros_roundtrip() {
    let mut m = mouse();
    let txt = vec!["ctrl+a", "40ms"].repeat(20).join(",");
    let steps = macros::from_text(&txt).unwrap();
    let n = m.write_macro(5, &steps).unwrap();
    assert!(n > 64);
    assert_eq!(m.macro_ids().unwrap(), vec![5]);
    assert_eq!(macros::encode(&m.read_macro(5).unwrap()).unwrap(), macros::encode(&steps).unwrap());
    m.write_macro(5, &[MacroStep::new("key_down", 4), MacroStep::new("key_up", 4)]).unwrap();
    assert_eq!(m.macro_ids().unwrap().len(), 1);
    assert_eq!(m.free_macro_id().unwrap(), 1);
    m.delete_macro(5).unwrap();
    assert!(m.macro_ids().unwrap().is_empty());
}

#[test]
fn backup_restore() {
    let mut m = mouse();
    m.write_macro(2, &macros::from_text("ctrl+z").unwrap()).unwrap();
    let snap = m.backup(&[1, 2]).unwrap();
    m.set_button(0x40, &Action::kind("disabled"), 1, false).unwrap();
    m.set_button(0x40, &Action::kind("disabled"), 2, true).unwrap();
    m.delete_macro(2).unwrap();
    m.set_dpi_stages(&[(100, 100)], 1, 1).unwrap();
    m.restore(&snap).unwrap();
    assert_eq!(m.get_button(0x40, 1, false).unwrap().describe(), "Key: 1");
    assert_eq!(m.get_button(0x40, 2, true).unwrap().describe(), "Key: 1");
    assert_eq!(m.macro_ids().unwrap(), vec![2]);
    assert_eq!(m.get_dpi_stages(1).unwrap().stages.len(), 5);
}

#[test]
fn lighting_all_zones() {
    let mut m = mouse();
    let zones: Vec<&str> = m.zones.iter().map(|z| z.id).collect();
    assert_eq!(zones, vec!["scroll", "logo", "side"]);
    for zone in zones {
        for eff in m.zone_effects(zone) {
            let st = ZoneState { effect: eff.to_string(), leds: vec![[9, 9, 9]; 3], ..Default::default() };
            m.set_lighting(zone, &st).unwrap();
        }
        m.set_brightness(zone, 40).unwrap();
        assert_eq!(m.get_brightness(zone).unwrap(), 40);
    }
    assert!(m.set_lighting("side", &ZoneState { effect: "wave".into(), ..Default::default() }).is_err());
}

#[test]
fn log_records_operations() {
    openrzr::logs::init(openrzr::logs::Level::Debug, false, false);
    let mut m = mouse();
    let (seq, _) = openrzr::logs::recent(0);
    m.set_dpi(1234, 1234).unwrap();
    let (_, lines) = openrzr::logs::recent(seq);
    assert!(lines.iter().any(|l| l.contains("usb") && l.contains(">> ") && l.contains("cmd=0405")));
    assert!(lines.iter().any(|l| l.contains("device") && l.contains("set_dpi 1234x1234 — ok")));
}

#[test]
fn side_plate_detection_and_filter() {
    use openrzr::devices::{button_on_plate, plate_of};
    let mut m = mouse();
    assert_eq!(m.side_plate().unwrap(), Some(12));
    let all = m.scan_buttons(1, false, false).unwrap();
    let on12: Vec<u8> = all.iter().map(|b| b.id).filter(|id| button_on_plate(*id, 12)).collect();
    assert!(on12.contains(&0x40) && on12.contains(&0x4B) && on12.contains(&0x0E));
    assert!(!on12.contains(&0x55) && !on12.contains(&0x04));
    assert_eq!(plate_of(0x05), Some(2));
    assert_eq!(plate_of(0x0E), None);
}

#[test]
fn factory_defaults_and_codes() {
    use openrzr::devices::default_action;
    assert_eq!(default_action(0x40).unwrap().describe(), "Key: 1");
    assert_eq!(default_action(0x4B).unwrap().describe(), "Key: =");
    assert_eq!(default_action(0x05).unwrap().describe(), "Mouse: Forward");
    assert_eq!(default_action(0x0E).unwrap().describe(), "Profile: cycle up");
    assert!(default_action(0x7E).is_none());
    let mut m = mouse();
    m.set_low_battery_threshold(80).unwrap();
    assert_eq!(m.get_low_battery_threshold().unwrap(), 80);
    let e = m.set_button(0x7E, &Action::kind("disabled"), 1, false).unwrap_err();
    assert_eq!(e.code(), "failed");
}

#[test]
fn openrazer_bridge_with_hid_behind() {
    use openrzr::transport::{mock::MockNagaPro, openrazer::OpenRazerBridge};
    let dir = std::env::temp_dir().join(format!("openrzr-it-bridge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (f, v) in [("firmware_version", "v2.1\n"), ("dpi", "800:800\n"), ("charge_level", "255\n"), ("logo_matrix_effect_static", "")] {
        std::fs::write(dir.join(f), v).unwrap();
    }
    let info = openrzr::DeviceInfo { pid: 0x008F, ..openrzr::DeviceInfo::mock() };
    let bridge = OpenRazerBridge::new(dir.clone(), Some(Box::new(MockNagaPro::new())));
    let mut m = openrzr::RazerMouse::with_transport(&info, Box::new(bridge), Some(dir.clone()));
    assert_eq!(m.firmware().unwrap(), "v2.1");
    assert_eq!(m.get_battery().unwrap(), 100);
    m.set_dpi(3200, 3200).unwrap();
    assert_eq!(std::fs::read(dir.join("dpi")).unwrap(), vec![0x0C, 0x80, 0x0C, 0x80]);
    m.set_lighting("logo", &ZoneState { effect: "static".into(), color1: [1, 2, 3], ..Default::default() }).unwrap();
    assert_eq!(std::fs::read(dir.join("logo_matrix_effect_static")).unwrap(), vec![1, 2, 3]);
    // buttons are not an OpenRazer feature: they go over HID (the emulator here)
    assert_eq!(m.get_button(0x40, 1, false).unwrap().describe(), "Key: 1");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn openrazer_database_device_is_limited_to_its_features() {
    use openrzr::{devices, feat};
    let d = devices::openrazer_db::DEVICES
        .iter()
        .find(|d| d.direct & feat::DPI != 0 && d.direct & feat::FIRMWARE != 0 && devices::spec_for(d.pid).unwrap().source == devices::Source::OpenRazer)
        .unwrap();
    let info = openrzr::DeviceInfo { pid: d.pid, ..openrzr::DeviceInfo::mock() };
    let mut m = openrzr::RazerMouse::with_transport(&info, Box::new(openrzr::transport::mock::MockNagaPro::new()), None);
    assert!(!m.spec.verified());
    m.set_dpi(800, 800).unwrap();
    let e = m.scan_buttons(1, false, false).unwrap_err();
    assert_eq!(e.code(), "unsupported");
    assert!(m.default_action(0x40).is_none());
    assert_eq!(m.list_profiles(), vec![1]);
    assert_eq!(m.side_plate().unwrap(), None);
}

#[test]
fn profile_roundtrip_through_slots() {
    use openrzr::features::profiles::TEMPORARY;
    let mut m = mouse();
    m.write_macro(5, &macros::from_text("ctrl+a, 30ms, ctrl+c").unwrap()).unwrap();
    m.set_button(0x41, &Action { kind: "macro".into(), macro_id: 5, macro_mode: "once".into(), repeat: 1, ..Default::default() }, 1, false).unwrap();
    let p = m.read_profile(1, "Work").unwrap();
    assert!(p.buttons.len() >= 20);
    assert_eq!(p.used_macros(), vec![5]);
    assert!(p.macros.contains_key("5"));
    assert!(p.dpi_stages.is_some());
    // into slot 3, then only the difference
    let n = m.write_profile(&p, 3, None).unwrap();
    assert!(n >= 20);
    assert_eq!(m.get_button(0x41, 3, false).unwrap().kind, "macro");
    let mut q = p.clone();
    q.set(0x42, false, Action::keyboard(0, 0x3E));
    assert_eq!(m.write_profile(&q, 3, Some(&p)).unwrap(), 1);
    assert_eq!(m.get_button(0x42, 3, false).unwrap().describe(), "Key: F5");
    // temporary layer
    m.write_profile(&q, TEMPORARY, None).unwrap();
    assert_eq!(m.get_button(0x42, 0, false).unwrap().describe(), "Key: F5");
    // factory profile
    let f = m.factory_profile("Default").unwrap();
    assert_eq!(f.button(0x42).unwrap().normal.describe(), "Key: 3");
}

#[test]
fn probing_a_database_model() {
    // the emulator answers like a current-generation mouse; pretend it is a Basilisk V3 (database entry)
    let info = DeviceInfo { pid: 0x0099, ..DeviceInfo::mock() };
    let mut m = RazerMouse::open(&info).unwrap();
    assert_eq!(m.spec.source, openrzr::devices::Source::OpenRazer);
    assert!(m.scan_buttons(1, false, false).is_err());
    m.probe();
    use openrzr::devices::feat;
    assert_eq!(m.experimental & (feat::BUTTONS | feat::PROFILES | feat::MACROS), feat::BUTTONS | feat::PROFILES | feat::MACROS);
    let b = m.scan_buttons(1, false, false).unwrap();
    assert!(b.iter().any(|x| x.id == 0x01 && x.name == "Left button"));
    // a verified module is not probed
    let mut n = RazerMouse::open(&DeviceInfo::mock()).unwrap();
    n.probe();
    assert_eq!(n.experimental, 0);
}
