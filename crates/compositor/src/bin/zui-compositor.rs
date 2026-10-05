//! Standalone launcher for the ZUI-TAD canvas compositor.
//!
//! Запуск (nested, поверх текущей графической сессии):
//!   ZUI_STORE_PATH=data/store.sled \
//!   cargo run -p compositor --features smithay --bin zui-compositor
//!
//! Проба железа (ничего не захватывает, просто докладывает):
//!   cargo run -p compositor --features smithay --bin zui-compositor -- --probe
//!
//! `--probe` смотрит /dev/dri и /sys/class/drm: какие карты есть, какие
//! коннекторы подключены, какой драйвер. Это честная проверка DRM-слоя
//! БЕЗ захвата вывода (захват = отдельный бэкенд, см. docs/ROADMAP в README).

use anyhow::Result;
use parking_lot::Mutex;
use std::sync::Arc;
use tad_core::GraphStore;

fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--probe") {
        probe();
        return Ok(());
    }
    if std::env::args().any(|a| a == "--check") {
        check();
        return Ok(());
    }
    if std::env::args().any(|a| a == "--drm-render") {
        // ЗАБИРАЕТ КОНСОЛЬ: только из TTY. Требует seat/VT.
        #[cfg(feature = "smithay")]
        {
            let path = std::env::var("ZUI_STORE_PATH").unwrap_or_else(|_| "data/store.sled".into());
            let _ = std::fs::create_dir_all("data");
            let store = Arc::new(Mutex::new(GraphStore::open(&path)?));
            compositor::drm::run_drm_session(store)?;
        }
        #[cfg(not(feature = "smithay"))]
        println!("соберите с --features smithay");
        return Ok(());
    }
    if std::env::args().any(|a| a == "--drm") {
        // Слой 0 TTY-бэкенда: сессия + карта + план вывода.
        #[cfg(feature = "smithay")]
        {
            compositor::drm::probe_and_plan()?;
        }
        #[cfg(not(feature = "smithay"))]
        println!("соберите с --features smithay");
        return Ok(());
    }

    let path = std::env::var("ZUI_STORE_PATH").unwrap_or_else(|_| "data/store.sled".into());
    let _ = std::fs::create_dir_all("data");
    let store = Arc::new(Mutex::new(GraphStore::open(&path)?));
    compositor::run_compositor(store, false)
}

fn probe() {
    println!("== ZUI-TAD DRM probe ==");
    println!(
        "session: XDG_SESSION_TYPE={:?} DISPLAY={:?} WAYLAND_DISPLAY={:?} XDG_RUNTIME_DIR={:?}",
        std::env::var("XDG_SESSION_TYPE").ok(),
        std::env::var("DISPLAY").ok(),
        std::env::var("WAYLAND_DISPLAY").ok(),
        std::env::var("XDG_RUNTIME_DIR").ok()
    );

    // 1. Карты.
    let mut cards = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev/dri") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("card") || name.starts_with("renderD") {
                cards.push(name);
            }
        }
    }
    cards.sort();
    println!("dri: {:?}", cards);

    // 2. Коннекторы и их статус.
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        let mut rows: Vec<(String, String, String)> = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let status = std::fs::read_to_string(format!("/sys/class/drm/{name}/status"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if status.is_empty() {
                continue;
            }
            let enabled = std::fs::read_to_string(format!("/sys/class/drm/{name}/enabled"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "-".into());
            rows.push((name, status, enabled));
        }
        rows.sort();
        println!("connectors ({} total):", rows.len());
        for (name, status, enabled) in rows {
            let mark = if status == "connected" { "●" } else { "○" };
            println!("  {mark} {name:<22} {status:<12} enabled={enabled}");
        }
    } else {
        println!("connectors: /sys/class/drm недоступен");
    }

    // 3. Мышь/клавиатура (libinput-слой).
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        let ev: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("event") || n.starts_with("mouse") || n.starts_with("kbd"))
            .collect();
        println!("input: {:?}", ev);
    }

    println!();
    println!("ВЫВОД: nested-бэкенд (winit+GLES) можно запускать прямо сейчас.");
    println!("DRM/TTY-бэкенд требует активного seat'а: запускать из TTY ЛИБО через");
    println!("logind с переключением VT. Из сессии другого типа захват вывода — небезопасен.");
}

