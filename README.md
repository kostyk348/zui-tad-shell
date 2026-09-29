# ZUI-TAD Shell — Spatial Desktop Environment

[![CI](https://github.com/kostyk348/zui-tad-shell/actions/workflows/ci.yml/badge.svg)](https://github.com/kostyk348/zui-tad-shell/actions/workflows/ci.yml)
![tests](https://img.shields.io/badge/tests-106%20passing-brightgreen)
![license](https://img.shields.io/badge/license-MIT-blue)
![render](https://img.shields.io/badge/render-CPU%20%C2%B7%2060fps-orange)
![platform](https://img.shields.io/badge/platform-Linux%20(Wayland%20%2B%20X11)-lightgrey)

**EN:** a tiling-free Wayland desktop: an infinite 2D canvas instead of
workspaces, a phosphor-CRT shell (Dead Space RIG × Signalis mood) with a command
palette, and a TAD/BTRON object model underneath. Pure-CPU renderer keeps 60 fps
without a GPU. **RU:** ниже.

Полноценный Wayland-композитор на Rust с ZUI-парадигмой (бесконечный холст + BTRON/TAD модель данных).

**Стек:** Rust 1.80+ · smithay 0.5 (Wayland compositor) · tiny-skia (2D) · ab_glyph (TTF) · sled (embedded DB) · MessagePack.

---

## Что можно посмотреть прямо сейчас

Две вещи работают **без GPU и без Wayland-сессии** (CPU-рендер, softbuffer):

```bash
# 1. ЖИВОЕ окно холста: мышь (drag окон, панорама, зум Mod+колесо),
#    клавиши: ←↑→↓ прыжок · Space home · W overview · M fit · +/-/0 зум
#             1/2/3 тема (Rig / Signalis / Phosphor) · L лаунчер · Esc выход
cargo run --release -p phosphor --bin zui-preview

# 2. Статичный мок оболочки в PNG (в репо: docs/shots/)
cargo run --release -p phosphor --bin shell_shot -- docs/shots/shell.png rig
cargo run --release -p phosphor --bin shell_shot -- docs/shots/shell.png signalis
cargo run --release -p phosphor --bin shell_shot -- docs/shots/shell.png rig clean  # без CRT-эффектов
```

Композитор (настоящие Wayland-клиенты на холсте) — nested-режим:

```bash
ZUI_STORE_PATH=data/store.sled \
cargo run -p compositor --features smithay --bin zui-compositor
# Отдельно, в другом терминале:
XDG_RUNTIME_DIR=/run/user/$UID WAYLAND_DISPLAY=zui-tad-0 alacritty
```

Проба железа (ничего не захватывает): `... --bin zui-compositor -- --probe` —
покажет карты `/dev/dri`, коннекторы и их статус, устройства ввода.

### Что уже умеет композитор (проверено вживую)

* настоящие Wayland-приложения живут на холсте: `alacritty`, `foot`, GTK-приложения;
* **перетаскивание** окна мышью (или за CSD-заголовок — через `move_request` клиента);
* **ресайз за любой из 8 краёв** (9 px зона, размер уходит клиенту через `xdg configure`);
* двойной клик по окну = fit-window; `maximize`/`fullscreen`/`minimize` от клиента и с клавиатуры;
* клавиатурный фокус по клику, системные курсоры ресайза, скрытие курсора по просьбе клиента;
* горячие клавиши холста: `Mod+←↑→↓` прыжок, `Alt+Tab` MRU, `Mod+W` overview, `Mod+M` fit,
  `Mod+±/0` зум, `Mod+1..4` закладки, `Mod+Return` терминал, `Mod+Q` закрыть;
* **оболочка живёт внутри композитора**: панель с телеметрией, миникарта, ЭКГ-виталы,
  строка подсказок, тосты и **командное меню** рисуются GL-слоем поверх реальных окон
  (`MemoryRenderBuffer`). Клик по панели открывает меню, строка меню выполняет действие
  (смена темы видна сразу), `F12`/`Mod+D` — то же с клавиатуры.
  Слой перерисовывается только при изменениях (секунда часов, состояние меню) — в простое
  это один блит текстуры на кадр.

### Превью оболочки (HUD)

`zui-preview` держит **60 fps** (CPU, без GPU) и показывает: фосфорную панель,
миникарту холста, ЭКГ-виталы, луч CRT-развёртки, рамку-видоискатель с прицелом,
тосты и HUD окна под курсором (`app · размер · зум`).

Скриншоты — в `docs/shots/` (кликабельны, лежат в репо): `shell-rig.png`, `shell-signalis.png`,
`shell-rig-clean.png` (без CRT-эффектов), `shell-menu.png`, `shell-help.png`,
`preview-rig.png`, `preview-hud.png`, `preview-menu.png`, `preview-help.png`,
`compositor-real-app.png` (живой alacritty на холсте), `compositor-resize.png`.

---

## FAQ · горячие клавиши

| Клавиша | Что делает |
|---|---|
| `:`, `/`, `p` | **командное меню** (фильтр набором, `↑↓`, `Enter`) |
| `?`, `F1` | справка по клавишам (этот список) |
| `1` `2` `3` | тема: **RIG** (оранжевый сигнал) / **SIGNALIS** (амбер CRT) / **PHOSPHOR** (зелёный) |
| `F2` | HUD вкл/выкл (миникарта, виталы, HUD окна, видоискатель) |
| `F3` | **focus mode** — тихий режим для долгой работы (панель + подсказки, без HUD) |
| `F4` | виталы ЭКГ вкл/выкл |
| `F5` | луч CRT-развёртки вкл/выкл |
| `F6` | качество: **AUTO / RICH / LEAN** (AUTO сам ужимает эффекты, если кадр не влезает) |
| `L` | лаунчер приложений |
| `Space` | home: origin холста, зум 1:1 |
| `W` | overview — показать все окна разом |
| `M` | fit окна под экран (maximize на холсте) |
| `S` | suspend окна (остаётся плейсхолдер, `Enter` вернёт на место) |
| `Tab` | окна по MRU (как `Alt+Tab`) |
| `+` `-` `0` | зум / сброс зума |
| `Mod`+колесо | зум к курсору |
| `← ↑ → ↓` | прыжок к ближайшему окну в направлении |
| ЛКМ | фокус + перенос окна; по пустому холсту — панорама |
| **ЛКМ за край (9px)** | ресайз за любой из 8 краёв, размер уходит клиенту |
| `Mod`+ЛКМ | панорама холста |
| двойной клик | fit окна |
| `Esc` | закрыть окно/меню; на пустом экране — выход (настройки сохраняются) |

### Настройки

`~/.config/zui-tad/shell.toml` — тема, качество, HUD/виталы/развёртка, focus mode.
Файл создаётся сам при выходе; правится руками в любой момент:

```toml
theme = "signalis"      # rig | signalis | phosphor
quality = "auto"        # auto | rich | lean
bg = "hull"             # hull | starfield | crt | blueprint | off | путь к картинке
focus_mode = false
hud = true
vitals = true
beam = true
help_on_start = false
```

### Обои

Фон — процедурный (своя графика, никаких чужих игровых ассетов):

```bash
cargo run --release -p phosphor --bin gen_bg            # 12 обоев в assets/backgrounds
cargo run --release -p phosphor --bin gen_bg -- 2560 1440
ZUI_WALLPAPER=~/Pictures/wp.png cargo run --release -p phosphor --bin zui-preview
```

Виды: `hull` (панельная обшивка корабля), `starfield` (звёздное поле),
`crt` (кинескоп с полутоном и «текстом»), `blueprint` (чертёж). Фон
двигается с параллаксом и кэшируется — кадр почти не дорожает.

### Структура

| Крейт | Роль |
|---|---|
| `crates/canvas-engine` | камера, раскладка холста, Scene/кластеры, взаимодействие (drag/resize/pan) — чистая логика, 46 тестов |
| `crates/phosphor` | оболочка: палитры, CRT-эффекты, панель, OSD, HUD, меню, справка, иконки, текстуры, обои — 60 тестов |
| `crates/compositor` | Wayland-композитор на smithay: реальные окна на холсте, wl_output, drag/resize/maximize |
| `crates/tad-core`, `crates/de-common` | объектная модель RO/VO и общие утилиты DE |
| `legacy/` | прежний стек (softbuffer-шелл, TAD-редакторы) — не собирается, см. `legacy/README.md` |

### Известные ограничения

* вложенный запуск под X11 без рабочего WM: синтетические клавиши (`xdotool key`) до
  окна композитора не доходят — горячие клавиши проверяются в живой сессии или в
  `zui-preview` (там тот же код оболочки);
* белые полосы при ресайзе, которые были видны с `alacritty` (GTK CSD), **не
  воспроизводятся** с `kitty` — это клиентская перерисовка, а не композитор.

### Частые вопросы

**Будет ли 60 fps на моём железе?** Превью — чистый CPU-рендер. На Ryzen 7 7840HS
кадр занимает ~9 мс из 16.6 мс бюджета (запас ~1.8×). На слабом CPU `quality`
сам уйдёт в `LEAN` (сканлайны без зерна/дизеринга) — ничего делать не нужно.

**Почему окна «плавают», а не тайлятся?** Это бесконечный холст: у окна есть
место в мире, а не «слот» на экране. Сцепка окон (кластер) образуется сама,
когда края касаются.

**Что значит suspend?** Закрытое окно оставляет плейсхолдер на своём месте
холста: `Enter`/клик возвращает приложение туда же (сессия не теряет раскладку).

**Как посмотреть настройки качества в рантайме?** В модуле `RIG` справа:
`FRAME 8.8ms`, `MODE AUTO·RICH|RICH|LEAN`.

---

## ⚙️ Установка

### 1. Установить Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source "$HOME/.cargo/env"
rustup default stable   # ВАЖНО: без этого cargo не запустится!
rustc --version          # должно быть >= 1.80
```

### 2. Системные зависимости

**Ubuntu/Debian:**
```bash
sudo apt install -y \
    build-essential pkg-config \
    libwayland-dev libxkbcommon-dev libx11-dev libxrandr-dev \
    libxinerama-dev libxcursor-dev libxi-dev libegl1 libgles2 \
    libudev-dev libseat-dev libdrm-dev libgbm-dev libinput-dev \
    fonts-dejavu-core wmctrl xdotool
```

**Fedora:**
```bash
sudo dnf install -y gcc pkg-config wayland-devel libxkbcommon-devel \
    libX11-devel libXrandr-devel libXinerama-devel libXcursor-devel \
    libXi-devel mesa-libGLES-devel systemd-devel libseat-devel \
    libdrm-devel gbm-devel libinput-devel dejavu-fonts-common wmctrl xdotool
```

**Arch:**
```bash
sudo pacman -S base-devel pkgconf wayland libxkbcommon libx11 libxrandr \
    libxinerama libxcursor libxi mesa systemd libseat libdrm libinput \
    ttf-dejavu wmctrl xdotool
```

### 3. Сборка

```bash
git clone <repo> zui-tad-shell
cd zui-tad-shell
cargo build --release
```

### 4. Установка как DE

```bash
sudo ./install.sh
```

Ставит:
- `/usr/local/bin/zui-tad-shell` — основной бинарник
- `/usr/local/bin/zui-tad-shell-session` — wrapper для DM
- `/usr/share/wayland-sessions/zui-tad-shell.desktop` — для GDM/SDDM
- `/usr/share/xsessions/zui-tad-shell.desktop` — для LightDM
- `/usr/share/applications/zui-tad-shell.desktop` — в меню приложений
- `/usr/local/share/zui-tad-shell/fonts/` — шрифты

### 5. Запуск

**Способ 1: Из логин-экрана (рекомендуется)**
1. Выйти из текущей сессии
2. На экране входа выбрать "ZUI-TAD Shell"
3. Ввести пароль → запустится как настоящая DE

**Способ 2: Внутри текущей DE (для теста)**
```bash
zui-tad-shell
```

**Способ 3: Wayland-композитор (smithay 0.5)**
```bash
cargo build --release --features compositor/smithay
ZUI_STORE_PATH=data/store.sled ./target/release/zui-tad-shell --compositor --embedded
# Запуск клиента: WAYLAND_DISPLAY=... alacritty
```

### Встроенные TAD-приложения

| Клавиша | Приложение |
|---------|------------|
| `Ctrl+,` | Settings (настройки DE) |
| `Ctrl+F` | Files (файловый менеджер) |
| `Ctrl+Shift+C` | Calculator |

На рабочем столе демо-сцены три VO закреплены вверху. Calculator принимает цифры и `+-*/=C`. Files: стрелки, Enter — открыть, Backspace — вверх. Settings: Space/Enter — переключить опцию.

---

## Возможности DE

| Компонент | Что делает |
|-----------|------------|
| **Wayland compositor** | Полноценный композитор на smithay 0.5 (как sway/Hyprland) |
| **Top panel** | Встроенная или waybar/polybar (через wlr_layer_shell) |
| **App launcher** | Встроенный или rofi/wofi/fuzzel/walker/tofi/dmenu |
| **Workspace manager** | 9 независимых рабочих столов, каждый со своей камерой |
| **TAD-документы** | Встроенные редакторы текста/вектора/таблиц с undo/redo |
| **Порталы** | Анимированные переходы между документами по клику |
| **WM-интеграция** | Запуск терминала/браузера/редактора как VO на холсте |
| **Window embedding** | Захват surface как текстуры → встраивание в VO |
| **Multi-monitor** | Поддержка нескольких мониторов + hotplug через udev |
| **XWayland** | Для X11-приложений |
| **Session lock** | ext_session_lock_v1 protocol (swaylock/hyprlock) |
| **Foreign toplevel** | waybar видит список окон, переключение, закрытие |
| **Импорт файлов** | Drag-and-drop `.txt/.md/.png/.jpg` → RO + VO |
| **Autostart** | `~/.config/zui-tad/autostart.sh` |
| **Конфиг TOML** | `~/.config/zui-tad/config.toml` |

---

## Горячие клавиши

### DE
| Клавиша | Действие |
|---------|----------|
| `Ctrl+1..9` | Workspace switch |
| `Ctrl+Shift+1..9` | Переместить окно на workspace |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next/prev workspace |
| `Ctrl+Space` | App launcher (встроенный или rofi/wofi) |
| `Ctrl+Alt+L` | Lock screen (swaylock/hyprlock) |
| `Ctrl+Alt+Del` | Выход |
| `Ctrl+Q` | Выход |

### Запуск приложений
| Клавиша | Приложение |
|---------|------------|
| `Ctrl+T` | Терминал |
| `Ctrl+B` | Браузер |
| `Ctrl+Shift+E` | Редактор кода |
| `Ctrl+F` | Файловый менеджер |
| `Ctrl+Shift+C` | Калькулятор |

Кастомизация: `ZUI_TERMINAL=alacritty ZUI_BROWSER=firefox ZUI_EDITOR=code ZUI_FILES=thunar ZUI_CALC=gnome-calculator`

### Навигация по холсту
| Клавиша | Действие |
|---------|----------|
| `Space` | Сброс камеры |
| `+`/`-` | Зум |
| `0` | Зум 1:1 |
| `F` | Перелёт к фокусному VO |
| `Tab` / `Shift+Tab` | След./пред. сегмент |
| `Esc` | Снять фокус |

### Редакторы
| Клавиша | Действие |
|---------|----------|
| `Ctrl+Z`/`Ctrl+Y` | Undo/Redo |
| `Ctrl+S` | Сохранить в sled |
| `Ctrl+E` | Экспорт в `.md` |
| `Ctrl+N` | Новый текстовый документ |
| `Ctrl+Shift+N` | Новый mindmap |
| `Ctrl+Alt+N` | Новая таблица |
| `Ctrl+D` | Дублировать VO |
| `Delete` | Удалить VO |

### Мышь
| Действие | Эффект |
|----------|--------|
| Колесо | Зум к курсору |
| Средняя + drag | Pan |
| ЛКМ по VO | Фокус |
| ЛКМ по порталу | Полёт камеры |
| ЛКМ + drag | Перетаскивание VO |
| Drag файла | Импорт `.txt/.md/.png/.jpg` |

### Система
| Клавиша | Действие |
|---------|----------|
| `F1` / `?` | Окно справки |
| `F12` | Скриншот в `data/screenshot.png` |

---

## Конфигурация

`~/.config/zui-tad/config.toml`:

```toml
[launcher]
backend = "rofi"            # builtin | rofi | wofi | fuzzel | walker | tofi | dmenu
rofi_theme = "gruvbox-dark"

[panel]
backend = "waybar"          # builtin | waybar | polybar | yambar | none
waybar_config = "~/.config/waybar/config"
waybar_style = "~/.config/waybar/style.css"

[wallpaper]
backend = "swaybg"          # solid | swaybg | hyprpaper | wpaperd
color = "#1a1b20"
path = "~/Pictures/wallpaper.jpg"
mode = "fill"

[locker]
backend = "swaylock"        # none | swaylock | hyprlock | waylock

[notifications]
backend = "mako"            # none | mako | dunst | fnott

[clipboard]
backend = "wl-clipboard"    # none | wl-clipboard

[idle]
backend = "swayidle"        # none | swayidle | hypridle
timeout = 300
lock_cmd = "swaylock -f"

[session]
polkit = true
xdg_portal = true
autostart_script = true
```

Пример в `assets/config.example.toml`.

---

## Autostart

`~/.config/zui-tad/autostart.sh`:

```bash
#!/bin/sh
waybar &
nm-applet &
blueman-applet &
/usr/libexec/polkit-gnome-authentication-agent-1 &
```

```bash
chmod +x ~/.config/zui-tad/autostart.sh
```

---

## Архитектура

```
zui-tad-shell/
├── crates/
│   ├── tad-core/         # TAD-формат, Real/Virtual Objects, GraphStore (sled)
│   ├── canvas-engine/    # Камера, frustum culling, semantic LOD
│   ├── editor-core/      # Фокус, редакторы, порталы, undo/redo
│   ├── skia-renderer/    # CPU-рендеринг: tiny-skia + ab_glyph
│   ├── de-common/        # Workspaces, launcher (без Wayland)
│   ├── compositor/       # Опциональный smithay backend (`--features compositor/smithay`)
│   └── shell-app/        # Основной ZUI DE (winit + softbuffer)
├── assets/
│   ├── fonts/                    # DejaVuSans
│   ├── zui-tad-shell.desktop     # session entry для DM
│   ├── zui-tad-shell-session.sh  # session wrapper
│   └── config.example.toml
├── docs/
│   └── UPGRADE_TO_SMITHAY_0.5.md
├── install.sh
└── data/                         # sled-хранилище + screenshots (создаётся)
```

---

## Устранение ошибок

| Ошибка | Решение |
|--------|---------|
| `rustup could not choose a version of cargo` | `rustup default stable` |
| `Permission denied (os error 13)` | Запускайте из каталога проекта |
| `Package libudev was not found` | `sudo apt install libudev-dev pkg-config` |
| `Package libdrm was not found` | `sudo apt install libdrm-dev` |
| `Package libinput was not found` | `sudo apt install libinput-dev` |
| `Package libseat was not found` | `sudo apt install libseat-dev` |
| `Package gbm was not found` | `sudo apt install libgbm-dev` |
| `font DejaVuSans.ttf not found` | `sudo apt install fonts-dejavu-core` |
| Сессия не появляется в GDM | Проверьте `ls /usr/share/wayland-sessions/zui-tad-shell.desktop` |
| Не запускается из TTY | `loginctl` должен показывать вашу сессию как `seat0` |
| Приложение не запускается через Ctrl+T | `ZUI_TERMINAL=alacritty zui-tad-shell` |

---

## Что есть vs чего нет

**Есть (полноценная DE):**
- ✅ Wayland-композитор (smithay 0.5 + DRM/KMS)
- ✅ XDG shell (настоящие приложения как Wayland-клиенты)
- ✅ Layer shell (waybar/top panel)
- ✅ Foreign toplevel management (waybar taskbar)
- ✅ Session lock protocol (swaylock/hyprlock)
- ✅ Multi-output + hotplug (udev)
- ✅ XWayland (X11 приложения)
- ✅ libinput (клавиатура/мышь через evdev)
- ✅ Бесконечный холст с pan/zoom, LOD
- ✅ TAD-документы с undo/redo
- ✅ 9 workspaces
- ✅ App launcher (встроенный + rofi/wofi/fuzzel/walker/tofi/dmenu)
- ✅ Top panel (встроенная + waybar/polybar/yambar)
- ✅ Обои (swaybg/hyprpaper/wpaperd)
- ✅ Уведомления (mako/dunst/fnott)
- ✅ Буфер обмена (wl-clipboard + cliphist)
- ✅ Idle management (swayidle/hypridle)
- ✅ Polkit agent
- ✅ XDG Desktop Portal
- ✅ Session .desktop для GDM/SDDM/LightDM
- ✅ Autostart скрипт
- ✅ Конфиг TOML

**Чего нет (требует доработки):**
- ❌ DRM/TTY backend композитора (только winit+GLES прототип)
- ❌ Полный window embedding в ZUI-холст (Wayland clients рендерятся отдельно)
- ❌ Settings daemon (используйте swaybg/waybar/wl-clipboard)
- ❌ System tray из коробки (через waybar module)
- ❌ Accessibility (нужно реализовать отдельно)

## Лицензия
MIT OR Apache-2.0.
