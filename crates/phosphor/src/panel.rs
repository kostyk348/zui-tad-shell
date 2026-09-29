//! Верхняя панель оболочки: слева — холст, в центре — время, справа — телеметрия.
//!
//! Панель НЕ рисует скруглений, теней и полупрозрачных «стёкол». Только:
//! фон-плашка, одна акцентная линия снизу, капс-подписи с разрядкой,
//! сегментные шкалы для чисел и угловые засечки.

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::texture::{texture, TexKind};
use crate::theme::{Metrics, Palette};
use crate::widgets::{bar_cells, data_row, draw_text, frame, hairline, text_width, Fonts};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect4 {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect4 {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn overlaps(&self, o: &Rect4) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelLayout {
    pub left: Rect4,
    pub center: Rect4,
    pub right: Rect4,
}

pub struct PanelData<'a> {
    pub title: &'a str,
    pub workspace: u8,
    pub canvas_pos: (f32, f32),
    pub zoom: f32,
    pub windows: usize,
    pub clock: &'a str,
    pub date: &'a str,
    /// (подпись, 0..1, тревога?)
    pub meters: [(&'a str, f32, bool); 4],
}

pub fn draw_top_panel(
    pm: &mut Pixmap,
    width: f32,
    fonts: &Fonts,
    data: &PanelData,
    palette: &Palette,
    metrics: &Metrics,
) -> PanelLayout {
    let h = metrics.panel_h;
    let pad = metrics.pad;

    // Фон + акцентная линия снизу (одна, во всю ширину).
    let mut bg = Paint::default();
    bg.set_color_rgba8(
        palette.panel_bg[0],
        palette.panel_bg[1],
        palette.panel_bg[2],
        0xf2,
    );
    if let Some(r) = Rect::from_xywh(0.0, 0.0, width, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    // «потёртая» техпанель: полутон + затемняющие полосы (не идеальная заливка)
    texture(
        pm,
        (0.0, 0.0, width, h),
        TexKind::Halftone,
        palette.dim,
        9.0,
        17,
    );
    texture(pm, (0.0, 0.0, width, h), TexKind::Worn, palette.dim, 3.0, 5);
    hairline(
        pm,
        0.0,
        h - metrics.line,
        width,
        h - metrics.line,
        palette.primary,
        metrics.line,
    );

    // --- левый блок: холст ---
    let ty = h * 0.5 + metrics.value_size * 0.36;
    let title = data.title.to_uppercase();
    let mut x = pad;
    let tw = draw_text(
        pm,
        &fonts.bold,
        &title,
        x,
        ty,
        metrics.value_size,
        metrics.tracking,
        palette.primary,
    );
    x += tw + pad;

    // 9 «воркспейсов» = 9 закладок холста; активная залита.
    for i in 1..=9u8 {
        let on = i == data.workspace;
        let color = if on { palette.primary } else { palette.dim };
        let mut p = Paint::default();
        p.set_color_rgba8(color[0], color[1], color[2], color[3]);
        if let Some(r) = Rect::from_xywh(x, h * 0.35, 7.0, h * 0.3) {
            pm.fill_rect(r, &p, Transform::identity(), None);
        }
        x += 10.0;
    }
    let coord = format!(
        "CAM {:>6.0}:{:<6.0}  ZOOM {:.2}  WIN {}",
        data.canvas_pos.0, data.canvas_pos.1, data.zoom, data.windows
    );
    let cw = text_width(&fonts.regular, &coord, metrics.label_size, metrics.tracking);
    draw_text(
        pm,
        &fonts.regular,
        &coord,
        x + pad,
        h * 0.5 + metrics.label_size * 0.36,
        metrics.label_size,
        metrics.tracking,
        palette.dim,
    );
    let left = Rect4 {
        x: pad,
        y: 0.0,
        w: (x + pad + cw) - pad,
        h,
    };

    // --- центр: время ---
    let clock_scale = metrics.value_size * 1.35;
    let clock_w = text_width(&fonts.bold, data.clock, clock_scale, metrics.tracking);
    let date_w = text_width(
        &fonts.regular,
        data.date,
        metrics.label_size,
        metrics.tracking,
    );
    let center_w = clock_w.max(date_w);
    let cx = (width - center_w) * 0.5;
    draw_text(
        pm,
        &fonts.bold,
        data.clock,
        cx,
        h * 0.5 + clock_scale * 0.36,
        clock_scale,
        metrics.tracking,
        palette.primary,
    );
    // дата — справа от часов тем же блоком (не второй строкой: панель низкая)
    draw_text(
        pm,
        &fonts.regular,
        &data.date.to_uppercase(),
        cx + clock_w + pad,
        h * 0.5 + metrics.label_size * 0.36,
        metrics.label_size,
        metrics.tracking,
        palette.dim,
    );
    let center = Rect4 {
        x: cx,
        y: 0.0,
        w: clock_w + pad + date_w,
        h,
    };

    // --- правый блок: телеметрия ---
    let meter_w = 92.0;
    let total = meter_w * data.meters.len() as f32 + pad * (data.meters.len() as f32 - 1.0);
    let mut mx = width - pad - total;
    let right = Rect4 {
        x: mx,
        y: 0.0,
        w: total,
        h,
    };
    for (name, value, alert) in data.meters.iter() {
        let color = if *alert {
            palette.alert
        } else {
            palette.primary
        };
        let pct = format!("{:>3.0}%", (value.clamp(0.0, 1.0) * 100.0));
        // подпись + процент одной строкой, под ней — сегментная шкала
        let up = name.to_uppercase();
        draw_text(
            pm,
            &fonts.regular,
            &up,
            mx,
            h * 0.5 - 1.0,
            metrics.label_size,
            metrics.tracking,
            palette.dim,
        );
        let pw = text_width(&fonts.regular, &pct, metrics.label_size, metrics.tracking);
        draw_text(
            pm,
            &fonts.regular,
            &pct,
            mx + meter_w - pw,
            h * 0.5 - 1.0,
            metrics.label_size,
            metrics.tracking,
            color,
        );
        // локальная палитра для тревоги
        let mut pal = *palette;
        pal.primary = color;
        bar_cells(
            pm,
            mx,
            h * 0.5 + 2.0,
            10,
            (meter_w - 9.0 * 2.0) / 10.0,
            4.0,
            2.0,
            *value,
            &pal,
        );
        mx += meter_w + pad;
    }

    // Угловые засечки панели — «техническая» деталь RIG.
    frame(
        pm,
        (0.5, 0.5, width - 1.0, h - 1.0),
        palette.dim,
        metrics.line,
        5.0,
    );

    PanelLayout {
        left,
        center,
        right,
    }
}

/// Нижняя строка подсказок: шелл объясняет себя сам.
/// `hint` — контекстная левая часть, `right` — правая (например, «60 FPS · CPU»).
pub fn hint_bar(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    width: f32,
    height: f32,
    hint: &str,
    right: &str,
) {
    let bar_h = 18.0;
    let y = height - bar_h;
    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.panel_bg[0], pal.panel_bg[1], pal.panel_bg[2], 0xdd);
    if let Some(r) = Rect::from_xywh(0.0, y, width, bar_h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    texture(pm, (0.0, y, width, bar_h), TexKind::Bands, pal.dim, 4.0, 21);
    hairline(pm, 0.0, y, width, y, pal.dim, m.line);
    draw_text(
        pm,
        &fonts.regular,
        hint,
        12.0,
        y + 13.0,
        m.label_size,
        m.tracking,
        pal.dim,
    );
    if !right.is_empty() {
        let rw = text_width(&fonts.regular, right, m.label_size, m.tracking);
        draw_text(
            pm,
            &fonts.regular,
            right,
            width - rw - 12.0,
            y + 13.0,
            m.label_size,
            m.tracking,
            pal.primary,
        );
    }
}

/// Утилита для скриншота: правая колонка телеметрии как список строк.
pub fn draw_side_list(
    pm: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    rows: &[(&str, &str)],
    fonts: &Fonts,
    palette: &Palette,
    metrics: &Metrics,
) {
    let mut cy = y;
    for (name, value) in rows {
        data_row(pm, &fonts.regular, name, value, x, cy, w, palette, metrics);
        cy += metrics.value_size + 8.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Mode;
    use tiny_skia::Pixmap;

    fn data() -> PanelData<'static> {
        PanelData {
            title: "ZUI-TAD",
            workspace: 3,
            canvas_pos: (120.0, -40.0),
            zoom: 1.25,
            windows: 7,
            clock: "21:47",
            date: "29 SEP",
            meters: [
                ("cpu", 0.37, false),
                ("ram", 0.62, false),
                ("vol", 0.80, false),
                ("bat", 0.12, true),
            ],
        }
    }

    #[test]
    fn panel_blocks_are_disjoint_and_inside_width() {
        let fonts = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Rig);
        let m = Metrics::default();
        let mut pm = Pixmap::new(1920, 200).unwrap();
        let l = draw_top_panel(&mut pm, 1920.0, &fonts, &data(), &pal, &m);

        assert!(!l.left.overlaps(&l.right), "лево и право не пересекаются");
        assert!(!l.center.overlaps(&l.left), "центр не лезет в левый блок");
        for r in [l.left, l.center, l.right] {
            assert!(
                r.x >= 0.0 && r.right() <= 1920.0,
                "блок внутри ширины: {r:?}"
            );
            assert_eq!(r.h, m.panel_h);
        }
    }

    #[test]
    fn panel_paints_its_strip_and_nothing_below() {
        let fonts = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Signalis);
        let m = Metrics::default();
        let mut pm = Pixmap::new(800, 120).unwrap();
        draw_top_panel(&mut pm, 800.0, &fonts, &data(), &pal, &m);
        // ниже панели пусто
        let below = (m.panel_h as u32 + 3) * 800 + 400;
        assert_eq!(pm.pixels()[below as usize].alpha(), 0);
        // внутри — есть акцент
        let lit = pm.pixels()[..(m.panel_h as usize * 800)]
            .iter()
            .filter(|p| p.alpha() > 0)
            .count();
        assert!(lit > 500, "панель должна быть плотно нарисована: {lit}");
    }

    #[test]
    fn narrow_panel_does_not_panic() {
        let fonts = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Phosphor);
        let m = Metrics::default();
        let mut pm = Pixmap::new(400, 40).unwrap();
        let _ = draw_top_panel(&mut pm, 400.0, &fonts, &data(), &pal, &m);
    }
}
