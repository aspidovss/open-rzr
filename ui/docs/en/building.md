# Building and contributing

## Windows

Install once (PowerShell):

```powershell
winget install -e --id Rustlang.Rustup
winget install -e --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Build (from the project folder):

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1             # exe + portable zip
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1 -Installer  # + setup.exe
```

## Linux

```bash
sudo apt install build-essential curl wget file libssl-dev libxdo-dev \
     libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
curl https://sh.rustup.rs -sSf | sh
./packaging/linux/build.sh            # binaries + tar.gz
./packaging/linux/build.sh --bundles  # + .deb and .AppImage
```

Both scripts run the core tests, build `open-rzr`, `open-rzr-cli` and `orzr-cli`, and check the console programs on the emulator. The result is in `dist/`.

## Automatic builds on GitHub

The workflow is in `.github/workflows/build.yml` (a copy is kept in `packaging/ci/github-build.yml`). On every push GitHub builds both systems, runs the tests and clippy, and tests the window in a browser; a tag like `v0.5.0` creates a release with the installer, zip, .deb, AppImage and checksums.

## Window tests

`tools/uitest` runs the interface in a browser against a mock of the Rust side — no mouse needed: `cd tools/uitest && npm install && npx playwright install chromium && npm test`. See `tools/uitest/README.md`.

## Layout

```
crates/rzr-protocol/  the 90-byte report: build, parse, checksum, command codes (no_std, no dependencies)
crates/rzr-hal/       hardware layer: transports (hidraw, hidapi, emulator, OpenRazer bridge), request
                      channel (deadline, BUSY retries, sleeping wireless mouse), error type, local log
crates/rzr-devices/   device database: tested modules (naga_pro.rs, generic.rs), OpenRazer (openrazer_db.rs),
                      description files (external.rs, *.toml / *.json), button action codec
crates/rzr-core/      library `openrzr`: capability traits (caps.rs), features, macro compiler,
                      program actions, drivers, hotplug, Bluetooth GATT diagnostics
crates/rzr-cli/       open-rzr-cli and the razer-cli compatible orzr-cli
app/       Tauri app: main.rs (window, tray, start), commands/ (one module per page),
           settings.json (store.rs), profile library (library.rs), autostart
ui/        interface (HTML/CSS/JS, no bundler); i18n/ — translations; docs/ — this guide
tools/openrazer-import/   regenerates the device database from an OpenRazer checkout
tools/uitest/             window tests in a browser (mock of the Rust side)
packaging/ Windows and Linux builds, udev rule, CI
```

## Translating

Interface strings: `ui/i18n/en.js` (main) and `ui/i18n/ru.js`. To add a language, copy `en.js`, translate the values and add its `<script>` tag in `ui/index.html`. The guide: `ui/docs/en/*.md` → `ui/docs/<lang>/*.md`.

## Device database

```bash
git clone --depth 1 https://github.com/openrazer/openrazer ../openrazer
cargo run -p openrazer-import -- ../openrazer crates/rzr-devices/src/openrazer_db.rs
cargo run -p rzr-cli --bin open-rzr-cli -- install-udev --print > packaging/linux/99-open-rzr.rules
```
