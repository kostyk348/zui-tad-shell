#!/bin/sh
# Проверка ZUI-TAD ПРЯМО СЕЙЧАС, внутри текущей сессии (ничего не забирает).
# Что делает: поднимает композитор окном, запускает Wayland-клиент и X11-клиент,
# снимает скриншот холста изнутри, печатает лог и держит окно открытым.
# Выход: Ctrl+C здесь (всё аккуратно завершится).
set -u
cd "$(dirname "$0")/.."
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
OUT=/tmp/zui-test; mkdir -p "$OUT"
BIN=./target/release/zui-compositor
# Авто-пересборка: бинарь старше исходников — значит он устарел (грабля «check != build»).
if [ ! -x "$BIN" ] || [ -n "$(find crates -name '*.rs' -newer "$BIN" -print -quit 2>/dev/null)" ]; then
  echo "== бинарь устарел или отсутствует — собираю =="
  cargo build --release -p compositor --features smithay --bin zui-compositor || exit 1
fi

if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
  echo "нет ни DISPLAY, ни WAYLAND_DISPLAY — вложенный режим (winit) тут не заведётся."
  echo "запусти этот скрипт ИЗ ГРАФИЧЕСКОЙ СЕССИИ (в окне терминала), а из TTY используй ./scripts/test-tty.sh"
  exit 2
fi
ZUI_STORE_PATH="$PWD/data/store.sled" setsid "$BIN" >"$OUT/compositor.log" 2>&1 </dev/null &
COMP=$!
trap 'kill $COMP ${K:-} ${X:-} ${XT:-} 2>/dev/null; echo; echo "остановлено"; exit 0' INT TERM
sleep 5
SOCK=$(grep -oE 'WAYLAND_DISPLAY=[^ ]+' "$OUT/compositor.log" | head -1 | cut -d= -f2)
echo "композитор: pid=$COMP, сокет=${SOCK:-?}"

if [ -n "${SOCK:-}" ]; then
  setsid env WAYLAND_DISPLAY="$SOCK" kitty --title ZUI-KITTY >"$OUT/kitty.log" 2>&1 </dev/null & K=$!
  sleep 3
  if command -v xwayland-satellite >/dev/null 2>&1; then
    setsid xwayland-satellite :0 >"$OUT/xwayland.log" 2>&1 </dev/null & X=$!
    sleep 3
    DISPLAY=:0 xterm -geometry 60x14 -e sh -c 'echo "X11 ON CANVAS"; sleep 120' >"$OUT/xterm.log" 2>&1 </dev/null & XT=$!
    sleep 3
  else
    echo "xwayland-satellite не найден — X11-клиент пропущен"
  fi
  if command -v grim >/dev/null 2>&1; then
    WAYLAND_DISPLAY="$SOCK" grim "$OUT/canvas.png" 2>"$OUT/grim.log" \
      && echo "скриншот холста: $OUT/canvas.png ($(wc -c <"$OUT/canvas.png") байт)"
  fi
fi

echo
echo "--- что сделать руками (окно композитора) ---"
echo "  клик по верхней панели  -> командное меню (темы, fit, suspend)"
echo "  Alt+Tab / Mod+стрелки    -> переключение и прыжки по окнам"
echo "  мышь: тащить окно, за край (9px) — ресайз, по пустому — панорама"
echo "  Mod+колесо               -> зум к курсору"
echo "--- лог ---"
grep -E "toplevel|loop it|session|adopted" "$OUT/compositor.log" | tail -6
echo
echo "Ctrl+C здесь — остановить всё."
wait $COMP
