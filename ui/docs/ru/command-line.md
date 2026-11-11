# Командная строка

Вместе с open-rzr идут две консольные программы. Обе обращаются к мыши сами (служба не нужна) и работают в Windows и Linux.

## open-rzr-cli

```
open-rzr-cli --help
open-rzr-cli bind --help                           справка по одной команде
open-rzr-cli device-files                          ваши файлы описаний устройств
open-rzr-cli list | info | plate
open-rzr-cli devices basilisk                      поддерживаемые модели
open-rzr-cli dpi 1600 | stages 800 1600 3200 --active 2 | poll 1000
open-rzr-cli buttons [--hypershift] [--all] [--plate auto|12|6|2|all]
open-rzr-cli bind 0x40 key ctrl+shift+c            панель на 12 кнопок, кнопка 1
open-rzr-cli bind 0x40 default                     заводское назначение
open-rzr-cli --profile 2 bind 0x41 mouse back
open-rzr-cli bind 0x42 dpi aim 400                 снайпер, пока зажата
open-rzr-cli copy-profile 1 2
open-rzr-cli macro write 3 "ctrl+a, 30ms, ctrl+c"
open-rzr-cli bind 0x43 macro 3 --mode hold
open-rzr-cli light all static 00ff80 | brightness all 40
open-rzr-cli backup naga.json --profiles 1,2,3,4,5 | restore naga.json
open-rzr-cli openrazer [status|stop]               Linux: драйвер и служба OpenRazer
open-rzr-cli razer [stop|stop-autostart]           Windows: Razer Synapse
open-rzr-cli install-udev                          Linux: правило доступа
open-rzr-cli watch                                 события подключения / отключения
open-rzr-cli -v raw 0085 --size 1                  любая команда с выводом пакетов
open-rzr-cli logs 50
```

`--mock` — встроенный эмулятор Naga Pro (для окна — `open-rzr --mock`).

## orzr-cli — синтаксис razer-cli

`orzr-cli` повторяет синтаксис [razer-cli](https://github.com/LoLei/razer-cli), так что скрипты переносятся (написана с нуля, без кода razer-cli).

```
orzr-cli -ls                                    устройства и их настройки
orzr-cli -e static -c ff0000                    все зоны красным
orzr-cli -e static spectrum -z logo scroll_wheel -c 00ff00
orzr-cli -e multicolor -c ff0000 00ff00 0000ff -z generic
orzr-cli -b 40   |  --dpi 1600  |  --poll 1000  |  --battery print
orzr-cli -a                                     цвет из pywal / X resources
orzr-cli --restore                              повторить последние настройки
orzr-cli -man [ТЕМА]
```

Зоны: `generic` (у Naga Pro — боковая панель), `logo`, `scroll_wheel`, `left`, `right`, `backlight`, `all`. Эффекты: `none`, `static`, `spectrum`, `wave`, `breath_single`, `breath_dual`, `breath_random`, `reactive[,1-4]`, `multicolor`.
