# Сборка и участие

## Windows

Один раз установите (PowerShell):

```powershell
winget install -e --id Rustlang.Rustup
winget install -e --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Сборка (из папки проекта):

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1             # exe + портативный zip
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1 -Installer  # + setup.exe
```

## Linux

```bash
sudo apt install build-essential curl wget file libssl-dev libxdo-dev \
     libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
curl https://sh.rustup.rs -sSf | sh
./packaging/linux/build.sh            # программы + tar.gz
./packaging/linux/build.sh --bundles  # + .deb и .AppImage
```

Оба скрипта прогоняют тесты ядра, собирают `open-rzr`, `open-rzr-cli` и `orzr-cli` и проверяют консольные программы на эмуляторе. Результат — в `dist/`.

## Автоматическая сборка на GitHub

Сценарий лежит в `.github/workflows/build.yml` (копия — `packaging/ci/github-build.yml`). При каждом push GitHub собирает обе системы, прогоняет тесты и clippy и проверяет окно в браузере.

## Выпуск релиза

1. Укажите версию в `Cargo.toml` и `app/tauri.conf.json`. Версия из четырёх чисел — крупные изменения . новые функции . исправления . необязательные; четвёртое Cargo пишет как метаданные сборки: `0.8.1+5` — это **0.8.1.5**, `0.9.0` — **0.9.0**.
2. Добавьте раздел `## open-rzr 0.8.1.5` в начало `RELEASE_NOTES.en.md` (по-английски) и `RELEASE_NOTES.md` (по-русски).
3. Сделайте commit и отправьте тег с номером версии: `git tag v0.8.1.5 && git push origin main v0.8.1.5`.

После этого GitHub собирает всё и публикует релиз **open-rzr 0.8.1.5** с установщиком, zip, .deb, AppImage, tar.gz и контрольными суммами и помечает его последним. Текст релиза собирает `packaging/ci/release-notes.py` из обоих файлов: все разделы после предыдущего тега, сначала английский, ниже свёрнутый русский (`python3 packaging/ci/release-notes.py v0.8.1.5` покажет его у вас). Тег, который не совпадает с `Cargo.toml`, останавливает релиз: проверка обновлений во всех установленных копиях спрашивает у GitHub последний релиз, и ошибочный тег вроде `v1.8.1.1` предлагался бы всем как новая версия.

## Тесты окна

`tools/uitest` запускает интерфейс в браузере против имитации Rust-части — мышь не нужна: `cd tools/uitest && npm install && npx playwright install chromium && npm test`. Подробнее — `tools/uitest/README.md`.

## Структура

```
crates/rzr-protocol/  90-байтовый отчёт: сборка, разбор, контрольная сумма, коды команд (no_std, без зависимостей)
crates/rzr-hal/       аппаратный слой: транспорты (hidraw, hidapi, эмулятор, мост к OpenRazer), канал
                      запросов (срок ответа, повтор при BUSY, спящая беспроводная мышь), тип ошибок, журнал
crates/rzr-devices/   база устройств: проверенные модули (naga_pro.rs, generic.rs), OpenRazer (openrazer_db.rs),
                      файлы описаний (external.rs, *.toml / *.json), кодек действий кнопок
crates/rzr-core/      библиотека `openrzr`: трейты возможностей (caps.rs), функции, компилятор макросов,
                      действия программы, драйверы, hotplug, диагностика Bluetooth GATT
crates/rzr-cli/       open-rzr-cli и совместимый с razer-cli orzr-cli
app/       приложение Tauri: main.rs (окно, трей, запуск), commands/ (по модулю на страницу),
           settings.json (store.rs), библиотека профилей (library.rs), автозапуск
ui/        интерфейс (HTML/CSS/JS без сборщиков); i18n/ — переводы; docs/ — это руководство
tools/openrazer-import/   пересоздаёт базу устройств из исходников OpenRazer
tools/uitest/             тесты окна в браузере (имитация Rust-части)
packaging/ сборка для Windows и Linux, правило udev, CI
```

## Перевод

Строки интерфейса: `ui/i18n/en.js` (основной) и `ui/i18n/ru.js`. Чтобы добавить язык, скопируйте `en.js`, переведите значения и добавьте его `<script>` в `ui/index.html`. Руководство: `ui/docs/en/*.md` → `ui/docs/<язык>/*.md`.

## База устройств

```bash
git clone --depth 1 https://github.com/openrazer/openrazer ../openrazer
cargo run -p openrazer-import -- ../openrazer crates/rzr-devices/src/openrazer_db.rs
cargo run -p rzr-cli --bin open-rzr-cli -- install-udev --print > packaging/linux/99-open-rzr.rules
```
