//! open-rzr-cli — command line for Razer mice.

use std::process::ExitCode;

use openrzr::bindings::{DPI_AIM, DPI_FIXED, PROFILE_FIXED};
use openrzr::device::{find_devices, RazerMouse};
use openrzr::lighting::{self, ZoneState};
use openrzr::logs::{self, Level};
use openrzr::{hidkeys, macros, udev, Action, Error};

const HELP: &str = "open-rzr-cli — configure Razer mice without Synapse

Global flags: --mock (emulator)  -v (log to console)  --profile N (1-5, 0 = temporary)
              --log-level debug|info|warn|error
              --openrazer auto|on|off   OpenRazer compatibility (Linux, default auto)

Commands:
  list                                 devices found
  devices [TEXT]                       supported models (own modules and the OpenRazer database)
  openrazer [status|stop]              OpenRazer driver/daemon state; stop the daemon
  info                                 model, firmware, features, DPI, battery
  dpi [X [Y]]                          show / set DPI
  stages [DPI...] [--active N]         DPI stages
  poll [125|500|1000]                  polling rate
  idle [SECONDS]                       sleep after idle (60-900)
  lowbatt [PERCENT]                    low power mode threshold (5-100)
  buttons [--all] [--hypershift] [--plate auto|12|6|2|all]
                                       buttons and bindings (Naga: attached plate only)
  plate                                which side plate is attached (Naga Pro)
  bind ID KIND [ARGS] [--hypershift]
       off | default | mouse left|right|middle|back|forward|wheelup|wheeldown [--double] [--turbo MS]
       key ctrl+shift+c [--turbo MS] | media volup|voldown|mute|play|next|prev
       macro ID [--mode once|hold|toggle|sequence] [--repeat N]
       dpi next|prev|cycle|cycleback|stage N|aim DPI | profile next|prev|cycle|cycleback|N
       hypershift | rawbind HEX
  copy-profile FROM TO                 copy bindings and DPI stages between profiles
  macro list | read ID | write ID \"ctrl+a, 30ms, ctrl+c\" | delete ID | usage
  light ZONE|all EFFECT [COLOUR...] [--speed 1-4]   zones: scroll logo side left right backlight matrix
       off|static|spectrum|wave|breath|breath2|breathrandom|reactive|custom
  brightness ZONE|all [PERCENT]
  backup FILE [--profiles 1,2,3]       backup to JSON
  restore FILE                         restore from a backup
  raw CMD [BYTES...] [--size N]        any command (hex), e.g.: raw 0085 --size 1
  logs [N]                             log file path and the last N lines
  watch                                print Razer devices being plugged in / removed (Ctrl+C to stop)
  razer [status|stop|stop-autostart]   Windows: Razer Synapse services and programs; stop them
  install-udev [--print]               Linux: mouse access without root (--print: show the rule)
  version
";

struct Args {
    pos: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

const VALUE_FLAGS: &[&str] = &["--plate", "--profile", "--active", "--turbo", "--mode", "--repeat", "--speed",
                               "--profiles", "--size", "--log-level", "--openrazer"];

impl Args {
    fn parse(raw: Vec<String>) -> Args {
        let mut pos = Vec::new();
        let mut flags = Vec::new();
        let mut it = raw.into_iter();
        while let Some(a) = it.next() {
            if a.starts_with("--") || a == "-v" || a == "-h" {
                if let Some((k, v)) = a.split_once('=') {
                    flags.push((k.to_string(), Some(v.to_string())));
                } else if VALUE_FLAGS.contains(&a.as_str()) {
                    flags.push((a, it.next()));
                } else {
                    flags.push((a, None));
                }
            } else {
                pos.push(a);
            }
        }
        Args { pos, flags }
    }
    fn has(&self, f: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == f)
    }
    fn val(&self, f: &str) -> Option<&str> {
        self.flags.iter().rev().find(|(k, _)| k == f).and_then(|(_, v)| v.as_deref())
    }
    fn num<T: std::str::FromStr>(&self, f: &str, default: T) -> Result<T, String> {
        match self.val(f) {
            None => Ok(default),
            Some(v) => v.parse().map_err(|_| format!("{f}: bad number {v}")),
        }
    }
    fn arg(&self, i: usize) -> Option<&str> {
        self.pos.get(i).map(String::as_str)
    }
}

