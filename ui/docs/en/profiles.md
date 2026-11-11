# Profiles and writing to the mouse

A **profile** is a full set of settings: the bindings of both layers (standard and Hypershift), DPI stages, polling rate, lighting, the macros it uses and the programs it is linked to. Profiles are kept on this computer (`%APPDATA%\open-rzr\profiles\` on Windows, `~/.config/open-rzr/profiles/` on Linux) and can be written into the mouse.

## On-board slots

The Naga Pro holds **five profiles** in its own memory. Each slot has an indicator colour: 1 white, 2 red, 3 green, 4 blue, 5 cyan. The profile button under the mouse switches between them — without any program running.

The **memory** button next to the profile shows the five slots and what each holds, and the free flash memory. Choosing a profile for a slot writes it there.

## Write to mouse

Changes apply **at once** through the **temporary layer** (the mouse's RAM), so you can try them right away. They stay in the mouse after a restart only when written into a slot. The button next to the profile shows the state:

| Button | Meaning |
|---|---|
| **● Write to mouse** (green) | the profile was changed: the mouse already behaves the new way, but its slot still holds the old version. Click to write. |
| **✓ In slot N** | the slot holds exactly this version |
| **Write to mouse ▾** | the profile is in no slot yet: choose one of the five (what each holds now is shown) |

Only the changed bindings are written, which saves flash memory.

**Settings → Profiles → Write changes straight into the profile's on-board slot** — every change goes into flash at once, without the button (the behaviour of older versions).

## The ••• menu

- **Add** — with factory bindings, as a copy of the current profile, or read from a slot of the mouse.
- **Import / Export** — `*.openrzr.json` files (export goes to *Downloads*). Synapse profiles cannot be imported: their format is not documented.
- **Link programs** — see below.
- **Rename, Duplicate, Delete.**
- **Read from the mouse** — take the profile back from its slot (when it was changed elsewhere).
- **Reset** — factory bindings for every button.
- **Backup / Restore** — the whole mouse (all slots, both layers, macros) to a file and back.

## Linked programs

*••• → Link programs*: pick from the running programs or type `game.exe`. With **automatic switching** on, open-rzr (running, also in the tray) watches the program in front and applies its profile — through the temporary layer or into a chosen slot — and goes back for other programs.

- Windows: always works.
- Linux: X11 sessions only (Wayland does not let programs see the active window).

## First connection

On the first connection the mouse's slots are read into the library as `<COMPUTER>-Default`, `Profile 2`, … so nothing that was in the mouse is lost.
