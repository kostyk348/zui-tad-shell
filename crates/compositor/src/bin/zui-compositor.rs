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
