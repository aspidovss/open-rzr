# Windows and Razer Synapse

open-rzr needs **no driver, service or administrator rights**: it uses the HID driver built into Windows. The window runs in WebView2 (part of Windows 11).

## Razer Synapse

Synapse and its services (Razer Elevation Service, Game Manager, RazerAppEngine…) talk to the mouse at the same time as open-rzr, and the two get each other's replies. open-rzr checks for them every few seconds:

- the notice at the top lists what is running and offers **Stop Razer software**;
- **Settings → Razer Synapse** lists the services and programs, stops them (Windows asks for administrator rights) and can keep the services from starting with Windows;
- **Start Razer services again** restores exactly what was changed.

From the console: `open-rzr-cli razer`, `open-rzr-cli razer stop`, `open-rzr-cli razer stop-autostart`.

## Drivers

**Diagnostics → Drivers** lists the driver of every Razer device: provider, version, driver and filters.

- **Standard Microsoft drivers only** — fine, nothing else is needed.
- **Razer driver packages or filters** (left by Synapse) are marked. open-rzr does not need them. If the mouse does not answer, remove them: *Device Manager → the device → Uninstall device → Attempt to remove the driver*, then reconnect the mouse. open-rzr never removes anything by itself.

Razer drivers cannot be shipped with open-rzr: they are closed and licensed by Razer — and they are not needed.

## Plugging in

Windows reports plugging and unplugging at once, so the window reconnects by itself. `open-rzr-cli watch` prints these events.
