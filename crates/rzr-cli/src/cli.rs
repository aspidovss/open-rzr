//! open-rzr-cli — command line for Razer mice (arguments parsed with clap).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use openrzr::bindings::{DPI_AIM, DPI_FIXED, PROFILE_FIXED};
use openrzr::device::{find_devices, RazerMouse};
use openrzr::lighting::{self, ZoneState};
use openrzr::logs::{self, Level};
use openrzr::{hidkeys, macros, udev, Action, Error};

const BIND_HELP: &str = "\
Binding kinds (bind ID KIND ARGS):
  off | default
  mouse left|right|middle|back|forward|wheelup|wheeldown|wheelleft|wheelright [--double] [--turbo MS]
  key ctrl+shift+c [--turbo MS]
  media volup|voldown|mute|play|next|prev
  macro ID [--mode once|hold|toggle|sequence] [--repeat N]
  dpi next|prev|cycle|cycleback | dpi stage N | dpi aim DPI
  profile next|prev|cycle|cycleback|N
  hypershift | rawbind HEX

Numbers may be decimal or hex (0x40). Examples:
  open-rzr-cli bind 0x40 key ctrl+shift+c
  open-rzr-cli --profile 2 bind 0x41 mouse back
  open-rzr-cli light all static 00ff80
  open-rzr-cli -v raw 0085 --size 1";

/// Configure Razer mice without Synapse. No network, no accounts.
#[derive(Parser, Debug)]
#[command(name = "open-rzr-cli", version, after_help = BIND_HELP, arg_required_else_help = true)]
struct Cli {
    /// Use the emulator only (never touches a real mouse)
    #[arg(long, global = true)]
    mock: bool,

    /// Print the log to the console too
    #[arg(short = 'v', long = "verbose", global = true)]
    verbose: bool,

    /// On-board profile 1-5 (0 = temporary, not stored)
    #[arg(long, global = true, default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=5))]
    profile: u8,

    /// Log level
    #[arg(long, global = true, value_name = "LEVEL", value_parser = ["debug", "info", "warn", "error"])]
    log_level: Option<String>,

