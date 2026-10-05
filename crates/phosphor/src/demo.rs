//! Мок-содержимое холста: окна, кластеры, PiP, модуль телеметрии, лаунчер.
//!
//! Живёт в библиотеке (а не в бинарях), потому что этим кодом рисуются
//! И статичный PNG (`shell_shot`), И живое окно (`zui-preview`).
//! Всё — обычные функции от `(x, y, w, h)`, без знания о камере: caller
//! сам решает, как получить экранный прямоугольник.

use cgmath::Point2;
use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::texture::{texture, TexKind};
use crate::theme::{Metrics, Palette};
use crate::widgets::{bar_cells, data_row, draw_text, frame, hairline, text_width, Fonts};
use canvas_engine::Camera;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinState {
    Live,
    Focused,
    Suspended,
}

pub fn fill(pm: &mut Pixmap, c: [u8; 4]) {
    crate::blit::fill(pm, c);
}

pub fn with_alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

/// Точки бесконечного холста в экранных координатах (учитывает pan/zoom).
pub fn canvas_grid_camera(pm: &mut Pixmap, pal: &Palette, camera: &Camera, world_step: f32) {
    let step_px = world_step * camera.zoom;
    if step_px < 6.0 {
        return;
    }
    let view = camera.view_aabb_world();
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let mut p = Paint::default();
    p.set_color_rgba8(pal.dim[0], pal.dim[1], pal.dim[2], 0x38);

    let start_x = (view.min.x / world_step).floor() * world_step;
    let start_y = (view.min.y / world_step).floor() * world_step;
    let mut wx = start_x;
    while wx <= view.max.x {
        let sx = camera.world_to_screen(Point2::new(wx, 0.0)).x;
        // засечка на каждой второй линии — «технический» вид, не обои
        let tick_h = if ((wx / world_step).round() as i64) % 4 == 0 {
            3.0
        } else {
            1.0
        };
        let mut wy = start_y;
        while wy <= view.max.y {
            let sy = camera.world_to_screen(Point2::new(0.0, wy)).y;
            if sx >= 0.0 && sx < w && sy >= 0.0 && sy < h {
                if let Some(r) = Rect::from_xywh(sx, sy, tick_h, tick_h) {
                    pm.fill_rect(r, &p, Transform::identity(), None);
                }
            }
            wy += world_step;
        }
        wx += world_step;
    }
}

/// Окно-карточка на холсте. `rect` — уже экранный.
#[allow(clippy::too_many_arguments)]
pub fn window_card(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    rect: (f32, f32, f32, f32),
    title: &str,
    app: &str,
    state: WinState,
) {
    let (x, y, w, h) = rect;
    if w < 8.0 || h < 8.0 {
        return;
    }
    let accent = match state {
        WinState::Focused => pal.primary,
        WinState::Live => with_alpha(pal.dim, 0xff),
        WinState::Suspended => with_alpha(pal.cold, 0xb0),
    };

    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.panel_bg[0], pal.panel_bg[1], pal.panel_bg[2], 0xf0);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }

    // Заголовок: при сильном отдалении не рисуем текст (LOD).
    if h > 40.0 {
        let bar_h = 22.0;
        let mut bar = Paint::default();
        bar.set_color_rgba8(pal.bg[0], pal.bg[1], pal.bg[2], 0xff);
        if let Some(r) = Rect::from_xywh(x, y, w, bar_h) {
            pm.fill_rect(r, &bar, Transform::identity(), None);
        }
        hairline(pm, x, y + bar_h, x + w, y + bar_h, accent, m.line);
        draw_text(
            pm,
            &fonts.bold,
            &title.to_uppercase(),
            x + 8.0,
            y + 15.0,
            m.label_size,
            m.tracking,
            accent,
        );
        let aw = text_width(&fonts.regular, app, m.label_size, m.tracking);
        if aw < w - 20.0 {
            draw_text(
                pm,
                &fonts.regular,
                app,
                x + w - aw - 8.0,
                y + 15.0,
                m.label_size,
                m.tracking,
                pal.dim,
            );
        }
    }

    frame(pm, (x, y, w, h), accent, m.line, 6.0);
    // технические детали RIG: заклёпки по углам и полосы внутри корпуса
    texture(pm, (x, y, w, h), TexKind::Rivets, accent, 4.0, 0);
    texture(
        pm,
        (x, y + 22.0, w, (h - 22.0).max(1.0)),
        TexKind::Bands,
        accent,
        5.0,
        1,
    );

    match state {
        WinState::Suspended => {
            draw_text(
                pm,
                &fonts.bold,
                "SUSPENDED",
                x + 16.0,
                y + 60.0,
                m.value_size * 1.1,
                m.tracking,
                accent,
            );
            draw_text(
                pm,
                &fonts.regular,
                "ENTER / CLICK TO RESUME IN PLACE",
                x + 16.0,
                y + 82.0,
                m.label_size,
                m.tracking,
                pal.dim,
            );
            let mut ly = y + 110.0;
            while ly < y + h - 12.0 {
                hairline(
                    pm,
                    x + 16.0,
                    ly,
                    x + w - 16.0,
                    ly,
                    with_alpha(pal.dim, 0x50),
                    1.0,
                );
                ly += 18.0;
            }
        }
        _ => {
            let lines: [(&str, f32); 8] = [
                ("ok", 0.55),
                ("build", 0.78),
                ("focused", 0.34),
                ("canvas", 0.62),
                ("cluster", 0.45),
                ("phosphor", 0.86),
                ("snap", 0.28),
                ("zoom", 0.70),
            ];
            for (i, (txt, lw)) in lines.iter().enumerate() {
                let ly = y + 48.0 + i as f32 * 22.0;
                if ly > y + h - 18.0 {
                    break;
                }
                let dim = with_alpha(pal.dim, 0xcc);
                draw_text(
                    pm,
                    &fonts.regular,
                    &format!("{:02}", i + 1),
                    x + 12.0,
                    ly,
                    m.label_size,
                    m.tracking,
                    dim,
                );
                draw_text(
                    pm,
                    &fonts.regular,
                    txt,
                    x + 40.0,
                    ly,
                    m.value_size,
                    0.6,
                    pal.text,
                );
                let barw = (w - 60.0) * lw;
                if barw > 2.0 {
                    let mut p = Paint::default();
                    p.set_color_rgba8(accent[0], accent[1], accent[2], 0x60);
                    if let Some(r) = Rect::from_xywh(x + w - 12.0 - barw, ly - 9.0, barw, 3.0) {
                        pm.fill_rect(r, &p, Transform::identity(), None);
                    }
                }
            }
        }
    }
}

