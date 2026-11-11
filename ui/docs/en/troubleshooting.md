# Troubleshooting

## The mouse is not found or does not answer

1. **Razer Synapse or OpenRazer** is talking to the mouse — see the notice at the top; stop it ([Windows](windows.md), [Linux](linux.md)).
2. **Linux:** the udev rule is missing — *Settings* offers to install it; reconnect the mouse afterwards.
3. **Bluetooth:** settings may be unavailable over Bluetooth — connect by cable or receiver ([details](devices.md)).
4. **Diagnostics → All HID devices** shows what the system sees. **Diagnostics → Drivers** shows Razer drivers left by Synapse.
5. Press **⟳** next to the device list.

## A button does something unexpected

- Check the layer: **Standard** or **Hypershift**.
- *Find by pressing…* shows which button you pressed.
- *Default* in the binding dialog returns the factory binding; *••• → Reset* does it for every button.

## Changes are lost after unplugging

The profile was not written into a slot: the button next to it says **● Write to mouse** — click it ([Profiles](profiles.md)).

## Send a report

**Diagnostics → Save report** collects the HID device list, the mouse state, all buttons with their raw bytes and the last 800 log lines. Nothing is sent anywhere by open-rzr — attach the file to your message or GitHub issue.

## Where things are

| | Windows | Linux |
|---|---|---|
| Settings | `%APPDATA%\open-rzr\settings.json` | `~/.config/open-rzr/settings.json` |
| Profiles | `%APPDATA%\open-rzr\profiles\` | `~/.config/open-rzr/profiles/` |
| Log | `%APPDATA%\open-rzr\logs\open-rzr.log` | `~/.config/open-rzr/logs/open-rzr.log` |
| Backups | `%APPDATA%\open-rzr\backups\` | `~/.config/open-rzr/backups/` |

The log rotates at 2 MB, five old files are kept; times are UTC.
