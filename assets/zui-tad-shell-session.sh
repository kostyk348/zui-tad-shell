#!/bin/sh
# ZUI-TAD Shell session — запускается из display manager (GDM/SDDM/LightDM).
# Файл устанавливается в /usr/share/wayland-sessions/zui-tad-shell.desktop
# или /usr/share/xsessions/zui-tad-shell.desktop

# Загружаем переменные окружения пользователя.
[ -f ~/.profile ] && . ~/.profile
[ -f ~/.bash_profile ] && . ~/.bash_profile

# Каталог с конфигом.
export ZUI_CONFIG_DIR="${ZUI_CONFIG_DIR:-$HOME/.config/zui-tad}"
mkdir -p "$ZUI_CONFIG_DIR"

# Хранилище — в XDG_DATA_HOME.
export ZUI_STORE_PATH="${ZUI_STORE_PATH:-${XDG_DATA_HOME:-$HOME/.local/share}/zui-tad/store.sled}"
mkdir -p "$(dirname "$ZUI_STORE_PATH")"

# Запускаем autostart-скрипт пользователя (если есть).
if [ -x "$ZUI_CONFIG_DIR/autostart.sh" ]; then
    "$ZUI_CONFIG_DIR/autostart.sh" &
fi

# Запускаем polkit (если есть в системе).
if command -v /usr/libexec/polkit-gnome-authentication-agent-1 >/dev/null 2>&1; then
    /usr/libexec/polkit-gnome-authentication-agent-1 &
fi

# Запускаем XDG-desktop-portal (для drag-and-drop файлов из файловых менеджеров).
if command -v xdg-desktop-portal >/dev/null 2>&1; then
    xdg-desktop-portal &
fi

# Запускаем основной shell.
exec zui-tad-shell