/// Скобка кластера вокруг сцепленных окон.
pub fn cluster_bracket(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    rect: (f32, f32, f32, f32),
    count: usize,
) {
    let (x, y, w, h) = rect;
    let c = with_alpha(pal.cold, 0xcc);
    let d = 14.0;
    hairline(pm, x - d, y - d, x - d + 40.0, y - d, c, m.line);
    hairline(pm, x - d, y - d, x - d, y - d + 30.0, c, m.line);
    hairline(
        pm,
        x + w + d,
        y + h + d,
        x + w + d - 40.0,
        y + h + d,
        c,
        m.line,
    );
    hairline(
        pm,
        x + w + d,
        y + h + d,
        x + w + d,
        y + h + d - 30.0,
        c,
        m.line,
    );
    let txt = format!("CLUSTER · {count} WINDOWS · MOVE TOGETHER");
    draw_text(
        pm,
        &fonts.regular,
        &txt,
        x - d + 48.0,
        y - d - 6.0,
        m.label_size,
        m.tracking,
        c,
    );
}

/// PiP: pinned_to_screen — не подчиняется камере холста.
pub fn pip_card(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    rect: (f32, f32, f32, f32),
) {
    let (x, y, w, h) = rect;
    let c = with_alpha(pal.cold, 0xdd);
    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.bg[0], pal.bg[1], pal.bg[2], 0xf0);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    let mut i = 0;
    while 34.0 + i as f32 * 16.0 < h {
        let ly = y + 34.0 + i as f32 * 16.0;
        let mut p = Paint::default();
        p.set_color_rgba8(c[0], c[1], c[2], 0x40);
        let lw = (w - 60.0) * (1.0 - i as f32 * 0.08);
        if lw > 2.0 {
            if let Some(r) = Rect::from_xywh(x + 12.0, ly, lw, 2.0) {
                pm.fill_rect(r, &p, Transform::identity(), None);
            }
        }
        i += 1;
    }
    frame(pm, (x, y, w, h), c, m.line, 6.0);
    draw_text(
        pm,
        &fonts.bold,
        "PIP · PINNED TO SCREEN",
        x + 10.0,
        y + 20.0,
        m.label_size,
        m.tracking,
        c,
    );
}

