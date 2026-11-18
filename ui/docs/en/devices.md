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

Over Bluetooth LE the Naga Pro is a different device for the system (product id `0092`) with six HID collections (mouse, keyboard, media keys, system keys and two more). They carry input only: none of them takes the setting commands that work over the cable. The settings channel of the Naga Pro over Bluetooth is a separate vendor GATT service (`52401523-…`), which open-rzr uses instead — see below.

- If one answers the setting commands, the mouse is configured as over the cable.
- If none does (the Naga Pro), open-rzr connects through the Razer GATT service instead (Windows). It reads firmware, serial number, battery, DPI and stages, sleep timer, lighting brightness, the mouse's slots and their button bindings, and it **changes button bindings** (both layers, in the slots the mouse has) and the **sleep timer**. The other settings are read-only over Bluetooth, slots cannot be created or deleted, and there is no temporary layer: a profile must be in a slot to be used, and its changes go into the slot at once. Connect the mouse by cable or through the HyperSpeed receiver for the rest — the settings stay in the mouse and work over Bluetooth too.
- There is no polling rate over Bluetooth: the reports follow the Bluetooth connection interval (the Naga Pro asks for 7.5–11.25 ms, about 90–130 Hz), so the polling setting is hidden. The 125/500/1000 Hz setting applies to the cable and the receiver.

**Diagnostics → Wireless** (a view of the Diagnostics page, next to the log) shows, for a mouse on Bluetooth, its HID collections with report lengths and ids, its GATT services and characteristics (readable values are read — battery, device information) and can **probe** the collections with the read-only “firmware version” request. Everything goes to the log too, so *Save report* carries it. **Diagnostics → All HID devices** shows every collection with its usage page (★ = vendor-defined) and the connection type — useful when a mouse is not found.

### Razer commands over Bluetooth (Windows)

The vendor service takes its own commands — not the 90-byte USB report:

```text
…1524  write   request [number, length, 00, 00, class, command, sub1, sub2] (+ data of a "set" in a second write)
…1525  notify  reply   [number, length, 00, 00, 00, 00, 00, status], then `length` bytes in 20-byte notifications
```

The status is the same as over USB (`02` OK, `03` failed, `05` not supported); numbers are little-endian; class and command numbers are their own. Known so far on the Naga Pro: `00 81` firmware, `05 81` (sub `00 01`) battery, `0b 84` (sub `01 00`) DPI stages, `10 83` lighting effect, `03 80` the list of slots and `03 82` the active one (numbered as over USB), `08 83` (sub `<slot> 00`) all bindings of a slot, `08 84` (sub `<slot> <button>`) one binding (slot `00` = the active one, read only).

Writes: a button binding is `08 04` (sub `<slot> <button>`) with the data `<slot> <button> <layer> <action, 7 bytes>` (layer `00` normal, `01` Hypershift) — the format the OpenSnek project found on Basilisk mice; the sleep timer is `05 04` with the seconds. **Diagnostics → Wireless → Write self-test** checks both on your mouse, and whether a binding stays after the mouse is switched off and on.

- **Read what is known** — asks those four and shows what they mean.
- **Class / Command / Sub → Read** — one request.
- **Try every command** — every class × command × sub of the ranges given (for example classes `00-1f`, commands `80-9f`). Only “get” commands (command `80`–`FF`) are sent: nothing in the mouse changes. “Not supported” replies are hidden unless you want them.

**Razer GATT channel** below it: subscribes to the notifying characteristics, writes raw bytes you enter (or only listens) and shows every notification with its time — for working out the rest of the protocol. Send the report afterwards.

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
