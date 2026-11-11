# Supported mice and Bluetooth

| Support | Models | What works |
|---|---|---|
| **Tested module** | Razer Naga Pro: cable (`1532:008F`) and HyperSpeed receiver (`1532:0090`) | everything |
| **Naga Pro family**, not tested | Naga V2 Pro (`00A7`, `00A8`); Naga Pro over Bluetooth (`0092`) | everything, experimental |
| **OpenRazer database + on-board check** | 68 current mice: Basilisk V2/V3, DeathAdder V2/V3, Viper Ultimate, Viper V2/V3, Cobra Pro, Naga X, Naga V2 HyperSpeed, Pro Click… | what the database confirms, plus buttons, macros and profiles if the mouse answers them (experimental) |
| **OpenRazer database** | older mice | DPI, stages, polling, battery, sleep, lighting — what the database confirms |

The full list: **Settings → Supported devices**, or `open-rzr-cli devices`.

## On-board check

Current Razer mice use the same button, macro and profile commands as the Naga Pro. At connection open-rzr asks such a mouse with **read-only** requests (the binding of the left button, the number of profiles, the list of macros). What answers is turned on and marked *experimental*, with a notice that suggests a backup first.

Buttons are found by scanning. Those that are the same on every Razer mouse (left, right, middle, back, forward, wheel) have names and factory bindings; the others appear as *Button 0x..* with what they do now and can be renamed. The drawing shows only the buttons the mouse has; a Naga without plates (Naga X, Left-Handed, V2 HyperSpeed) gets the 12-key side view.

## Bluetooth

Over Bluetooth LE the Naga Pro is a different device for the system (product id `0092`) with six HID collections (mouse, keyboard, media keys, system keys and two more). They carry input only: none of them takes the setting commands that work over the cable. The settings channel of the Naga Pro over Bluetooth is a separate vendor GATT service (`52401523-…`), which open-rzr can only explore so far — see below.

- If one answers the setting commands, the mouse is configured as over the cable.
- If none does, open-rzr says so: connect the mouse by cable or through the HyperSpeed receiver to change settings. The settings stay in the mouse and work over Bluetooth too.

**Diagnostics → Wireless** shows, for a mouse on Bluetooth, its HID collections with report lengths and ids, its GATT services and characteristics (readable values are read — battery, device information) and can **probe** the collections with the read-only “firmware version” request. Everything goes to the log too, so *Save report* carries it. **Diagnostics → All HID devices** shows every collection with its usage page (★ = vendor-defined) and the connection type — useful when a mouse is not found.

**Razer GATT channel** (in the same dialog, Windows): subscribes to the notifying characteristics of the vendor service, writes a request to its writable one — only listen, the read-only firmware request, or bytes you enter — and shows every notification with its time. Use it to help work out the Bluetooth protocol: listen while pressing buttons or changing DPI, then send the report.

## Another mouse

If your mouse is listed as *experimental* and something works wrongly, send the report from **Diagnostics → Save report**. A proper module (names, drawing, factory bindings) for a new model starts as a copy of `crates/rzr-devices/src/naga_pro.rs`.

## Device files

A model that is not in the list — or one whose entry is not quite right — can be described in a file, without recompiling. Put one `*.toml` (or `*.json`) per device into the **devices** folder: **Diagnostics → Device files** shows the folder, reloads it and lists errors. On Windows it is `%APPDATA%\open-rzr\devices`, on Linux `~/.config/open-rzr/devices`.

A file starts from the OpenRazer database entry with the same product id (or from the model given as `base_pid`, for example `0x008F` for a Naga Pro sibling) and changes only what it states:

```toml
pid = 0x00AB                    # product id (Device Manager / lsusb: 1532:00ab)
name = "Razer Example Mouse"
# base_pid = 0x008F             # start from a known model instead of the OpenRazer entry
transaction_id = 0x1F
wireless = false
battery = false
max_dpi = 26000
dpi_stages = 5
features = ["firmware", "serial", "dpi", "dpi_stages", "polling", "lighting"]

[[zones]]
id = "logo"                     # scroll, logo, side, left, right, backlight
led = 0x04
effects = ["off", "static", "spectrum", "breath"]

[[buttons]]
id = 0x60
name = "DPI down"
```

Features: firmware, serial, dpi, dpi_stages, polling, battery, charging, idle, low_battery, lighting, buttons, macros, profiles, side_plate. Effects: off, static, spectrum, wave, breath, reactive, custom. Unknown keys and names are reported as warnings. A tested module (Naga Pro) is not replaced unless the file says `override = true`. Models from files are marked *your file* and treated as experimental. The command line shows the same with `open-rzr-cli device-files`.
