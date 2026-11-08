# open-rzr

[Русская версия](README.ru.md)

An open-source configurator for **Razer mice** on **Windows and Linux**, made first of all for the **Razer Naga Pro**. Written in **Rust**, with a **Tauri** window. It replaces Razer Synapse for everything the mouse itself can do.

- **No analytics, no telemetry, no accounts.** The program never talks to the network. Settings live in the mouse memory and in one local `settings.json`.
- **Settings stay in the mouse.** Button bindings, DPI stages, macros and lighting are written to the on-board memory and keep working without the program, on any computer and any OS.
- **Logging is always on.** Everything goes to a local file in English: window actions (`ui`), mouse operations (`device`) and every USB packet with its timing (`usb`).
- **English and Russian interface.** On first start the system language is detected and you are asked to keep it; it can be changed in Settings.
- **117 models known.** The Naga Pro has its own tested module with everything; the other Razer mice come from the [OpenRazer](https://openrazer.github.io) device database with the features it confirms.
- **Works alongside OpenRazer on Linux.** If the OpenRazer driver is attached to the mouse, open-rzr goes through it instead of fighting it.

> Independent project, not affiliated with Razer Inc. No Synapse code is used: the program talks to the mouse over the HID protocol documented by the community (links at the end).

## Features

| Tab | What it does |
|---|---|
| Device | model, device module (tested / OpenRazer database), connection (direct / OpenRazer driver), firmware, serial number, profiles, side plate, battery and charging |
| Performance | up to 5 DPI stages (separate X/Y, 100–20000) per profile with the indicator colour of each stage, polling rate 125/500/1000 Hz |
| Buttons | reads every button; for mice with swappable side plates (Naga Pro) the **attached plate is detected automatically** and only its buttons are listed — swap the plate and the list follows; keys and shortcuts, mouse buttons, double click, turbo, multimedia, macros, DPI, sniper, profiles, Hypershift; **reset to default**; 5 profiles and a temporary layer; find a button by pressing it; **copy profile**; backup and restore |
| Macros | editor like the original: record (real, fixed or no delays), keys, mouse buttons, wheel, media keys, delays, **loops**, **other macros inside a macro**, **typed text**; inline editing, multi-select, drag and drop, undo/redo, total time and size check; **program actions** (launch a program or website, run a command, type any text); local library |
| Lighting | scroll wheel, logo, side panel: static, spectrum, wave, breathing, reactive, custom colours for the 3 side LEDs, brightness, “same for all zones” |
| Power | sleep after idle, low power mode threshold (5–100 %) |
| Diagnostics | live log with filters, log folder, one-click report, raw commands |
| Settings | window style (like the original configurator / classic open-rzr with a sidebar), language, start with the system, start minimized to the tray, close to tray, log level, OpenRazer compatibility, about and supported devices |

Tabs a mouse cannot use are hidden: a model from the OpenRazer database shows only the features the database confirms.

## Profiles

The window follows the layout of the original configurator: tabs for the mouse, macros, settings and diagnostics; under them the **profile** selector with the **•••** menu and the **on-board memory** button, and the sub-tabs Customize / Performance / Lighting / Power / Info. Customize shows a drawing of the mouse with every binding next to its button, the side plate as a keypad, the Standard / Hypershift switch and the side plate choice (the attached plate is ticked).

- **Profiles on this computer** (`%APPDATA%\open-rzr\profiles\*.json`): bindings of both layers, DPI stages, polling rate, lighting, the macros they use and the linked programs. On the first connection the mouse's slots are read into the library (`<COMPUTER>-Default`, `Profile 2`…).
- **•••**: Add (factory bindings, a copy, or a slot of the mouse), Import, Link programs, Rename, Duplicate, Export (to Downloads, `*.openrzr.json`), Read from the mouse, Reset, Delete, and the whole-mouse backup/restore.
- **On-board memory**: five slots (indicator colours white, red, green, blue, cyan); choosing a profile for a slot writes it into the mouse, where it works without the program. The footer shows the free flash memory.
- Editing the selected profile writes to its slot; a profile that is in no slot is applied through the **temporary layer** (mouse RAM).
- **Link programs**: pick from running programs (or Browse / type `game.exe`). With automatic switching on, open-rzr (running, also in the tray) watches the program in front and writes its profile to the temporary layer — or to a chosen slot — and back for other programs. Windows: always; Linux: X11 sessions (Wayland does not let programs see the active window).
- Importing Razer Synapse profiles is not supported yet: their export format is not documented.

## Macros

The Macros tab has three columns: the library, what can be added, and the steps.

- **Runs in the mouse** — delay, keyboard function (press / down / up, hold time), mouse function (click / down / up), wheel, media key, text (Latin letters, digits, ASCII symbols — typed as US-layout keystrokes), another macro (inserted in place) and loops (unrolled). open-rzr compiles this into the on-board macro, so it works on any computer without the program. The status line shows the steps, the size (at most 4096 bytes) and the total time; switched-off steps (◉/○) stay in the document but are not written.
- **Program actions** — launch a program, file or website, run a command, type any text (any language), pause. The mouse cannot do these, so they are kept separately: the macro written to the mouse ends with a press of a free **F13–F24** key, and open-rzr, while it runs (the tray is enough), performs the actions when it sees that key. Windows: the trigger keys are registered as hot keys (other programs don't receive them). Linux: open-rzr reads the mouse's own key interface (`/dev/input/event*`; the udev rule gives the logged-in user access to the Razer mice' nodes only), so it works on X11 and Wayland; typing text needs `xdotool` (X11) or `wtype` / `ydotool` (Wayland).
- Recording happens in the recording area (no global keyboard hook). The ▾ next to Record chooses real, fixed or no delays and whether mouse buttons are recorded.
- Keys: Ctrl+Z / Ctrl+Y — undo / redo, Ctrl+D — duplicate, Del — delete the selected steps; Shift/Ctrl+click selects several; drag by ⋮⋮ moves them.

## Hypershift

Hypershift is a second layer of bindings: while a button bound to **Hypershift** is held, the other buttons use their Hypershift bindings. On Customize:

- **Make Hypershift button** in a button's menu (or the notice in the Hypershift layer). The button keeps “Hypershift” in both layers and is marked ⇧; in the Hypershift layer it cannot be changed.
- The Hypershift layer shows what is really in the mouse: an empty binding is **Not assigned** (it does not fall back to the standard binding).
- Notices: Hypershift bindings without a Hypershift button (they never work), a Hypershift button with an empty layer.
- **•••** next to the layer switch: copy the standard layer to Hypershift, clear the Hypershift layer.
- Macros and keys a keyboard may not have (F13–F24, numpad) can be bound in either layer (**Other key** in the binding dialog).

## Supported devices

| | Models | What works |
|---|---|---|
| Own module, tested | Razer Naga Pro: cable (`1532:008F`) and HyperSpeed receiver (`1532:0090`) | everything above |
| Naga Pro family, not tested | Razer Naga V2 Pro: cable (`1532:00A7`) and receiver (`1532:00A8`) — the same three side plates and button ids | everything above (experimental) |
| OpenRazer database + on-board check | 68 mice of the current generation (Basilisk V2/V3, DeathAdder V2/V3, Viper Ultimate, Viper V2/V3, Cobra Pro, Naga X, Naga V2 HyperSpeed, Pro Click…) | everything the database confirms, plus **buttons, macros and on-board profiles if the mouse answers them** (experimental) |
| OpenRazer database | older mice (DeathAdder, Mamba, Orochi, Naga 2014…) | DPI, DPI stages, polling rate, battery, sleep, low power mode, lighting — what the database confirms for the model; more through the OpenRazer driver on Linux |

The full list: **Settings → Supported devices** or `open-rzr-cli devices`.

**On-board check.** Current-generation Razer mice use the same button, macro and profile commands as the Naga Pro. At connection open-rzr asks such a mouse with read-only requests (the binding of the left button, the number of profiles, the list of macros); what answers is turned on and marked as experimental, with a notice that suggests a backup first. Buttons are found by scanning: ids that are the same on every Razer mouse (left, right, middle, back, forward, wheel, tilt) have names and factory bindings, the others are shown as “Button 0x..” with what they do now and can be renamed. The Customize drawing shows only the buttons the mouse has; a Naga without plates (Naga X, Left-Handed, V2 HyperSpeed) gets the 12-key side view. A proper module (names, drawing, factory bindings) is a copy of `core/src/devices/naga_pro.rs` — logs from these mice help to write one.

The database is generated from the OpenRazer source by `tools/openrazer-import`, which records only facts (product ids, names, features, maximum DPI). For every model it compares the requests the OpenRazer driver sends with the ones open-rzr sends; a feature counts as “direct” only if they are the same, transaction id included.

## OpenRazer compatibility (Linux)

[OpenRazer](https://openrazer.github.io) is the open Linux driver (`razermouse`) and daemon for Razer devices. Two programs talking to one mouse get each other's replies, so open-rzr can work **through** the OpenRazer driver: DPI, polling, battery, sleep, lighting go to its sysfs files (`/sys/bus/hid/drivers/razermouse/…`), the kernel serialises them with the daemon. Buttons, macros, profiles and the side plate, which the driver does not handle, still go directly.

**Settings → OpenRazer compatibility**:

| Mode | Behaviour |
|---|---|
| Automatic (default) | use the OpenRazer driver when it is attached to the mouse, otherwise talk directly |
| Always through OpenRazer | the same, and a log warning when the driver is not there |
| Off | always directly; stop the daemon (button in the same card or `systemctl --user stop openrazer-daemon`) |

If the daemon runs while open-rzr talks to the mouse directly, the Device tab shows a warning with a **Stop OpenRazer daemon** button. Note that the daemon restores its own saved lighting and DPI when it starts.

### Tray and autostart

open-rzr can start with the system (Windows: the current user's `Run` registry key; Linux: `~/.config/autostart`) and run minimized in the system tray. Closing the window keeps it in the tray; click the tray icon to open it, use **Quit** in the tray menu to exit. A second start just brings the running window to the front.

### Polychromatic

[Polychromatic](https://github.com/polychromatic/polychromatic) (GPL-3.0) is the lighting front-end for OpenRazer on Linux. open-rzr works **alongside** it and never modifies or loads it — it is only started as a separate program:

- if it is installed, **Settings → OpenRazer compatibility** and the tray menu get **Open Polychromatic**;
- while it runs, open-rzr goes through the OpenRazer driver (even in mode “Off”) and leaves the lighting to it: the Lighting tab says so and is locked (**Manage lighting here anyway** unlocks it), profile switching does not touch the LEDs;
- buttons, macros, on-board profiles and program links — which Polychromatic does not have — stay in open-rzr;
- open-rzr does not offer to stop the OpenRazer daemon while Polychromatic needs it.

## Where things are

| | Windows | Linux |
|---|---|---|
| Settings | `%APPDATA%\open-rzr\settings.json` | `~/.config/open-rzr/settings.json` |
| Log | `%APPDATA%\open-rzr\logs\open-rzr.log` | `~/.config/open-rzr/logs/open-rzr.log` |
| Backups | `%APPDATA%\open-rzr\backups\` | `~/.config/open-rzr/backups/` |

The log rotates at 2 MB, 5 old files are kept. Times are UTC.

## Building on Windows

Install the tools once (PowerShell):

```powershell
winget install -e --id Rustlang.Rustup
winget install -e --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Open a new PowerShell window and build:

```powershell
cd path\to\open-rzr
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1             # exe + portable zip
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1 -Installer  # + installer
```

The script runs the tests, builds `open-rzr.exe` (window) and `open-rzr-cli.exe` (console), checks that they start and puts the result in `dist\`. With `-Installer` it installs `tauri-cli` once and builds an NSIS installer. WebView2 is part of Windows 11.

Razer Synapse and its services talk to the mouse at the same time as open-rzr, and the two get each other's replies. open-rzr finds them itself: when Razer software is running, the Device tab shows a warning with **Stop Razer software**. **Settings → Razer Synapse (Windows)** lists the services and programs, stops them (Windows asks for administrator rights) and can also keep the services from starting with Windows; **Start Razer services again** restores exactly what was changed. From the console: `open-rzr-cli razer`, `open-rzr-cli razer stop`, `open-rzr-cli razer stop-autostart`.

### How it works on Windows

```
open-rzr.exe ── WebView2 window (ui/) ⇄ Tauri commands (app/) ── core (openrzr)
                                                                  │ hidapi → hid.dll: HidD_SetFeature / HidD_GetFeature
                                                                  ▼
                                    Windows HID driver (built in, nothing to install) → USB → mouse / HyperSpeed receiver
```

No driver, service or administrator rights are needed: Windows lets any program exchange feature reports with the mouse. Plugging and unplugging is reported by Windows at once (`CM_Register_Notification`), so the window reconnects without polling; on Linux the same comes from kernel uevents. `open-rzr-cli watch` prints these events.

## Building on Linux

```bash
sudo apt install build-essential curl wget file libssl-dev libxdo-dev \
     libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
curl https://sh.rustup.rs -sSf | sh
./packaging/linux/build.sh            # binaries + tar.gz
./packaging/linux/build.sh --bundles  # + .deb and .AppImage
```

Access without root: on first start the program offers to install a udev rule; `open-rzr-cli install-udev` does the same. The `.deb` installs the rule itself. Stop the OpenRazer daemon if it runs.

## Automatic builds on GitHub

Copy `packaging/ci/github-build.yml` to `.github/workflows/build.yml`. GitHub then builds both systems on every push; a tag like `v0.5.0` creates a release with the installer, zip, .deb, AppImage and checksums.

## First start: make a backup

**Buttons → Backup** saves all profiles, both layers and the macros. From the console:

```bash
open-rzr-cli backup naga-original.json --profiles 1,2,3,4,5
open-rzr-cli restore naga-original.json
```

## orzr-cli — razer-cli syntax

`orzr-cli` is a second command line that follows the syntax of [razer-cli](https://github.com/LoLei/razer-cli), so scripts and habits carry over. It talks to the mouse itself (no OpenRazer daemon needed) and works on Windows too. Written from scratch for open-rzr — no razer-cli code.

```
orzr-cli -ls                                   devices and their settings (-l / -ll: with zones and effects)
orzr-cli -e static -c ff0000                   every zone red
orzr-cli -e static spectrum -z logo scroll_wheel -c 00ff00
orzr-cli -e multicolor -c ff0000 00ff00 0000ff -z generic   Naga Pro side panel LEDs
orzr-cli -e breath_dual -c ff0000 0 0 255      hex or three numbers
orzr-cli -b 40            |  -b logo 100 generic 0
orzr-cli --dpi 1600       |  --dpi 1600,800  |  --dpi print
orzr-cli --poll 1000      |  --poll print
orzr-cli --battery print  |  --battery low 15 idle 300
orzr-cli -d "Razer Naga Pro (wired)" ...       or -d SERIAL
orzr-cli -a                                    colour from pywal / X resources
orzr-cli --restore                             apply the last used settings again
orzr-cli -man [TOPIC]  |  orzr-cli help effect
```

Zones: `generic` (whole device; on the Naga Pro the side panel), `logo`, `scroll_wheel`, `left`, `right`, `backlight`, `all`. Effects: `none`, `static`, `spectrum`, `wave`, `breath_single`, `breath_dual`, `breath_random`, `reactive[,1-4]`, `multicolor`; razer-cli effects the mouse lacks (ripple, starlight…) are reported and skipped. Extras: `--mock`, `--debug` (USB log), `--openrazer auto|on|off`. Last used settings: `orzr-cli-settings.txt` in the settings folder.

## Console

```
open-rzr-cli --help
open-rzr-cli list | info | plate
open-rzr-cli devices basilisk                      # supported models
open-rzr-cli openrazer [status|stop]               # OpenRazer driver/daemon
open-rzr-cli --openrazer off info                  # compatibility mode: auto|on|off
open-rzr-cli razer [stop|stop-autostart]           # Windows: Razer Synapse services and programs
open-rzr-cli watch                                 # print plug / unplug events
open-rzr-cli dpi 1600 | stages 800 1600 3200 --active 2 | poll 1000
open-rzr-cli buttons [--hypershift] [--all] [--plate auto|12|6|2|all]
open-rzr-cli bind 0x40 key ctrl+shift+c            # 12-button plate, button 1
open-rzr-cli bind 0x40 default                     # factory binding
open-rzr-cli --profile 2 bind 0x41 mouse back
open-rzr-cli bind 0x42 dpi aim 400                 # sniper while held
open-rzr-cli copy-profile 1 2
open-rzr-cli macro write 3 "ctrl+a, 30ms, ctrl+c"
open-rzr-cli bind 0x43 macro 3 --mode hold
open-rzr-cli light all static 00ff80 | light side custom red green blue | brightness all 40
open-rzr-cli lowbatt 10                            # low power mode below 10 %
open-rzr-cli -v raw 0085 --size 1                  # any command, packets to the console
open-rzr-cli logs 50
```

`--mock` uses the built-in Naga Pro emulator; in the window it is `open-rzr.exe --mock`.

## If something does not work

**Diagnostics → Save report** collects the HID device list, the mouse state, all buttons with raw bytes and the last 800 log lines. Send it along with the problem.

## Translating

The interface strings are in `ui/i18n/en.js` (main) and `ui/i18n/ru.js`. To add a language, copy `en.js`, translate the values and add its `<script>` tag in `ui/index.html`; it appears in Settings automatically. Plural forms use `{ "one": …, "few": …, "many": …, "other": … }`.

## Layout

```
core/              crate openrzr: std only (+ serde for JSON, hidapi on Windows)
  src/protocol.rs     90-byte report, CRC, command codes, error codes
  src/transport/      hidraw (Linux, own FFI), hidapi (Windows), emulator,
                      openrazer.rs — bridge to the OpenRazer driver's sysfs files
  src/device.rs       a connected mouse: opening, feature checks, logged requests
  src/features/       what a mouse can do, one module each:
                      info, dpi, power, lighting, buttons, macros, backup
  src/devices.rs + devices/  what each mouse is (registry, feature flags):
                      naga_pro.rs      — tested module (buttons, plates, factory bindings)
                      openrazer_db.rs  — generated from OpenRazer, 117 models
  src/compat.rs       OpenRazer detection, modes, stopping the daemon
  src/synapse.rs      Windows: Razer Synapse services/programs — find, stop, start again
  src/hotplug.rs      plug/unplug notifications (Windows cfgmgr32, Linux netlink uevent)
  src/bindings.rs, macros.rs, lighting.rs, hidkeys.rs — packet codecs
  src/macro_doc.rs    macro editor documents and their compiler (loops, nested macros, text)
  src/program_actions.rs  program actions of macros: F13–F24 trigger keys, launch / run / type
  src/logs.rs  src/udev.rs  src/bin/cli.rs (open-rzr-cli)
  tests/              emulator tests (cargo test -p openrzr)
app/               Tauri app: commands, settings.json, tray, autostart
ui/                interface: HTML/CSS/JS without bundlers or Node; i18n/ — translations
tools/openrazer-import/   regenerates the device database from an OpenRazer checkout
packaging/         Windows/Linux builds, udev rule, CI
```

Regenerating the device database:

```bash
git clone --depth 1 https://github.com/openrazer/openrazer ../openrazer
cargo run -p openrazer-import -- ../openrazer core/src/devices/openrazer_db.rs
cargo run -p openrzr --bin open-rzr-cli -- install-udev --print > packaging/linux/99-open-rzr.rules
```

## Acknowledgements

open-rzr is built on the open work of others.

**[OpenRazer](https://openrazer.github.io)** ([GitHub](https://github.com/openrazer/openrazer), GPL-2.0) is one of the main projects open-rzr takes its knowledge from:

- the device database: product ids, model names, maximum DPI and the features of every Razer mouse it supports;
- the Razer report format: 90-byte packets, checksum, transaction ids;
- commands for DPI, DPI stages, polling rate, battery, charging, sleep timer and low power mode;
- lighting commands: Chroma effects, the Naga Pro side panel matrix, brightness;
- the sysfs interface of the `razermouse` driver, used by the compatibility mode.

Also:

- **[razerqdhid](https://github.com/geezmolycos/razerqdhid)** (MIT) — button, macro and on-board profile commands;
- **[naga-synapse](https://github.com/Renari/naga-synapse)** — side plate detection command (0x00B9).
- **[razer-cli](https://github.com/LoLei/razer-cli)** (GPL-3.0) — the command line syntax `orzr-cli` follows (no code taken).

### Special thanks to the OpenRazer community

To the developers, maintainers and everyone who, over many years, tested devices, captured and decoded packets and kept Razer hardware usable on Linux. Without that open work there would be no open-rzr.

open-rzr contains no OpenRazer code: only facts about devices and the protocol. The same thanks are shown in the program (**Settings → About**) and in `open-rzr-cli version`.

License: MIT.
