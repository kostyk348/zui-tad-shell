//! # init_demo
//!
//! Инициализирует хранилище демо-сценой без запуска GUI.
//! Удобно для тестов и для render_screenshot.

use anyhow::Result;
use parking_lot::Mutex;
use std::sync::Arc;
use tad_core::GraphStore;

#[path = "demo_scene.rs"]
mod demo_scene;

fn main() -> Result<()> {
    // Относительный путь по умолчанию (создаётся в CWD), можно переопределить.
    let store_path =
        std::env::var("ZUI_STORE_PATH").unwrap_or_else(|_| "data/store.sled".to_string());
    if std::path::Path::new(&store_path).exists() {
        std::fs::remove_dir_all(&store_path)?;
    }
    // Создаём родительский каталог, если его нет.
    if let Some(parent) = std::path::Path::new(&store_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let store = Arc::new(Mutex::new(GraphStore::open(&store_path)?));
    let demo = demo_scene::build_demo_scene();
    {
        let s = store.lock();
        for ro in &demo.ros {
            s.put_ro(ro)?;
        }
        for vo in &demo.vos {
            s.put_vo(vo)?;
        }
    }
    println!(
        "Демо-сцена инициализирована: {} RO, {} VO в {store_path}",
        demo.ros.len(),
        demo.vos.len()
    );
    println!("Запустите: ./target/release/zui-tad-shell");
    Ok(())
}