/// RIG-модуль телеметрии (левый нижний блок).
pub fn telemetry_module(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    x: f32,
    y: f32,
    w: f32,
    rows: &[(&str, &str)],
    bars: &[(&str, f32, bool)],
) -> f32 {
    let rows_h = 40.0 + rows.len() as f32 * (m.value_size + 8.0) + bars.len() as f32 * 22.0 + 10.0;
    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.panel_bg[0], pal.panel_bg[1], pal.panel_bg[2], 0xdc);
    if let Some(r) = Rect::from_xywh(x, y, w, rows_h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    frame(
        pm,
        (x, y, w, rows_h),
        with_alpha(pal.dim, 0xff),
        m.line,
        6.0,
    );
    draw_text(
        pm,
        &fonts.bold,
        "RIG",
        x + 12.0,
        y + 20.0,
        m.value_size,
        m.tracking,
        pal.primary,
    );

    let mut cy = y + 44.0;
    for (name, value) in rows {
        data_row(
            pm,
            &fonts.regular,
            name,
            value,
            x + 12.0,
            cy,
            w - 24.0,
            pal,
            m,
        );
        cy += m.value_size + 8.0;
    }
    cy += 6.0;
    for (label, v, alert) in bars {
        let mut p = *pal;
        if *alert {
            p.primary = pal.alert;
        }
        draw_text(
            pm,
            &fonts.regular,
            label,
            x + 12.0,
            cy + 8.0,
            m.label_size,
            m.tracking,
            p.dim,
        );
        bar_cells(pm, x + 60.0, cy, 20, 9.0, 6.0, 2.0, *v, &p);
        cy += 22.0;
    }
    rows_h
}

/// Лаунчер (spotlight) с активной строкой.
pub fn launcher_overlay(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    x: f32,
    y: f32,
    w: f32,
    query: &str,
    entries: &[(&str, &str, bool)],
) {
    let row_h = 34.0;
    let h = 46.0 + entries.len() as f32 * row_h + 26.0;

    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.panel_bg[0], pal.panel_bg[1], pal.panel_bg[2], 0xf4);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    frame(pm, (x, y, w, h), pal.primary, m.line, 8.0);
    draw_text(
        pm,
        &fonts.bold,
        "LAUNCH",
        x + 14.0,
        y + 24.0,
        m.value_size,
        m.tracking,
        pal.primary,
    );
    draw_text(
        pm,
        &fonts.regular,
        ">",
        x + 100.0,
        y + 24.0,
        m.value_size,
        m.tracking,
        pal.primary,
    );
    draw_text(
        pm,
        &fonts.regular,
        query,
        x + 118.0,
        y + 24.0,
        m.value_size,
        0.6,
        pal.text,
    );
    // каретка
    let qw = text_width(&fonts.regular, query, m.value_size, 0.6);
    let mut cur = Paint::default();
    cur.set_color_rgba8(pal.primary[0], pal.primary[1], pal.primary[2], 0xcc);
    if let Some(r) = Rect::from_xywh(x + 120.0 + qw, y + 14.0, 2.0, 14.0) {
        pm.fill_rect(r, &cur, Transform::identity(), None);
    }

    let mut ry = y + 46.0;
    for (name, kind, selected) in entries {
        if *selected {
            let mut sel = Paint::default();
            sel.set_color_rgba8(pal.primary[0], pal.primary[1], pal.primary[2], 0x28);
            if let Some(r) = Rect::from_xywh(x + 8.0, ry, w - 16.0, row_h - 4.0) {
                pm.fill_rect(r, &sel, Transform::identity(), None);
            }
            let mut mk = Paint::default();
            mk.set_color_rgba8(pal.primary[0], pal.primary[1], pal.primary[2], 0xff);
            if let Some(r) = Rect::from_xywh(x + 8.0, ry, 3.0, row_h - 4.0) {
                pm.fill_rect(r, &mk, Transform::identity(), None);
            }
        }
        draw_text(
            pm,
            if *selected {
                &fonts.bold
            } else {
                &fonts.regular
            },
            name,
            x + 22.0,
            ry + 20.0,
            m.value_size,
            0.6,
            if *selected { pal.primary } else { pal.text },
        );
        let kw = text_width(&fonts.regular, kind, m.label_size, m.tracking);
        draw_text(
            pm,
            &fonts.regular,
            &kind.to_uppercase(),
            x + w - kw - 16.0,
            ry + 20.0,
            m.label_size,
            m.tracking,
            pal.dim,
        );
        ry += row_h;
    }
    draw_text(
        pm,
        &fonts.regular,
        "ENTER RUN · ESC CLOSE · 1..3 THEME",
        x + 14.0,
        y + h - 12.0,
        m.label_size,
        m.tracking,
        pal.dim,
    );
}