fn int(s: &str) -> Result<u32, String> {
    let s = s.trim();
    let r = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(h, 16)
    } else {
        s.parse()
    };
    r.map_err(|_| format!("bad number: {s}"))
}

fn hexnum(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).map_err(|_| format!("bad hex number: {s}"))
}

fn main() -> ExitCode {
    let a = Args::parse(std::env::args().skip(1).collect());
    if a.pos.is_empty() || a.has("--help") || a.has("-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let level = a.val("--log-level").and_then(Level::parse).unwrap_or(Level::Debug);
    logs::init(level, a.has("-v"), true);
    logs::startup_banner("cli");
    if let Some(m) = a.val("--openrazer") {
        match openrzr::compat::Mode::parse(m) {
            Some(m) => openrzr::compat::set_mode(m),
            None => {
                eprintln!("Error: --openrazer auto|on|off");
                return ExitCode::from(2);
            }
        }
    }
    openrzr::log_info!("app", "command: {}", std::env::args().skip(1).collect::<Vec<_>>().join(" "));
    match run(&a) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            openrzr::log_error!("app", "{e}");
            eprintln!("Error: {e}");
            ExitCode::from(1)
        }
    }
}

fn open(a: &Args) -> Result<RazerMouse, String> {
    let mut devs = find_devices(a.has("--mock"));
    if a.has("--mock") {
        // --mock means the emulator only: never touch a real mouse (e.g. in the build smoke test)
        devs.retain(|d| d.backend == "mock");
    }
    RazerMouse::open_first(&devs).map_err(|e| e.to_string())
}

fn e2s(e: Error) -> String {
    e.to_string()
}

