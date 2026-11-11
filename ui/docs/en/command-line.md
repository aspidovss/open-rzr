# Command line

Two console programs come with open-rzr. Both talk to the mouse themselves (no daemon needed) and work on Windows and Linux.

## open-rzr-cli

```
open-rzr-cli --help
open-rzr-cli bind --help                           help for one command
open-rzr-cli device-files                          your device description files
open-rzr-cli list | info | plate
open-rzr-cli devices basilisk                      supported models
open-rzr-cli dpi 1600 | stages 800 1600 3200 --active 2 | poll 1000
open-rzr-cli buttons [--hypershift] [--all] [--plate auto|12|6|2|all]
open-rzr-cli bind 0x40 key ctrl+shift+c            12-button plate, button 1
open-rzr-cli bind 0x40 default                     factory binding
open-rzr-cli --profile 2 bind 0x41 mouse back
open-rzr-cli bind 0x42 dpi aim 400                 sniper while held
open-rzr-cli copy-profile 1 2
open-rzr-cli macro write 3 "ctrl+a, 30ms, ctrl+c"
open-rzr-cli bind 0x43 macro 3 --mode hold
open-rzr-cli light all static 00ff80 | brightness all 40
open-rzr-cli backup naga.json --profiles 1,2,3,4,5 | restore naga.json
open-rzr-cli openrazer [status|stop]               Linux: OpenRazer driver / daemon
open-rzr-cli razer [stop|stop-autostart]           Windows: Razer Synapse
open-rzr-cli install-udev                          Linux: access rule
open-rzr-cli watch                                 plug / unplug events
open-rzr-cli -v raw 0085 --size 1                  any command, packets shown
open-rzr-cli logs 50
```

`--mock` uses the built-in Naga Pro emulator (also `open-rzr --mock` for the window).

## orzr-cli — razer-cli syntax

`orzr-cli` follows the syntax of [razer-cli](https://github.com/LoLei/razer-cli), so scripts carry over (written from scratch, no razer-cli code).

```
orzr-cli -ls                                    devices and their settings
orzr-cli -e static -c ff0000                    every zone red
orzr-cli -e static spectrum -z logo scroll_wheel -c 00ff00
orzr-cli -e multicolor -c ff0000 00ff00 0000ff -z generic
orzr-cli -b 40   |  --dpi 1600  |  --poll 1000  |  --battery print
orzr-cli -a                                     colour from pywal / X resources
orzr-cli --restore                              apply the last used settings again
orzr-cli -man [TOPIC]
```

Zones: `generic` (on the Naga Pro the side panel), `logo`, `scroll_wheel`, `left`, `right`, `backlight`, `all`. Effects: `none`, `static`, `spectrum`, `wave`, `breath_single`, `breath_dual`, `breath_random`, `reactive[,1-4]`, `multicolor`.
