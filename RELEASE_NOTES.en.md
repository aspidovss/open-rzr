## open-rzr 0.8.1.6

### Fixed
- **Slots the mouse does not have are no longer counted.** open-rzr remembered profiles in slots the mouse no longer has (here: 2 and 5 while the mouse had only slot 1), showed “In slot 1, 2, 5” and wrote the profile there too — over Bluetooth the mouse even answers “OK” to such writes. Now, when the mouse is connected, slots it does not report are forgotten (noted in the log), and writing a profile into a missing slot is refused: “slot N is not in the mouse — create it first”.
- **Diagnostics and Help no longer scroll the whole window.** On the log view only the log scrolls between the buttons above and the command row below; in Help the contents and the text scroll on their own. Works in both window styles and at any window size; the Wireless view and the other pages scroll as before.

### Write self-test
- **The test button is chosen from the buttons of your mouse**: side button 12 on a Naga Pro with the 12-button plate, otherwise another side key, Back/Forward or the middle button; it can be changed before the start. Left and right button, the wheel and the profile button are never used. The hints name the chosen button; the final check only requires that it no longer types 7.

### Documentation
- README: release and build badges, Bluetooth support in the device table, first start, the `ui` and `packaging` folders, author and contact; the software profile is described as “in its own slot” (README and guide).

### Checked on the mouse (Naga Pro, Bluetooth)
- Button bindings from the program, the Hypershift layer (layer byte `01`), the sleep timer, and keeping a binding after the mouse is switched off and on — all work.

## open-rzr 0.8.1.5

### Bluetooth: settings can be written
- **Button bindings are changed over Bluetooth** (Naga Pro, Windows): both layers, in the slots the mouse has (`08 04 <slot> <button>`, data `<slot> <button> <layer> <action>`). Profiles, the button editor, “In slot…”, “Write to mouse” and program buttons work as on the cable.
- There is no temporary layer over Bluetooth (profile `00` mirrors the active one and is read only): changes to a profile go into its slot at once, and a profile that is not in a slot cannot be used over Bluetooth — the program says so.
- **The sleep timer** is changed over Bluetooth (Power). Low-battery level, DPI and lighting are read only so far; writing a profile over Bluetooth skips DPI stages and lighting (noted in the log).
- The mouse's slots are read over Bluetooth (`03 80`); they cannot be created or deleted there, so those buttons are hidden.
- All bindings are read in one request (`08 83 <slot> 00`) instead of 127, checked against a single read of the last button.

### Write self-test, version 3 (5 stages)
- **Stage 4 — Hypershift layer**: the digit 7 is written to the Hypershift layer (layer byte `01`), read back to check that only that layer changed, and put back.
- **Stage 5 — does it stay in the mouse** (if you agree): the digit 7 is written, you switch the mouse off and on, the program waits for it to reconnect (up to ~40 s), reads the button and asks you to press it, then puts the binding back.

### Linux
- **Program buttons send keys, mouse buttons and media keys** through a virtual input device (`/dev/uinput`), held as long as the mouse button, as on Windows.
- The udev rule now also opens `/dev/uinput` and the mouse's keys over Bluetooth. If the rule of an older version is installed, the program offers to update it once. Settings has a new card, “Linux access (udev rule)”, with its state and an Install / Update button.

### Not tested on the mouse yet
- Writing over Bluetooth from the program (not the self-test), the Hypershift layer and keeping bindings after switching off — check with self-test version 3.
- Linux: `/dev/uinput` and the new udev rule are covered by tests only.

## open-rzr 0.8.1.4

### Bluetooth
- **Writing a button over Bluetooth works** (self-test of 0.8.1.3 on a Naga Pro): `08 04 <profile> <button>` with the data `<profile> <button> 00 <action>` into the active profile 1 changes the binding at once, reads back changed and is put back the same way. The profile list over Bluetooth is `03 80` (`01 02`), the active one `03 82` (`01`), numbered as over USB.
- The self-test now sets the digit 7 instead of the letter F: a digit does not depend on the keyboard layout.

## open-rzr 0.8.1.3

### Fixed
- **The side view shows the side plate again**: in 0.8.1.2 choosing a plate (2, 6 or 12 buttons) drew the top view.

### Bluetooth
- **Write self-test, version 2** (Diagnostics → Wireless). The earlier variants wrote the button into profile 0, the read-only mirror of the active profile. Now it works the way Synapse writes Basilisk mice over Bluetooth (as described by the OpenSnek project): stage 2 reads the profile list (`03 80`), the active profile (`03 82`) and side button 12 in profiles 0–5; stage 3 writes it (`08 04 <profile> <button>`, data `<profile> <button> <layer> <action, 7 bytes>`) into the active stored profile, the live profile 1 and both, asks you to press the button after each and puts it back.

## open-rzr 0.8.1.2

### New
- **Creating slots in the mouse's memory**: in “On-board memory”, a slot the mouse does not have gets a Create button, existing slots (except the first) a ✕ to delete them. The result is checked against the mouse's profile list; if the mouse only switched to that slot instead, the program switches it back and says the slot could not be created. The software profile window offers “Create slot 5” when it is missing.
- **All buttons beside the picture (as in Synapse)**: Settings → Window style → “Customize: the list button opens all buttons beside the picture (like Synapse)”. The ▢ button under the mouse opens a list of all buttons with their bindings and a filter (all / top / side plate); a click opens the binding. Off by default — the button works as before.
- Removed the layers icon left of the profile list and the ▶ mark in the “In slot …” button (the active slot is still shown next to it).

## open-rzr 0.8.1.1

### New
- **Versions of four numbers**: major . new features . fixes . optional (intermediate updates, support of particular devices). The program, log, CLI, installer and file names show `0.8.1.1`; Cargo keeps `0.8.1+1`.
- **Settings → Updates → “Also tell me about optional updates”** — a separate switch, off by default. Without it the update check tells only about versions where one of the first three numbers changed; a manual check mentions an optional update with a hint.

## open-rzr 0.8.1

### New
- **Active slot**: next to the On-board memory button the program shows which slot the mouse works by now (switched with the profile button on the mouse); the “In slot …” button and the memory list mark it. Refreshed every 3 seconds.
- **Software profile in a slot of your choice**: chosen from the slots the mouse has (5 if it has it). Slots the mouse does not have yet are labelled.
- A failed raw command in Diagnostics no longer logs an “unhandled rejection”.

## open-rzr 0.8.0

### New
- **Software profile (slot 5)**: profile menu → “Software profile in slot 5…” makes a copy of the current profile and writes it to slot 5 of the mouse; the other slots are not changed. In such a profile a button can be set to “⚡ Run by open-rzr”: an F13–F24 key is written to the mouse once, and the program performs the actions — a key or shortcut, a mouse button, a media key (held with the button), typing text, launching a program or website, a command, pauses. Changing these actions writes nothing to the mouse, and they run over Bluetooth too.
- **Trigger keys (Windows)**: a low-level keyboard hook instead of system hot keys — both press and release are seen, so an action can be held. Trigger keys of macros work as before.

### Bluetooth (0.6.15–0.7.3)
- Connecting the mouse over Bluetooth through the Razer GATT service: firmware, serial number, battery, DPI and stages, sleep timer, brightness, button bindings — read only.
- Diagnostics → Wireless — a separate view: Razer commands over Bluetooth, command scan, raw GATT channel, write self-test.

### Installing and first start (0.6.11–0.6.14)
- User agreement in the installer and at first start; optional update check on GitHub; uninstaller; “Remove open-rzr from this computer” for the portable version and Linux.