    /// OpenRazer compatibility (Linux)
    #[arg(long, global = true, value_name = "MODE", value_parser = ["auto", "on", "off"])]
    openrazer: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Devices found
    List,
    /// Supported models (own modules, your device files and the OpenRazer database)
    Devices {
        /// Part of the name or product id
        text: Option<String>,
    },
    /// Your device description files (*.toml / *.json) and their errors
    DeviceFiles,
    /// OpenRazer driver/daemon state; stop the daemon (Linux)
    Openrazer {
        #[arg(default_value = "status", value_parser = ["status", "stop"])]
        action: String,
    },
    /// Model, firmware, features, DPI, battery
    Info,
    /// Show / set DPI
    Dpi {
        #[arg(value_parser = num_u16)]
        x: Option<u16>,
        #[arg(value_parser = num_u16)]
        y: Option<u16>,
    },
    /// Show / set DPI stages
    Stages {
        #[arg(value_parser = num_u16)]
        dpi: Vec<u16>,
        /// Active stage (1-based)
        #[arg(long, default_value_t = 1)]
        active: u8,
    },
    /// Polling rate (125, 500, 1000 Hz)
    Poll {
        #[arg(value_parser = num_u32)]
        hz: Option<u32>,
    },
    /// Sleep after idle, seconds (60-900)
    Idle {
        #[arg(value_parser = num_u16)]
        seconds: Option<u16>,
    },
    /// Low power mode threshold, percent (5-100)
    Lowbatt {
        #[arg(value_parser = num_u8)]
        percent: Option<u8>,
    },
    /// Buttons and their bindings (Naga: attached plate only)
    Buttons {
        /// Also list disabled buttons
        #[arg(long)]
        all: bool,
        /// The Hypershift layer
        #[arg(long)]
        hypershift: bool,
        /// auto | 12 | 6 | 2 | all
        #[arg(long, default_value = "auto")]
        plate: String,
    },
    /// Which side plate is attached (Naga Pro)
    Plate,
    /// Bind a button (see the binding kinds below)
    Bind {
        #[arg(value_parser = num_u8)]
        id: u8,
        /// off | default | mouse | key | media | macro | dpi | profile | hypershift | rawbind
        kind: String,
        /// Arguments of the binding
        args: Vec<String>,
        /// The Hypershift layer
        #[arg(long)]
        hypershift: bool,
        /// mouse: double click
        #[arg(long)]
        double: bool,
        /// mouse / key: repeat every MS milliseconds while held
        #[arg(long, value_name = "MS", value_parser = num_u16)]
        turbo: Option<u16>,
        /// macro: once | hold | toggle | sequence
        #[arg(long, default_value = "once", value_parser = ["once", "hold", "toggle", "sequence"])]
        mode: String,
        /// macro: how many times (mode once)
        #[arg(long, default_value_t = 1)]
        repeat: u8,
    },
    /// Copy bindings and DPI stages between profiles
    CopyProfile {
        #[arg(value_parser = num_u8)]
        from: u8,
        #[arg(value_parser = num_u8)]
        to: u8,
    },
    /// On-board macros
    Macro {
        #[command(subcommand)]
        action: Option<MacroCmd>,
    },
    /// Lighting: ZONE (scroll logo side left right backlight matrix | all) EFFECT [COLOUR...]
    Light {
        zone: String,
        /// off | static | spectrum | wave | breath | breath2 | breathrandom | reactive | custom
        effect: String,
        /// Colours as RRGGBB
        colours: Vec<String>,
        /// Reactive afterglow 1-4
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(1..=4))]
        speed: u8,
    },
    /// Brightness of a zone (or all), percent
    Brightness {
        zone: String,
        #[arg(value_parser = num_u8)]
        percent: Option<u8>,
    },
    /// Back up profiles and macros to a JSON file
    Backup {
        file: String,
        /// Profiles to save, e.g. 1,2,3
        #[arg(long, default_value = "1")]
        profiles: String,
    },
    /// Restore from a backup file
    Restore { file: String },
    /// Any command (hex), e.g.: raw 0085 --size 1
    Raw {
        /// Command code, hex
        cmd: String,
        /// Argument bytes, hex
        bytes: Vec<String>,
        /// Data size of the request
        #[arg(long, value_parser = num_u8)]
        size: Option<u8>,
    },
    /// Log file path and the last N lines
    Logs {
        #[arg(default_value_t = 40)]
        n: usize,
    },
    /// Print Razer devices being plugged in / removed (Ctrl+C to stop)
    Watch,
    /// Windows: Razer Synapse services and programs; stop them
    Razer {
        #[arg(default_value = "status", value_parser = ["status", "stop", "stop-autostart"])]
        action: String,
    },
    /// Linux: mouse access without root
    InstallUdev {
        /// Only show the rule
        #[arg(long)]
        print: bool,
    },
    /// Version and device database
    Version,
}

#[derive(Subcommand, Debug)]
enum MacroCmd {
    /// List macros with their steps
    List,
    /// Steps of one macro
    Read {
        #[arg(value_parser = num_u16)]
        id: u16,
    },
    /// Write a macro: write 3 "ctrl+a, 30ms, ctrl+c"
    Write {
        #[arg(value_parser = num_u16)]
        id: u16,
        text: String,
    },
    /// Delete a macro
    Delete {
        #[arg(value_parser = num_u16)]
        id: u16,
    },
    /// Flash memory usage
    Usage,
}

/// Decimal or 0x-hex number.
fn int(s: &str) -> Result<u32, String> {
    let s = s.trim();
    let r = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(h, 16)
    } else {
        s.parse()
    };
    r.map_err(|_| format!("bad number: {s}"))
}

fn num_u32(s: &str) -> Result<u32, String> {
    int(s)
}

fn num_u16(s: &str) -> Result<u16, String> {
    u16::try_from(int(s)?).map_err(|_| format!("too large: {s}"))
}

fn num_u8(s: &str) -> Result<u8, String> {
    u8::try_from(int(s)?).map_err(|_| format!("too large: {s}"))
}

