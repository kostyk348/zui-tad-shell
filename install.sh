#!/bin/bash
# install.sh — системная установка ZUI-TAD Shell как DE
# Запускать: sudo ./install.sh

set -e

PREFIX="${PREFIX:-/usr/local}"
BINDIR="$PREFIX/bin"
SESSIONDIR_WAYLAND="/usr/share/wayland-sessions"
SESSIONDIR_X="/usr/share/xsessions"
DATADIR="$PREFIX/share/zui-tad-shell"
APPDIR="/usr/share/applications"

echo "==> Сборка release..."
cargo build --release

echo "==> Установка бинарников в $BINDIR/"
install -Dm755 target/release/zui-tad-shell     "$BINDIR/zui-tad-shell"
install -Dm755 target/release/render_screenshot "$BINDIR/zui-tad-shell-screenshot"
install -Dm755 target/release/init_demo         "$BINDIR/zui-tad-shell-init-demo"

echo "==> Установка данных в $DATADIR/"
install -Dm644 README.md       "$DATADIR/README.md"
install -Dm644 LICENSE         "$DATADIR/LICENSE" 2>/dev/null || true
cp -r assets/fonts             "$DATADIR/fonts/"

echo "==> Установка сессии для дисплейного менеджера..."
install -Dm755 assets/zui-tad-shell-session.sh "$BINDIR/zui-tad-shell-session"
install -Dm644 assets/zui-tad-shell.desktop    "$SESSIONDIR_WAYLAND/zui-tad-shell.desktop"
install -Dm644 assets/zui-tad-shell.desktop    "$SESSIONDIR_X/zui-tad-shell.desktop"

echo "==> Установка .desktop файла для launcher..."
install -Dm644 assets/zui-tad-shell.desktop    "$APPDIR/zui-tad-shell.desktop"

echo "==> Обновление кеша шрифтов..."
fc-cache -f >/dev/null 2>&1 || true

echo ""
echo "================================================"
echo " Готово! ZUI-TAD Shell установлен как DE."
echo "================================================"
echo ""
echo "Способы запуска:"
echo ""
echo "  1. Из логин-экрана (GDM/SDDM/LightDM):"
echo "     Выйдите из текущей сессии, выберите 'ZUI-TAD Shell'"
echo ""
echo "  2. Как отдельное приложение внутри текущей DE:"
echo "     zui-tad-shell"
echo ""
echo "  3. С явным запуском композитора:"
echo "     zui-tad-shell --compositor"
echo ""
echo "Горячие клавиши DE:"
echo "  Ctrl+1-9         Workspace switch"
echo "  Ctrl+Shift+1-9   Move window to workspace"
echo "  Ctrl+Space       App launcher"
echo "  Ctrl+T/B/Shift+E/F — Terminal/Browser/Editor/Files"
echo "  Ctrl+Z/Y         Undo/Redo"
echo "  F1               Help"
echo "  F12              Screenshot"
echo ""
echo "Для deинсталляции:"
echo "  rm $BINDIR/zui-tad-shell $BINDIR/zui-tad-shell-session $BINDIR/zui-tad-shell-*"
echo "  rm -r $DATADIR"
echo "  rm $SESSIONDIR_WAYLAND/zui-tad-shell.desktop $SESSIONDIR_X/zui-tad-shell.desktop"
echo "  rm $APPDIR/zui-tad-shell.desktop"
