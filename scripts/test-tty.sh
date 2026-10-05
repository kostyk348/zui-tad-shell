#!/bin/sh
# Проверка TTY-пути. ЗАПУСКАТЬ ИЗ TTY (Ctrl+Alt+F3), а не из-под X.
# Рендер ограничен 30 секундами, чтобы консоль гарантированно вернулась.
cd "$(dirname "$0")/.."
BIN=./target/release/zui-compositor
# Авто-пересборка: бинарь старше исходников — значит он устарел (грабля «check != build»).
if [ ! -x "$BIN" ] || [ -n "$(find crates -name '*.rs' -newer "$BIN" -print -quit 2>/dev/null)" ]; then
  echo "== бинарь устарел или отсутствует — собираю =="
  cargo build --release -p compositor --features smithay --bin zui-compositor || exit 1
fi

echo "=== 1. безопасная диагностика (ничего не захватывает) ==="
"$BIN" --check || true
echo
"$BIN" --drm || true

echo
echo "=== 2. рендер на 30 секунд (заберёт консоль, потом сам вернёт) ==="
echo "    ожидаю строку: TTY-сессия: /dev/dri/cardN режим WxH@R — рендер пошёл"
LOG=/tmp/zui-drm-render.log
timeout 30 "$BIN" --drm-render 2>&1 | tee "$LOG"
code=${PIPESTATUS[0]:-${?}}
echo "вышли (код $code). Полный вывод: $LOG"
if [ "$code" != "0" ] && [ "$code" != "124" ]; then
  echo "ОШИБКА — пришли мне содержимое $LOG"
  echo "(частые причины: X не отпустил DRM-master -> переключись на другой VT;"
  echo " бинарь устарел -> cargo build --release -p compositor --features smithay --bin zui-compositor)"
fi
if [ "$code" = "124" ]; then echo "таймаут 30 с — значит рендер шёл всё это время (это успех)."; fi

echo
echo "=== 3. вернуться в X: Ctrl+Alt+F7 (или где у тебя X) ==="