fn hexnum(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).map_err(|_| format!("bad hex number: {s}"))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let level = cli.log_level.as_deref().and_then(Level::parse).unwrap_or(Level::Debug);
    logs::init(level, cli.verbose, true);
    logs::startup_banner("cli");
    openrzr::caps::load_device_files();
    if let Some(m) = cli.openrazer.as_deref().and_then(openrzr::compat::Mode::parse) {
        openrzr::compat::set_mode(m);
    }
    openrzr::log_info!("app", "command: {}", std::env::args().skip(1).collect::<Vec<_>>().join(" "));
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            openrzr::log_error!("app", "{e}");
            eprintln!("Error: {e}");
            ExitCode::from(1)
        }
    }
}

fn open(cli: &Cli) -> Result<RazerMouse, String> {
    let mut devs = find_devices(cli.mock);
    if cli.mock {
        // --mock means the emulator only: never touch a real mouse (e.g. in the build smoke test)
        devs.retain(|d| d.backend == "mock");
    }
    RazerMouse::open_first(&devs).map_err(|e| e.to_string())
}

fn e2s(e: Error) -> String {
    e.to_string()
}

/// Commands that do not need a mouse; `Ok(false)` = not one of them.
fn run_without_mouse(cli: &Cli) -> Result<bool, String> {
    match &cli.cmd {
        Cmd::Version => {
            println!("open-rzr {}", openrzr::VERSION);
            println!(
                "Device database: own modules + OpenRazer {} ({} models)",
                openrzr::devices::openrazer_db::REVISION,
                openrzr::devices::openrazer_db::DEVICES.len()
            );
            println!("Thanks to the OpenRazer community: https://openrazer.github.io");
        }
        Cmd::Watch => {
            let w = openrzr::hotplug::watch(|c| println!("{} Razer device {}", openrzr::logs::timestamp(), c.code()))
                .ok_or("USB notifications are not available on this system")?;
            println!("Watching for Razer devices, Ctrl+C to stop…");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
                let _ = &w;
            }
        }
        Cmd::Razer { action } => match action.as_str() {
            "stop" | "stop-autostart" => {
                let saved = openrzr::synapse::stop(action == "stop-autostart").map_err(e2s)?;
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
                    return Ok(true);
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
        },
        Cmd::DeviceFiles => {
            let dir = openrzr::devices::external::folder(&logs::config_dir());
            println!("Folder: {}", dir.display());
            let device_files = openrzr::devices::external::load_dir(&dir);
            if device_files.is_empty() {
                println!("No *.toml / *.json files.");
            }
            for l in &device_files {
                if l.error.is_empty() {
                    println!("OK     {}  0x{:04x} {} (based on {})", l.file, l.pid, l.name, l.base);
                } else {
                    println!("ERROR  {}  {}", l.file, l.error);
                }
                for w in &l.warnings {
                    println!("       warning: {w}");
                }
            }
        }
        Cmd::Devices { text } => {
            let q = text.as_deref().unwrap_or("").to_lowercase();
            for d in openrzr::devices::all() {
                if !q.is_empty() && !d.name.to_lowercase().contains(&q) && !format!("{:04x}", d.pid).contains(&q) {
                    continue;
                }
                let src = match d.source {
                    openrzr::devices::Source::Verified => "tested",
                    openrzr::devices::Source::Family => "Naga Pro family",
                    openrzr::devices::Source::OpenRazer if openrzr::devices::generic::may_have_onboard(d.transaction_id) => "OpenRazer+probe",
                    openrzr::devices::Source::OpenRazer => "OpenRazer",
                    openrzr::devices::Source::User => "file",
                };
                println!("{:04x}  {:<52} {:<9} direct: {}", d.pid, d.name, src, openrzr::feat::names(d.direct).join(" "));
                let extra = d.openrazer & !d.direct;
                if extra != 0 {
                    println!("{:61}+ via OpenRazer driver: {}", "", openrzr::feat::names(extra).join(" "));
                }
            }
        }
        Cmd::Openrazer { action } => {
            if action == "stop" {
                openrzr::compat::stop_daemon().map_err(e2s)?;
                println!("OpenRazer daemon stopped.");
            } else {
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
        Cmd::InstallUdev { print } => {
            if *print {
                print!("{}", udev::rule_text());
            } else {
                udev::install(false)?;
                println!("Rule written to {}. Re-plug the mouse or receiver.", udev::RULE_PATH);
            }
        }
        Cmd::Logs { n } => {
            println!("Log: {}", logs::log_file().display());
            for l in logs::tail(*n) {
                println!("{l}");
            }
        }
        Cmd::List => {
            let devs = find_devices(cli.mock);
            if devs.is_empty() {
                println!("No Razer devices found.");
            }
            for d in devs {
                println!("{:<60} {}", d.label(), d.path);
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn run(cli: &Cli) -> Result<(), String> {
    if run_without_mouse(cli)? {
        return Ok(());
    }
    let mut m = open(cli)?;
    let profile = cli.profile;
    match &cli.cmd {
        Cmd::Info => {
            use openrzr::feat;
            println!("Device:      {}", m.name());
            println!(
                "Module:      {}",
                match m.spec.source {
                    openrzr::devices::Source::Verified => "tested",
                    openrzr::devices::Source::Family => "Naga Pro family (not tested on this model)",
                    openrzr::devices::Source::OpenRazer => "OpenRazer database (not tested by open-rzr)",
                    openrzr::devices::Source::User => "your description file (devices folder)",
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
        Cmd::Dpi { x, y } => {
            if let Some(x) = *x {
                m.set_dpi(x, y.unwrap_or(x)).map_err(e2s)?;
            }
            let (x, y) = m.get_dpi().map_err(e2s)?;
            println!("DPI: {x}x{y}");
        }
        Cmd::Stages { dpi, active } => {
            if !dpi.is_empty() {
                let st: Vec<(u16, u16)> = dpi.iter().map(|v| (*v, *v)).collect();
                m.set_dpi_stages(&st, *active, profile).map_err(e2s)?;
            }
            let st = m.get_dpi_stages(profile).map_err(e2s)?;
            for (i, (x, y)) in st.stages.iter().enumerate() {
                println!("{} {}: {x}x{y}", if i + 1 == st.active as usize { "*" } else { " " }, i + 1);
            }
        }
        Cmd::Poll { hz } => {
            if let Some(hz) = *hz {
                m.set_polling_rate(hz).map_err(e2s)?;
            }
            println!("Polling rate: {} Hz", m.get_polling_rate().map_err(e2s)?);
        }
        Cmd::Idle { seconds } => {
            if let Some(s) = *seconds {
                m.set_idle_time(s).map_err(e2s)?;
            }
            println!("Sleep after {} s", m.get_idle_time().map_err(e2s)?);
        }
        Cmd::Lowbatt { percent } => {
            if let Some(p) = *percent {
                m.set_low_battery_threshold(p).map_err(e2s)?;
            }
            println!("Low power mode below: {}%", m.get_low_battery_threshold().map_err(e2s)?);
        }
        Cmd::Plate => match m.side_plate().map_err(e2s)? {
            None => println!("This mouse has no swappable side plates"),
            Some(0) => println!("No side plate attached"),
            Some(p) => println!("Side plate: {p} buttons"),
        },
        Cmd::Buttons { all, hypershift, plate } => {
            let plate: Option<u8> = match plate.as_str() {
                "all" => None,
                "auto" => match m.side_plate() {
                    Ok(Some(p)) if p > 0 => Some(p),
                    _ => None,
                },
                n => Some(num_u8(n)?),
            };
            if let Some(p) = plate {
                println!("Side plate: {p} buttons (all plates: --plate all)");
            }
            for b in m.scan_buttons(profile, *hypershift, *all).map_err(e2s)? {
                if plate.is_some_and(|p| !openrzr::devices::button_on_plate(b.id, p)) {
                    continue;
                }
                println!("0x{:02x}  {:<28} {}", b.id, b.name, b.action.describe());
            }
        }
        Cmd::Bind { id, kind, args, hypershift, double, turbo, mode, repeat } => {
            let act = if kind == "default" {
                m.default_action(*id).ok_or("no factory default known for this button")?
            } else {
                build_action(kind, args, BindOpts { double: *double, turbo: turbo.unwrap_or(0), mode, repeat: *repeat })?
            };
            m.set_button(*id, &act, profile, *hypershift).map_err(e2s)?;
            println!("0x{id:02x} -> {}", m.get_button(*id, profile, *hypershift).map_err(e2s)?.describe());
        }
        Cmd::CopyProfile { from, to } => {
            let n = m.copy_profile(*from, *to).map_err(e2s)?;
            println!("Profile {from} -> {to}: {n} bindings copied");
        }
        Cmd::Macro { action } => match action.as_ref().unwrap_or(&MacroCmd::List) {
            MacroCmd::List => {
                for id in m.macro_ids().map_err(e2s)? {
                    let s: Vec<String> = m.read_macro(id).map_err(e2s)?.iter().map(|s| s.describe()).collect();
                    println!("#{id}: {}", s.join(", "));
                }
            }
            MacroCmd::Read { id } => {
                for s in m.read_macro(*id).map_err(e2s)? {
                    println!("{}", s.describe());
                }
            }
            MacroCmd::Write { id, text } => {
                let steps = macros::from_text(text)?;
                let n = m.write_macro(*id, &steps).map_err(e2s)?;
                println!("Macro #{id} written ({n} bytes)");
            }
            MacroCmd::Delete { id } => {
                m.delete_macro(*id).map_err(e2s)?;
                println!("Macro #{id} deleted");
            }
            MacroCmd::Usage => {
                let (t, f, r) = m.flash_usage().map_err(e2s)?;
                println!("Flash: total {t} B, free {f} B, reclaimable {r} B");
            }
        },
        Cmd::Light { zone, effect, colours, speed } => {
            let eff = match effect.as_str() {
                "breath" => "breath_single",
                "breath2" => "breath_dual",
                "breathrandom" => "breath_random",
                e => e,
            };
            let cols: Vec<lighting::Rgb> = colours.iter().map(|c| lighting::parse_color(c)).collect::<Result<_, _>>()?;
            let mut st = ZoneState { effect: eff.into(), speed: *speed, ..Default::default() };
            if let (Some(first), Some(last)) = (cols.first(), cols.last()) {
                st.color1 = *first;
                st.color2 = *cols.get(1).unwrap_or(&st.color2);
                st.leds = (0..lighting::SIDE_LEDS).map(|i| *cols.get(i).unwrap_or(last)).collect();
            }
            let zones: Vec<&str> = if zone == "all" { m.zones.iter().map(|z| z.id).collect() } else { vec![zone.as_str()] };
            for z in zones {
                if !m.zone_effects(z).contains(&eff) {
                    println!("{}: effect \"{}\" not supported, skipped", lighting::zone_name(z), lighting::effect_name(eff));
                    continue;
                }
                m.set_lighting(z, &st).map_err(e2s)?;
                println!("{}: {}", lighting::zone_name(z), lighting::effect_name(eff));
            }
        }
        Cmd::Brightness { zone, percent } => {
            let zones: Vec<&str> = if zone == "all" { m.zones.iter().map(|z| z.id).collect() } else { vec![zone.as_str()] };
            for z in zones {
                if let Some(p) = *percent {
                    m.set_brightness(z, p).map_err(e2s)?;
                }
                println!("{}: brightness {}%", lighting::zone_name(z), m.get_brightness(z).map_err(e2s)?);
            }
        }
        Cmd::Backup { file, profiles } => backup(&mut m, file, profiles)?,
        Cmd::Restore { file } => restore(&mut m, file)?,
        Cmd::Raw { cmd, bytes, size } => {
            let c = u16::try_from(hexnum(cmd)?).map_err(|_| format!("command code too large: {cmd}"))?;
            let mut args: Vec<u8> = bytes.iter().map(|b| hexnum(b).map(|v| v as u8)).collect::<Result<_, _>>()?;
            args.truncate(80);
            let r = m.req(c, &args, *size).map_err(e2s)?;
            let n = size.unwrap_or(16).max(1) as usize;
            println!("OK args={}", r[..n.min(80)].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
        }
        // handled in run_without_mouse
        _ => {}
    }
    Ok(())
}

#[cfg(feature = "json")]
fn backup(m: &mut RazerMouse, file: &str, profiles: &str) -> Result<(), String> {
    let profiles: Vec<u8> = profiles.split(',').map(num_u8).collect::<Result<_, _>>()?;
    let b = m.backup(&profiles).map_err(e2s)?;
    let json = serde_json::to_string_pretty(&b).map_err(|e| e.to_string())?;
    std::fs::write(file, json).map_err(|e| format!("{file}: {e}"))?;
    println!("Saved: {file} (profiles {profiles:?}, macros {})", b.macros.len());
    Ok(())
}

#[cfg(feature = "json")]
fn restore(m: &mut RazerMouse, file: &str) -> Result<(), String> {
    let s = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let b: openrzr::Backup = serde_json::from_str(&s).map_err(|e| format!("{file}: {e}"))?;
    println!("Records restored: {}", m.restore(&b).map_err(e2s)?);
    Ok(())
}

#[cfg(not(feature = "json"))]
fn backup(_: &mut RazerMouse, _: &str, _: &str) -> Result<(), String> {
    Err("built without JSON support (feature json)".into())
}

#[cfg(not(feature = "json"))]
fn restore(_: &mut RazerMouse, _: &str) -> Result<(), String> {
    Err("built without JSON support (feature json)".into())
}

/// Options of `bind` that apply to some kinds.
struct BindOpts<'a> {
    double: bool,
    turbo: u16,
    mode: &'a str,
    repeat: u8,
}

fn build_action(kind: &str, args: &[String], o: BindOpts) -> Result<Action, String> {
    let first = args.first().map(String::as_str).unwrap_or("");
    Ok(match kind {
        "off" => Action::kind("disabled"),
        "default" => return Err("use: bind ID default".into()),
        "mouse" => {
            let b = match first.to_ascii_lowercase().as_str() {
                "left" => 1,
                "right" => 2,
                "middle" => 3,
                "back" => 4,
                "forward" => 5,
                "wheelup" => 9,
                "wheeldown" => 10,
                "wheelleft" => 0x68,
                "wheelright" => 0x69,
                x => return Err(format!("mouse button: {x}")),
            };
            if o.turbo > 0 {
                Action { kind: "mouse_turbo".into(), button: b, interval: o.turbo, ..Default::default() }
            } else if o.double {
                Action { kind: "double_click".into(), button: b, ..Default::default() }
            } else {
                Action::mouse(b)
            }
        }
        "key" => {
            let (mods, key) = hidkeys::parse_combo(first)?;
            if o.turbo > 0 {
                Action { kind: "keyboard_turbo".into(), modifiers: mods, key, interval: o.turbo, ..Default::default() }
            } else {
                Action::keyboard(mods, key)
            }
        }
        "media" => {
            let code = match first.to_ascii_lowercase().as_str() {
                "volup" => 0xE9,
                "voldown" => 0xEA,
                "mute" => 0xE2,
                "play" => 0xCD,
                "next" => 0xB5,
                "prev" => 0xB6,
                x => num_u16(x)?,
            };
            Action { kind: "consumer".into(), code, ..Default::default() }
        }
        "macro" => Action {
            kind: "macro".into(),
            macro_id: num_u16(first)?,
            macro_mode: o.mode.into(),
            repeat: o.repeat,
            ..Default::default()
        },
        "dpi" => match first {
            "stage" => Action {
                kind: "dpi".into(),
                code: DPI_FIXED as u16,
                stage: num_u8(args.get(1).ok_or("stage number")?)?,
                ..Default::default()
            },
            "aim" => {
                let v = num_u16(args.get(1).ok_or("sniper DPI")?)?;
                Action { kind: "dpi".into(), code: DPI_AIM as u16, dpi_x: v, dpi_y: v, ..Default::default() }
            }
            op => {
                let code = match op {
                    "next" => 1,
                    "prev" => 2,
                    "cycle" => 6,
                    "cycleback" => 7,
                    x => return Err(format!("dpi {x}")),
                };
                Action { kind: "dpi".into(), code, ..Default::default() }
            }
        },
        "profile" => match first {
            "next" => Action { kind: "profile".into(), code: 1, ..Default::default() },
            "prev" => Action { kind: "profile".into(), code: 2, ..Default::default() },
            "cycle" => Action { kind: "profile".into(), code: 4, ..Default::default() },
            "cycleback" => Action { kind: "profile".into(), code: 5, ..Default::default() },
            n => Action { kind: "profile".into(), code: PROFILE_FIXED as u16, profile: num_u8(n)?, ..Default::default() },
        },
        "hypershift" => Action::kind("hypershift"),
        "rawbind" => {
            let hex: String = args.concat();
            let raw = (0..hex.len() / 2)
                .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "rawbind: hex bytes expected")?;
            Action { kind: "raw".into(), raw, ..Default::default() }
        }
        x => return Err(format!("unknown action kind {x}")),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn old_command_lines_still_parse() {
        let p = |s: &str| Cli::try_parse_from(std::iter::once("open-rzr-cli").chain(s.split_whitespace()));
        let c = p("--profile 2 bind 0x41 mouse back").unwrap();
        assert_eq!(c.profile, 2);
        assert!(matches!(c.cmd, Cmd::Bind { id: 0x41, ref kind, .. } if kind == "mouse"));
        let c = p("bind 0x40 key ctrl+shift+c --turbo 50 --hypershift").unwrap();
        assert!(matches!(c.cmd, Cmd::Bind { turbo: Some(50), hypershift: true, .. }));
        let c = p("stages 800 1600 3200 --active 2").unwrap();
        assert!(matches!(c.cmd, Cmd::Stages { ref dpi, active: 2 } if dpi == &[800, 1600, 3200]));
        assert!(matches!(p("-v raw 0085 --size 1").unwrap().cmd, Cmd::Raw { size: Some(1), .. }));
        assert!(matches!(p("info --mock").unwrap(), Cli { mock: true, cmd: Cmd::Info, .. }));
        assert!(matches!(p("macro").unwrap().cmd, Cmd::Macro { action: None }));
        assert!(matches!(p("macro read 0x10").unwrap().cmd, Cmd::Macro { action: Some(MacroCmd::Read { id: 16 }) }));
        assert!(matches!(p("light all static 00ff80").unwrap().cmd, Cmd::Light { speed: 2, .. }));
        assert!(matches!(p("backup naga.json --profiles 1,2,3").unwrap().cmd, Cmd::Backup { .. }));
        assert!(matches!(p("copy-profile 1 2").unwrap().cmd, Cmd::CopyProfile { from: 1, to: 2 }));
        assert!(matches!(p("device-files").unwrap().cmd, Cmd::DeviceFiles));
        assert!(matches!(p("install-udev --print").unwrap().cmd, Cmd::InstallUdev { print: true }));
        assert!(matches!(p("razer stop-autostart").unwrap().cmd, Cmd::Razer { .. }));
        // errors are caught by the parser
        assert!(p("--profile 9 info").is_err());
        assert!(p("dpi abc").is_err());
        assert!(p("razer launch").is_err());
        assert!(p("light logo static --speed 7").is_err());
    }

    #[test]
    fn actions() {
        let o = || BindOpts { double: false, turbo: 0, mode: "once", repeat: 1 };
        assert_eq!(build_action("mouse", &["back".into()], o()).unwrap().button, 4);
        let t = build_action("key", &["a".into()], BindOpts { turbo: 50, ..o() }).unwrap();
        assert_eq!((t.kind.as_str(), t.interval), ("keyboard_turbo", 50));
        assert_eq!(build_action("dpi", &["aim".into(), "400".into()], o()).unwrap().dpi_x, 400);
        assert!(build_action("rawbind", &["0g".into()], o()).is_err());
    }
}
