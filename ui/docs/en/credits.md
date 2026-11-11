# Acknowledgements

open-rzr is built on the open work of others.

**[OpenRazer](https://openrazer.github.io)** (GPL-2.0) is one of the main projects open-rzr takes its knowledge from:

- the device database: product ids, model names, maximum DPI and the features of every Razer mouse it supports;
- the Razer report format: 90-byte packets, checksum, transaction ids;
- commands for DPI, DPI stages, polling rate, battery, charging, sleep timer and low power mode;
- lighting commands: effects, the Naga Pro side panel, brightness;
- the sysfs interface of the `razermouse` driver, used by the compatibility mode.

Also:

- **[razerqdhid](https://github.com/geezmolycos/razerqdhid)** (MIT) — button, macro and on-board profile commands;
- **[naga-synapse](https://github.com/Renari/naga-synapse)** — side plate detection;
- **[razer-cli](https://github.com/LoLei/razer-cli)** (GPL-3.0) — the command line syntax `orzr-cli` follows (no code taken);
- **[Polychromatic](https://github.com/polychromatic/polychromatic)** (GPL-3.0) — started as a separate program, never modified.

## Special thanks to the OpenRazer community

To the developers, maintainers and everyone who, over many years, tested devices, captured and decoded packets and kept Razer hardware usable on Linux. Without that open work there would be no open-rzr.

open-rzr contains no OpenRazer code — only facts about devices and the protocol. It is an independent project, not affiliated with Razer Inc., and uses no Synapse code.

**License:** MIT.
