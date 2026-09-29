//! gen_bg — генерация обоев в `assets/backgrounds/`.
//!
//! Запуск:
//!   cargo run --release -p phosphor --bin gen_bg
//!   cargo run --release -p phosphor --bin gen_bg -- 2560 1440    # своё разрешение
//!
//! Рисуются все виды (hull/starfield/crt/blueprint) во всех трёх темах.
//! Это СВОЯ графика (см. bg.rs): чужие игровые ассеты в репозиторий не кладём.

use anyhow::Result;
use phosphor::bg::{render, BgKind};
use phosphor::theme::{Mode, Palette};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let w: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1920);
    let h: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1080);

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets")
        .join("backgrounds");
    std::fs::create_dir_all(&out)?;

    for (mode_name, mode) in [
        ("rig", Mode::Rig),
        ("signalis", Mode::Signalis),
        ("phosphor", Mode::Phosphor),
    ] {
        let pal = Palette::of(mode);
        for kind in BgKind::all() {
            let seed = (kind.name().len() as u32) * 977 + mode_name.len() as u32;
            let pm = render(kind, w, h, &pal, seed);
            let path = out.join(format!("{}-{}.png", kind.name(), mode_name));
            pm.save_png(&path)?;
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            println!(
                "{}  {}x{}  {:.0} КБ",
                path.display(),
                w,
                h,
                size as f32 / 1024.0
            );
        }
    }
    println!("готово: {}", out.display());
    Ok(())
}
