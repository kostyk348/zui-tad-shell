//! # render_screenshot
//!
//! Отдельный бинарь: открывает хранилище, рендерит текущее состояние
//! холста в один PNG. Удобно для отладки.
//!
//! Запуск:
//!   ZUI_STORE_PATH=./data/store.sled \
//!   ./target/release/render_screenshot out.png

use anyhow::Result;
use canvas_engine::{cull, Camera};
use cgmath::{Point2, Vector2};
use parking_lot::Mutex;
use skia_renderer::SkiaRenderer;
use std::collections::HashMap;
use std::sync::Arc;
use tad_core::{GraphStore, RealObject, RoId};

fn main() -> Result<()> {
    let store_path =
        std::env::var("ZUI_STORE_PATH").unwrap_or_else(|_| "data/store.sled".to_string());
    let store = Arc::new(Mutex::new(GraphStore::open(&store_path)?));

    let mut ro_index: HashMap<RoId, RealObject> = HashMap::new();
    let mut all_vos = Vec::new();
    for ro in store.lock().all_ro()? {
        ro_index.insert(ro.id, ro);
    }
    let ro_ids: Vec<_> = ro_index.keys().copied().collect();
    for parent in ro_ids {
        all_vos.extend(store.lock().children_of(parent)?);
    }

    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "data/screenshot.png".to_string());
    let _ = std::fs::create_dir_all("data");

    let mut r = SkiaRenderer::new(1920, 1200)?;
    let mut camera = Camera::new(Vector2::new(1920, 1200));
    camera.center = Point2::new(800.0, 500.0);
    camera.zoom = 0.35;

    let culled = cull(&camera, &all_vos);
    let mut frame = r.render(&camera, &culled, &ro_index);

    // Top panel как в DE.
    use ab_glyph::PxScale;
    let fonts = skia_renderer::fonts::FontCache::load_default().ok();
    if let Some(fonts) = &fonts {
        let win_count = all_vos
            .iter()
            .filter(|v| {
                ro_index
                    .get(&v.target_ro)
                    .map(|r| r.kind == tad_core::ObjectKind::WaylandWindow)
                    .unwrap_or(false)
            })
            .count();
        draw_panel_de(&mut frame.pixmap, fonts, 1920, win_count);
    }
    frame.pixmap.save_png(&out)?;
    println!("Saved: {out}  ({} visible VOs)", frame.visible_vos);
    Ok(())
}

fn draw_panel_de(
    pm: &mut tiny_skia::Pixmap,
    fonts: &skia_renderer::fonts::FontCache,
    width: u32,
    win_count: usize,
) {
    use ab_glyph::PxScale;
    use tiny_skia::{Paint, Rect, Transform};
    let width = width as f32;
    const H: f32 = 28.0;

    let mut bg = Paint::default();
    bg.set_color_rgba8(0x10, 0x11, 0x15, 0xFF);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, width, H) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    let mut line = Paint::default();
    line.set_color_rgba8(0xff, 0xd7, 0x3a, 0xFF);
    if let Some(r) = Rect::from_xywh(0.0, H - 1.0, width, 1.0) {
        pm.fill_rect(r, &line, Transform::identity(), None);
    }

    let mut x = 12.0;
    let y = 19.0;
    skia_renderer::painter::Painter::draw_text(
        pm,
        &fonts.bold,
        "ZUI-TAD",
        x,
        y,
        PxScale::from(14.0),
        [0xff, 0xd7, 0x3a, 0xff],
    );
    x += 70.0;
    for i in 0..9u8 {
        let is_current = i == 0;
        let label = (i + 1).to_string();
        let color = if is_current {
            [0xff, 0xd7, 0x3a, 0xff]
        } else {
            [0x80, 0x84, 0x90, 0xff]
        };
        if is_current {
            let mut p = Paint::default();
            p.set_color_rgba8(0xff, 0xd7, 0x3a, 0x30);
            if let Some(r) = Rect::from_xywh(x - 4.0, 4.0, 22.0, H - 8.0) {
                pm.fill_rect(r, &p, Transform::identity(), None);
            }
        }
        skia_renderer::painter::Painter::draw_text(
            pm,
            &fonts.regular,
            &label,
            x,
            y,
            PxScale::from(13.0),
            color,
        );
        x += 24.0;
    }
    x += 16.0;
    if win_count > 0 {
        skia_renderer::painter::Painter::draw_text(
            pm,
            &fonts.regular,
            &format!("Apps: {}", win_count),
            x,
            y,
            PxScale::from(12.0),
            [0xaa, 0xae, 0xb8, 0xff],
        );
    }
    let now = chrono::Local::now();
    let clock = now.format("%a %d %b  %H:%M").to_string();
    // Простая оценка ширины: ~7px/char при 13pt.
    let cw = clock.len() as f32 * 7.0;
    skia_renderer::painter::Painter::draw_text(
        pm,
        &fonts.regular,
        &clock,
        width - cw - 12.0,
        y,
        PxScale::from(13.0),
        [0xff, 0xff, 0xff, 0xff],
    );
}
