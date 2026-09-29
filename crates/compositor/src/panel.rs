//! # Top Panel
//!
//! Верхняя панель DE. Высота 28px, тёмно-серый фон.
//! Содержимое слева направо:
//!   [ZUI-TAD] [ws1 ws2 ws3 ... ws9] [running apps] ... [clock] [tray]

use crate::state::CompositorState;
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use tiny_skia::{Paint, Pixmap, Rect, Transform};

pub const PANEL_HEIGHT: f32 = 28.0;

pub fn draw_panel(
    pm: &mut Pixmap,
    state: &CompositorState,
    font: &FontVec,
    font_bold: &FontVec,
    width: u32,
) {
    // Фон.
    let mut bg = Paint::default();
    bg.set_color_rgba8(0x10, 0x11, 0x15, 0xFF);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, width as f32, PANEL_HEIGHT) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    // Нижняя линия.
    let mut line = Paint::default();
    line.set_color_rgba8(0xff, 0xd7, 0x3a, 0xFF);
    if let Some(r) = Rect::from_xywh(0.0, PANEL_HEIGHT - 1.0, width as f32, 1.0) {
        pm.fill_rect(r, &line, Transform::identity(), None);
    }

    let mut x = 12.0;
    let y = 19.0;

    // 1. Логотип.
    draw_text_bold(pm, "ZUI-TAD", x, y, 14.0, [0xff, 0xd7, 0x3a, 0xff], font_bold);
    x += 70.0;

    // 2. Workspaces.
    for i in 0..state.workspaces.count {
        let is_current = i == state.current_workspace;
        let label = (i + 1).to_string();
        let color = if is_current { [0xff, 0xd7, 0x3a, 0xff] } else { [0x80, 0x84, 0x90, 0xff] };
        // Бокс вокруг текущего.
        if is_current {
            let mut p = Paint::default();
            p.set_color_rgba8(0xff, 0xd7, 0x3a, 0x30);
            if let Some(r) = Rect::from_xywh(x - 4.0, 4.0, 22.0, PANEL_HEIGHT - 8.0) {
                pm.fill_rect(r, &p, Transform::identity(), None);
            }
        }
        draw_text(pm, &label, x, y, 13.0, color, font);
        x += 24.0;
    }

    x += 16.0;

    // 3. Running apps на текущем workspace.
    let win_count = state.windows.len();
    if win_count > 0 {
        draw_text(pm, &format!("Apps: {}", win_count), x, y, 12.0,
            [0xaa, 0xae, 0xb8, 0xff], font);
        x += 80.0;
    }

    // 4. Часы справа.
    let now = chrono::Local::now();
    let clock = now.format("%a %d %b  %H:%M").to_string();
    let clock_w = text_width(&clock, font, 13.0);
    draw_text(pm, &clock, width as f32 - clock_w - 12.0, y, 13.0,
        [0xff, 0xff, 0xff, 0xff], font);

    // 5. Launcher query (если открыт).
    if state.launcher_visible {
        let mut p = Paint::default();
        p.set_color_rgba8(0x1a, 0x1d, 0x25, 0xE0);
        if let Some(r) = Rect::from_xywh(width as f32 * 0.25, 40.0,
                                          width as f32 * 0.5, 36.0) {
            pm.fill_rect(r, &p, Transform::identity(), None);
        }
        let prompt = format!("▶ {}", state.launcher_query);
        draw_text_bold(pm, &prompt, width as f32 * 0.25 + 16.0, 64.0,
            18.0, [0xff, 0xd7, 0x3a, 0xff], font_bold);
    }
}

fn draw_text(pm: &mut Pixmap, text: &str, x: f32, y: f32, size: f32, color: [u8; 4], font: &FontVec) {
    skia_renderer::painter::Painter::draw_text(pm, font, text, x, y,
        PxScale::from(size), color);
}

fn draw_text_bold(pm: &mut Pixmap, text: &str, x: f32, y: f32, size: f32, color: [u8; 4], font: &FontVec) {
    skia_renderer::painter::Painter::draw_text(pm, font, text, x, y,
        PxScale::from(size), color);
}

fn text_width(text: &str, font: &FontVec, size: f32) -> f32 {
    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    let mut w = 0.0_f32;
    let mut last: Option<ab_glyph::GlyphId> = None;
    for ch in text.chars() {
        let gid = scaled.scaled_glyph(ch).id;
        w += scaled.h_advance(gid);
        if let Some(prev) = last {
            w += scaled.kern(prev, gid);
        }
        last = Some(gid);
    }
    w
}
