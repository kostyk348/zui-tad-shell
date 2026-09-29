# Ассеты: происхождение и лицензии

## Обои (`assets/backgrounds/`)

Все обои **сгенерированы кодом этого репозитория**:

```bash
cargo run --release -p phosphor --bin gen_bg          # 1920x1080
cargo run --release -p phosphor --bin gen_bg -- 2560 1440
```

Генераторы: `crates/phosphor/src/bg.rs` (hull / starfield / crt / blueprint).
Лицензия — та же, что у проекта (MIT). Файлы можно удалять: они
пересоздаются одной командой.

**Почему здесь нет «текстур из Dead Space / Signalis»**: это чужая
интеллектуальная собственность, её нельзя распространять в открытом
репозитории. Визуальный язык (оранжевый RIG-монохром, амбер CRT, сканлайны,
сегментные шкалы) воспроизведён собственным кодом — идеи не охраняются,
ассеты охраняются.

Свою картинку можно подставить без пересборки:

```bash
ZUI_WALLPAPER=~/Pictures/wp.png cargo run --release -p phosphor --bin zui-preview
# или в ~/.config/zui-tad/shell.toml:  bg = "/home/me/Pictures/wp.png"
```

## Шрифты (`assets/fonts/`)

`DejaVuSans.ttf`, `DejaVuSans-Bold.ttf` — шрифты DejaVu, лицензия Bitstream
Vera Fonts Copyright (свободная, разрешает использование и распространение).
Шрифты вшиты в бинарь через `include_bytes!`, поэтому лежат в репозитории.

## Скриншоты (`docs/shots/`)

Сняты из самого проекта (`shell_shot`, `zui-preview`, композитор), лицензия MIT.