fn run(a: &Args) -> Result<(), String> {
    let cmd = a.arg(0).unwrap_or("");
    match cmd {
        "version" => {
            println!("open-rzr {}", openrzr::VERSION);
            println!("Device database: own modules + OpenRazer {} ({} models)", openrzr::devices::openrazer_db::REVISION,
                     openrzr::devices::openrazer_db::DEVICES.len());
            println!("Thanks to the OpenRazer community: https://openrazer.github.io");
            return Ok(());
        }
        "watch" => {
            let w = openrzr::hotplug::watch(|c| println!("{} Razer device {}", openrzr::logs::timestamp(), c.code()))
                .ok_or("USB notifications are not available on this system")?;
            println!("Watching for Razer devices, Ctrl+C to stop…");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
                let _ = &w;
            }
        }
        "razer" => {
            match a.arg(1).unwrap_or("status") {
                "stop" | "stop-autostart" => {
                    let saved = openrzr::synapse::stop(a.arg(1) == Some("stop-autostart")).map_err(e2s)?;
                    println!("Stopped. To start them again:");
                    for (name, start) in saved {
                        if !start.is_empty() {
                            println!("  sc config \"{name}\" start= {start}");
                        }
                        println!("  sc start \"{name}\"");
                    }
                }
                _ => {
                    let sw = openrzr::synapse::scan();
                    if !sw.supported_os {
                        println!("Razer Synapse exists only on Windows.");
                        return Ok(());
                    }
                    for s in &sw.services {
                        println!("service  {:<36} {:<10} start: {}", s.name, if s.running { "running" } else { "stopped" }, s.start);
                    }
                    for p in &sw.processes {
                        println!("program  {p}");
                    }
                    if !sw.active() {
                        println!("Nothing of Razer is running.");
                    }
                }
            }
            return Ok(());
        }
        "devices" => {
            let q = a.arg(1).unwrap_or("").to_lowercase();
            for d in openrzr::devices::all() {
                if !q.is_empty() && !d.name.to_lowercase().contains(&q) && !format!("{:04x}", d.pid).contains(&q) {
                    continue;
                }
                let src = match d.source {
                    openrzr::devices::Source::Verified => "tested",
                    openrzr::devices::Source::Family => "Naga Pro family",
                    openrzr::devices::Source::OpenRazer if openrzr::devices::generic::may_have_onboard(d.transaction_id) => "OpenRazer+probe",
                    openrzr::devices::Source::OpenRazer => "OpenRazer",
                };
                println!("{:04x}  {:<52} {:<9} direct: {}", d.pid, d.name, src, openrzr::feat::names(d.direct).join(" "));
                let extra = d.openrazer & !d.direct;
                if extra != 0 {
                    println!("{:61}+ via OpenRazer driver: {}", "", openrzr::feat::names(extra).join(" "));
                }
            }
            return Ok(());
        }
        "openrazer" => {
            match a.arg(1).unwrap_or("status") {
                "stop" => {
                    openrzr::compat::stop_daemon().map_err(e2s)?;
                    println!("OpenRazer daemon stopped.");
                }
                _ => {
                    let st = openrzr::compat::status();
                    println!("OpenRazer driver:   {}", if st.driver_loaded { "loaded" } else if st.supported_os { "not loaded" } else { "Linux only" });
                    println!("OpenRazer daemon:   {}", if st.daemon_running { "running" } else { "not running" });
                    println!("Compatibility mode: {}", st.mode);
                    for (pid, dir) in &st.bound {
                        let name = openrzr::devices::spec_for(*pid).map(|s| s.name).unwrap_or("?");
                        println!("  {pid:04x} {name}: {dir}");
                    }
                }
            }
            return Ok(());
        }
        "install-udev" => {
            if a.has("--print") {
                print!("{}", udev::rule_text());
                return Ok(());
            }
            udev::install(false)?;
            println!("Rule written to {}. Re-plug the mouse or receiver.", udev::RULE_PATH);
            return Ok(());
        }
        "logs" => {
            let n = a.arg(1).map(int).transpose()?.unwrap_or(40) as usize;
            println!("Log: {}", logs::log_file().display());
            for l in logs::tail(n) {
                println!("{l}");
            }
            return Ok(());
        }
        "list" => {
            let devs = find_devices(a.has("--mock"));
            if devs.is_empty() {
                println!("No Razer devices found.");
            }
            for d in devs {
                println!("{:<60} {}", d.label(), d.path);
            }
            return Ok(());
        }
        _ => {}
    }
    let mut m = open(a)?;
    let profile: u8 = a.num("--profile", 1)?;
    let hs = a.has("--hypershift");
    match cmd {
        "info" => {
            use openrzr::feat;
            println!("Device:      {}", m.name());
            println!(
                "Module:      {}",
                match m.spec.source {
                    openrzr::devices::Source::Verified => "tested",
                    openrzr::devices::Source::Family => "Naga Pro family (not tested on this model)",
                    openrzr::devices::Source::OpenRazer => "OpenRazer database (not tested by open-rzr)",
                }
            );
            if m.experimental != 0 {
                println!("Probed:      {} (experimental)", openrzr::devices::feat::names(m.experimental).join(", "));
            }
            println!("Path:        {}", m.info.path);
            if let Some(d) = &m.openrazer {
                println!("Via:         OpenRazer driver ({})", d.display());
            }
            println!("Features:    {}", feat::names(m.features).join(", "));
            println!("Firmware:    {}", m.firmware().map_err(e2s)?);
            if m.has(feat::SERIAL) {
                println!("Serial:      {}", m.serial().unwrap_or_default());
            }
            if m.has(feat::DPI) {
                let (x, y) = m.get_dpi().map_err(e2s)?;
                println!("DPI:         {x}x{y}");
            }
            if m.has(feat::POLLING) {
                println!("Polling:     {} Hz", m.get_polling_rate().map_err(e2s)?);
            }
            if m.has(feat::BATTERY) {
                let ch = m.is_charging().unwrap_or(false);
                println!("Battery:     {}%{}", m.get_battery().map_err(e2s)?, if ch { " (charging)" } else { "" });
                if m.has(feat::IDLE) {
                    println!("Sleep after: {} s", m.get_idle_time().map_err(e2s)?);
                }
            }
            println!("Profiles:    {:?}", m.list_profiles());
        }
        "dpi" => {
            if let Some(x) = a.arg(1) {
                let x = int(x)? as u16;
                let y = a.arg(2).map(int).transpose()?.map(|v| v as u16).unwrap_or(x);
                m.set_dpi(x, y).map_err(e2s)?;
            }
            let (x, y) = m.get_dpi().map_err(e2s)?;
            println!("DPI: {x}x{y}");
        }
        "stages" => {
            if a.pos.len() > 1 {
                let st: Vec<(u16, u16)> = a.pos[1..].iter().map(|s| int(s).map(|v| (v as u16, v as u16))).collect::<Result<_, _>>()?;
                m.set_dpi_stages(&st, a.num("--active", 1)?, profile).map_err(e2s)?;
            }
            let st = m.get_dpi_stages(profile).map_err(e2s)?;
            for (i, (x, y)) in st.stages.iter().enumerate() {
                println!("{} {}: {x}x{y}", if i + 1 == st.active as usize { "*" } else { " " }, i + 1);
            }
        }
        "poll" => {
            if let Some(hz) = a.arg(1) {
                m.set_polling_rate(int(hz)?).map_err(e2s)?;
            }
            println!("Polling rate: {} Hz", m.get_polling_rate().map_err(e2s)?);
        }
        "idle" => {
            if let Some(s) = a.arg(1) {
                m.set_idle_time(int(s)? as u16).map_err(e2s)?;
            }
            println!("Sleep after {} s", m.get_idle_time().map_err(e2s)?);
        }
        "lowbatt" => {
            if let Some(p) = a.arg(1) {
                m.set_low_battery_threshold(int(p)? as u8).map_err(e2s)?;
            }
            println!("Low power mode below: {}%", m.get_low_battery_threshold().map_err(e2s)?);
        }
        "plate" => match m.side_plate().map_err(e2s)? {
            None => println!("This mouse has no swappable side plates"),
            Some(0) => println!("No side plate attached"),
            Some(p) => println!("Side plate: {p} buttons"),
        },
        "buttons" => {
            let want = a.val("--plate").unwrap_or("auto");
            let plate: Option<u8> = match want {
                "all" => None,
                "auto" => match m.side_plate() {
                    Ok(Some(p)) if p > 0 => Some(p),
                    _ => None,
                },
                n => Some(int(n)? as u8),
            };
            if let Some(p) = plate {
                println!("Side plate: {p} buttons (all plates: --plate all)");
            }
            for b in m.scan_buttons(profile, hs, a.has("--all")).map_err(e2s)? {
                if plate.is_some_and(|p| !openrzr::devices::button_on_plate(b.id, p)) {
                    continue;
                }
                println!("0x{:02x}  {:<28} {}", b.id, b.name, b.action.describe());
            }
        }
        "bind" => {
            let id = int(a.arg(1).ok_or("button ID required")?)? as u8;
            let kind = a.arg(2).ok_or("action kind required")?;
            let act = if kind == "default" {
                m.default_action(id).ok_or("no factory default known for this button")?
            } else {
                build_action(kind, &a.pos[3..], a)?
            };
            m.set_button(id, &act, profile, hs).map_err(e2s)?;
            println!("0x{id:02x} -> {}", m.get_button(id, profile, hs).map_err(e2s)?.describe());
        }
        "copy-profile" => {
            let from = int(a.arg(1).ok_or("FROM")?)? as u8;
            let to = int(a.arg(2).ok_or("TO")?)? as u8;
            let n = m.copy_profile(from, to).map_err(e2s)?;
            println!("Profile {from} -> {to}: {n} bindings copied");
        }
        "macro" => match a.arg(1).unwrap_or("list") {
            "list" => {
                for id in m.macro_ids().map_err(e2s)? {
                    let s: Vec<String> = m.read_macro(id).map_err(e2s)?.iter().map(|s| s.describe()).collect();
                    println!("#{id}: {}", s.join(", "));
                }
            }
            "read" => {
                let id = int(a.arg(2).ok_or("ID required")?)? as u16;
                for s in m.read_macro(id).map_err(e2s)? {
                    println!("{}", s.describe());
                }
            }
            "write" => {
                let id = int(a.arg(2).ok_or("ID required")?)? as u16;
                let steps = macros::from_text(a.arg(3).ok_or("macro text required")?)?;
                let n = m.write_macro(id, &steps).map_err(e2s)?;
                println!("Macro #{id} written ({n} bytes)");
            }
            "delete" => {
                let id = int(a.arg(2).ok_or("ID required")?)? as u16;
                m.delete_macro(id).map_err(e2s)?;
                println!("Macro #{id} deleted");
            }
            "usage" => {
                let (t, f, r) = m.flash_usage().map_err(e2s)?;
                println!("Flash: total {t} B, free {f} B, reclaimable {r} B");
            }
            x => return Err(format!("macro {x}: unknown action")),
        },
        "light" => {
            let zone = a.arg(1).ok_or("zone: scroll|logo|side|all")?;
            let eff = match a.arg(2).ok_or("effect required")? {
                "breath" => "breath_single",
                "breath2" => "breath_dual",
                "breathrandom" => "breath_random",
                e => e,
            };
            let cols: Vec<lighting::Rgb> = a.pos[3..].iter().map(|c| lighting::parse_color(c)).collect::<Result<_, _>>()?;
            let mut st = ZoneState { effect: eff.into(), speed: a.num("--speed", 2)?, ..Default::default() };
            if let Some(c) = cols.first() {
                st.color1 = *c;
                st.color2 = *cols.get(1).unwrap_or(&st.color2);
                st.leds = (0..lighting::SIDE_LEDS).map(|i| *cols.get(i).unwrap_or(cols.last().unwrap())).collect();
            }
            let zones: Vec<&str> = if zone == "all" { m.zones.iter().map(|z| z.id).collect() } else { vec![zone] };
            for z in zones {
                if !m.zone_effects(z).contains(&eff) {
                    println!("{}: effect \"{}\" not supported, skipped", lighting::zone_name(z), lighting::effect_name(eff));
                    continue;
                }
                m.set_lighting(z, &st).map_err(e2s)?;
                println!("{}: {}", lighting::zone_name(z), lighting::effect_name(eff));
            }
        }
        "brightness" => {
            let zone = a.arg(1).ok_or("zone: scroll|logo|side|left|right|backlight|matrix|all")?;
            let zones: Vec<&str> = if zone == "all" { m.zones.iter().map(|z| z.id).collect() } else { vec![zone] };
            for z in zones {
                if let Some(p) = a.arg(2) {
                    m.set_brightness(z, int(p)? as u8).map_err(e2s)?;
                }
                println!("{}: brightness {}%", lighting::zone_name(z), m.get_brightness(z).map_err(e2s)?);
            }
        }
        "backup" | "restore" => backup_restore(cmd, &mut m, a)?,
        "raw" => {
            let c = hexnum(a.arg(1).ok_or("command code required")?)? as u16;
            let bytes: Vec<u8> = a.pos[2..].iter().map(|b| hexnum(b).map(|v| v as u8)).collect::<Result<_, _>>()?;
            let size = a.val("--size").map(int).transpose()?.map(|v| v as u8);
            let mut ch_args = bytes.clone();
            ch_args.truncate(80);
            let r = m.req(c, &ch_args, size).map_err(e2s)?;
            let n = size.unwrap_or(16).max(1) as usize;
            println!("OK args={}", r[..n.min(80)].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
        }
        x => return Err(format!("unknown command {x}. Help: open-rzr-cli --help")),
    }
    Ok(())
}

