#!/bin/sh
# Установка ZUI-TAD: бинари, сессия для DM, ассеты.
# Запуск: sudo ./install.sh   (или ./install.sh --user для ~/.local)
set -e
cd "$(dirname "$0")"

if [ "$1" = "--user" ]; then
    BIN="$HOME/.local/bin"; SHARE="$HOME/.local/share"
else
    BIN="/usr/local/bin"; SHARE="/usr/local/share"
fi

echo "== сборка =="
cargo build --release -p phosphor --bins
cargo build --release -p compositor --features smithay --bin zui-compositor

echo "== бинари -> $BIN =="
mkdir -p "$BIN"
install -m755 target/release/zui-compositor "$BIN/zui-compositor"
install -m755 target/release/zui-preview "$BIN/zui-tad-preview"
install -m755 target/release/shell_shot "$BIN/zui-tad-shot"
install -m755 target/release/gen_bg "$BIN/zui-tad-genbg"
install -m755 assets/zui-tad-session.sh "$BIN/zui-tad-session"

echo "== ассеты -> $SHARE/zui-tad =="
mkdir -p "$SHARE/zui-tad/backgrounds"
install -m644 assets/backgrounds/*.png "$SHARE/zui-tad/backgrounds/"

echo "== сессии для DM =="
if [ "$1" = "--user" ]; then
    mkdir -p "$HOME/.local/share/wayland-sessions" "$HOME/.local/share/xsessions"
    install -m644 assets/zui-tad.desktop "$HOME/.local/share/wayland-sessions/zui-tad.desktop"
    install -m644 assets/zui-tad.desktop "$HOME/.local/share/xsessions/zui-tad.desktop"
else
    install -Dm644 assets/zui-tad.desktop /usr/share/wayland-sessions/zui-tad.desktop
    install -Dm644 assets/zui-tad.desktop /usr/share/xsessions/zui-tad.desktop
fi

echo
echo "готово. Проверка готовности:  zui-compositor --check"
echo "Nested-режим сразу:           zui-compositor"
echo "Живое превью оболочки:        zui-tad-preview"
echo "Сессия: выберите ZUI-TAD на экране входа (или запустите из TTY)."
