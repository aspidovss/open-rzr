# Linux

## Access to the mouse (udev)

Without a rule, only root can talk to the mouse. On first start open-rzr offers to install `/etc/udev/rules.d/99-open-rzr.rules` (asks for your password once); `open-rzr-cli install-udev` does the same, and the `.deb` installs it itself. The rule gives the logged-in user access to:

- the control interface (`hidraw`) of the Razer mice open-rzr knows, by cable, receiver and Bluetooth;
- the mouse's own key events (`/dev/input/event*`) — for the F13–F24 triggers of macro program actions.

Reconnect the mouse after installing the rule.

## OpenRazer

[OpenRazer](https://openrazer.github.io) is the open Linux driver (`razermouse`) and daemon for Razer devices. Two programs talking to one mouse get each other's replies, so open-rzr can work **through** the OpenRazer driver: DPI, polling, battery, sleep and lighting go to its sysfs files and the kernel serialises them with the daemon. Buttons, macros, profiles and the side plate, which the driver does not handle, still go directly.

**Settings → OpenRazer compatibility:**

| Mode | Behaviour |
|---|---|
| Automatic (default) | through the OpenRazer driver when it is attached to the mouse, otherwise directly |
| Always through OpenRazer | the same, with a log warning when the driver is not there |
| Off | always directly; stop the daemon (`systemctl --user stop openrazer-daemon`) |

The daemon restores its own saved lighting and DPI when it starts.

## Polychromatic

[Polychromatic](https://github.com/polychromatic/polychromatic) (GPL-3.0) is the lighting front-end for OpenRazer. open-rzr works **alongside** it and only starts it as a separate program:

- *Open Polychromatic* appears in Settings and in the tray menu when it is installed;
- while it runs, open-rzr goes through the OpenRazer driver and leaves the lighting to it (the Lighting page is locked; *Manage lighting here anyway* unlocks it);
- buttons, macros, on-board profiles and program links stay in open-rzr.

## Tray and autostart

open-rzr can start with the system (`~/.config/autostart`) and run in the tray. Closing the window keeps it in the tray; **Quit** in the tray menu exits.