#[cfg(feature = "json")]
fn backup_restore(cmd: &str, m: &mut RazerMouse, a: &Args) -> Result<(), String> {
    let file = a.arg(1).ok_or("file required")?;
    if cmd == "backup" {
        let profiles: Vec<u8> = a
            .val("--profiles")
            .unwrap_or("1")
            .split(',')
            .map(|p| int(p).map(|v| v as u8))
            .collect::<Result<_, _>>()?;
        let b = m.backup(&profiles).map_err(e2s)?;
        let json = serde_json::to_string_pretty(&b).map_err(|e| e.to_string())?;
        std::fs::write(file, json).map_err(|e| format!("{file}: {e}"))?;
        println!("Saved: {file} (profiles {profiles:?}, macros {})", b.macros.len());
    } else {
        let s = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
        let b: openrzr::Backup = serde_json::from_str(&s).map_err(|e| format!("{file}: {e}"))?;
        println!("Records restored: {}", m.restore(&b).map_err(e2s)?);
    }
    Ok(())
}

#[cfg(not(feature = "json"))]
fn backup_restore(_: &str, _: &mut RazerMouse, _: &Args) -> Result<(), String> {
    Err("built without JSON support (feature json)".into())
}

fn build_action(kind: &str, args: &[String], a: &Args) -> Result<Action, String> {
    let first = args.first().map(String::as_str).unwrap_or("");
    let turbo: u16 = a.num("--turbo", 0)?;
    Ok(match kind {
        "off" => Action::kind("disabled"),
        "default" => return Err("use: bind ID default".into()),
        "mouse" => {
            let b = match first.to_ascii_lowercase().as_str() {
                "left" => 1, "right" => 2, "middle" => 3, "back" => 4, "forward" => 5,
                "wheelup" => 9, "wheeldown" => 10, "wheelleft" => 0x68, "wheelright" => 0x69,
                x => return Err(format!("mouse button: {x}")),
            };
            if turbo > 0 {
                Action { kind: "mouse_turbo".into(), button: b, interval: turbo, ..Default::default() }
            } else if a.has("--double") {
                Action { kind: "double_click".into(), button: b, ..Default::default() }
            } else {
                Action::mouse(b)
            }
        }
        "key" => {
            let (mods, key) = hidkeys::parse_combo(first)?;
            if turbo > 0 {
                Action { kind: "keyboard_turbo".into(), modifiers: mods, key, interval: turbo, ..Default::default() }
            } else {
                Action::keyboard(mods, key)
            }
        }
        "media" => {
            let code = match first.to_ascii_lowercase().as_str() {
                "volup" => 0xE9, "voldown" => 0xEA, "mute" => 0xE2, "play" => 0xCD,
                "next" => 0xB5, "prev" => 0xB6, x => int(x)? as u16,
            };
            Action { kind: "consumer".into(), code, ..Default::default() }
        }
        "macro" => Action {
            kind: "macro".into(),
            macro_id: int(first)? as u16,
            macro_mode: a.val("--mode").unwrap_or("once").into(),
            repeat: a.num("--repeat", 1)?,
            ..Default::default()
        },
        "dpi" => match first {
            "stage" => Action { kind: "dpi".into(), code: DPI_FIXED as u16, stage: int(args.get(1).ok_or("stage number")?)? as u8, ..Default::default() },
            "aim" => {
                let v = int(args.get(1).ok_or("sniper DPI")?)? as u16;
                Action { kind: "dpi".into(), code: DPI_AIM as u16, dpi_x: v, dpi_y: v, ..Default::default() }
            }
            op => {
                let code = match op { "next" => 1, "prev" => 2, "cycle" => 6, "cycleback" => 7, x => return Err(format!("dpi {x}")) };
                Action { kind: "dpi".into(), code, ..Default::default() }
            }
        },
        "profile" => match first {
            "next" => Action { kind: "profile".into(), code: 1, ..Default::default() },
            "prev" => Action { kind: "profile".into(), code: 2, ..Default::default() },
            "cycle" => Action { kind: "profile".into(), code: 4, ..Default::default() },
            "cycleback" => Action { kind: "profile".into(), code: 5, ..Default::default() },
            n => Action { kind: "profile".into(), code: PROFILE_FIXED as u16, profile: int(n)? as u8, ..Default::default() },
        },
        "hypershift" => Action::kind("hypershift"),
        "rawbind" => {
            let hex: String = args.concat();
            let raw = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)).collect::<Result<Vec<_>, _>>().map_err(|_| "hex")?;
            Action { kind: "raw".into(), raw, ..Default::default() }
        }
        x => return Err(format!("unknown action kind {x}")),
    })
}
