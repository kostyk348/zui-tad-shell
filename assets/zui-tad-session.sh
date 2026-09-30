#!/bin/sh
# Обёртка сессии: ZUI-TAD как настоящий рабочий стол (DM или TTY).
# Логи — в $XDG_STATE_HOME/zui-tad.log (или ~/.local/state).
set -e
state="${XDG_STATE_HOME:-$HOME/.local/state}/zui-tad"
mkdir -p "$state"
export XDG_CURRENT_DESKTOP=zui-tad
export XDG_SESSION_DESKTOP=zui-tad
export XDG_SESSION_TYPE=wayland
# store/session холста — рядом с состоянием, а не в cwd
export ZUI_STORE_PATH="${ZUI_STORE_PATH:-$state/store.sled}"
export ZUI_SESSION_PATH="${ZUI_SESSION_PATH:-$state/session.json}"

# Автозапуск: свои команды — в этот файл (по одной на строку)
autostart="${XDG_CONFIG_HOME:-$HOME/.config}/zui-tad/autostart.sh"
if [ -x "$autostart" ]; then
    "$autostart" >>"$state/autostart.log" 2>&1 &
fi

exec /usr/local/bin/zui-compositor "$@" >>"$state/zui-tad.log" 2>&1