/// `--check`: что готово, а что нет — для запуска ZUI-TAD как НАСТОЯЩЕЙ сессии.
/// Ничего не захватывает: только смотрит окружение.
fn check() {
    println!("== ZUI-TAD: проверка готовности ==");
    let mut ok = 0;
    let mut warn = 0;
    let mut fail = 0;

    let mut line = |state: &str, what: &str| {
        match state {
            "ok" => ok += 1,
            "warn" => warn += 1,
            _ => fail += 1,
        }
        let mark = match state {
            "ok" => "\u{2713}",
            "warn" => "!",
            _ => "\u{2717}",
        };
        println!("  {mark} {what}");
    };

    // 1. Окружение: nested или настоящая сессия
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let display = std::env::var("DISPLAY").unwrap_or_default();
    let wayland = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    if wayland.is_empty() && display.is_empty() {
        line("ok", "нет вложенной сессии — можно занять TTY (DRM)");
    } else {
        line(
            "warn",
            &format!(
                "мы внутри сессии (SESSION_TYPE={session_type:?}, DISPLAY={display:?}) — DRM/TTY отсюда не занять,                  используйте nested-режим или выйдите в TTY"
            ),
        );
    }

    // 2. Устройства DRM
    let mut cards = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/dev/dri") {
        for e in dir.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("card") {
                cards.push(n);
            }
        }
    }
    cards.sort();
    if cards.is_empty() {
        line("fail", "нет /dev/dri/card* — DRM/TTY-бэкенд невозможен");
    } else {
        line("ok", &format!("DRM-карты: {cards:?}"));
    }

    // 3. Подключённые коннекторы
    let mut connected = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/sys/class/drm") {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Ok(st) = std::fs::read_to_string(format!("/sys/class/drm/{name}/status")) {
                if st.trim() == "connected" {
                    connected.push(name);
                }
            }
        }
    }
    if connected.is_empty() {
        line(
            "fail",
            "нет подключённых коннекторов (проверьте кабель/панель)",
        );
    } else {
        line("ok", &format!("подключено: {connected:?}"));
    }

    // 4. Устройства ввода
    let mut inputs = 0;
    if let Ok(dir) = std::fs::read_dir("/dev/input") {
        inputs = dir
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
            .count();
    }
    if inputs == 0 {
        line("fail", "нет /dev/input/event* — нечем управлять");
    } else {
        line("ok", &format!("устройств ввода: {inputs}"));
    }

    // 5. Seat / права
    if let Ok(groups) = std::process::Command::new("id").arg("-nG").output() {
        let g = String::from_utf8_lossy(&groups.stdout);
        if g.contains("input") || g.contains("video") {
            line(
                "ok",
                "пользователь в группах input/video (нужно для TTY-запуска)",
            );
        } else {
            line(
                "warn",
                "нет групп input/video — на TTY могут не открыться устройства (или используйте seatd/logind)",
            );
        }
    }

    // 5б. X11-приложения
    let xwl = std::process::Command::new("sh")
        .arg("-c")
        .arg("command -v xwayland-satellite")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if xwl {
        line(
            "ok",
            "xwayland-satellite есть — X11-приложения будут работать",
        );
    } else {
        line(
            "warn",
            "нет xwayland-satellite — только Wayland-клиенты (сборка: github.com/Supreeme/xwayland-satellite)",
        );
    }

    // 6. Конфиг и сессия
    let cfg = phosphor::config::ShellConfig::path();
    line(
        "ok",
        &format!(
            "конфиг оболочки: {} ({})",
            cfg.display(),
            if cfg.exists() {
                "есть"
            } else {
                "по умолчанию"
            }
        ),
    );
    let sess = std::path::PathBuf::from("data/session.json");
    line(
        "ok",
        &format!(
            "сессия холста: {} ({})",
            sess.display(),
            if sess.exists() {
                "будет восстановлена"
            } else {
                "чистый холст"
            }
        ),
    );

    println!();
    println!("итог: {ok} ok, {warn} предупреждений, {fail} проблем");
    if fail == 0 && warn == 0 {
        println!("готово к запуску как сессия: выберите ZUI-TAD в DM или запустите из TTY.");
    } else if fail == 0 {
        println!(
            "настоящая сессия: {warn} пункт(ов) стоит посмотреть; nested-режим работает всегда."
        );
    } else {
        println!("есть блокеры — см. выше; nested-режим (winit) всё равно рабочий.");
    }
}
