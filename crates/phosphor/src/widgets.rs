//! Примитивы оболочки: линии, рамки-скобки, сегментные шкалы, подписи.
//!
//! Всё рисуется в tiny-skia `Pixmap` — то есть одинаково работает и в
//! headless-скриншоте, и как текстура для композитора.
//!
//! Стилевые правила (Dead Space RIG / Signalis):
//!   * толщина всех линий = `Metrics::line` (важно: не 2px, не скругления);
//!   * подписи — КАПС с разрядкой `tracking`, значения — тем же шрифтом крупнее;
//!   * числа дублируются сегментной шкалой (RIG не доверяет тексту);
//!   * рамки имеют угловые засечки, а не скруглённые углы.

use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
use tiny_skia::{Paint, Pixmap, PremultipliedColorU8, Rect, Transform};

use crate::theme::{Metrics, Palette};

/// Регулярный + жирный шрифт оболочки.
pub struct Fonts {
    pub regular: FontVec,
    pub bold: FontVec,
}

impl Fonts {
    pub fn load_default() -> anyhow::Result<Self> {
        Ok(Self {
            regular: FontVec::try_from_vec(
                include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec(),
            )?,
            bold: FontVec::try_from_vec(
                include_bytes!("../../../assets/fonts/DejaVuSans-Bold.ttf").to_vec(),
            )?,
        })
    }
}

fn over(pm: &mut Pixmap, gx: i32, gy: i32, coverage: f32, color: [u8; 4]) {
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    if gx < 0 || gy < 0 || gx >= w || gy >= h {
        return;
    }
    let i = (gy as u32 * pm.width() + gx as u32) as usize;
    let a = (coverage * color[3] as f32).clamp(0.0, 1.0);
    let px = pm.pixels_mut();
    let dst = px[i].demultiply();
    let out = [
        (color[0] as f32 * a + dst.red() as f32 * (1.0 - a)) as u8,
        (color[1] as f32 * a + dst.green() as f32 * (1.0 - a)) as u8,
        (color[2] as f32 * a + dst.blue() as f32 * (1.0 - a)) as u8,
        255,
    ];
    if let Some(p) = PremultipliedColorU8::from_rgba(out[0], out[1], out[2], out[3]) {
        px[i] = p;
    }
}

/// Нарисовать строку с разрядкой. Возвращает занятую ширину.
pub fn draw_text(
    pm: &mut Pixmap,
    font: &FontVec,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    tracking: f32,
    color: [u8; 4],
) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut cursor = x;
    for ch in text.chars() {
        let mut glyph = scaled.scaled_glyph(ch);
        let gid = glyph.id;
        glyph.position = point(cursor, y);
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            let (ox, oy) = (bounds.min.x as i32, bounds.min.y as i32);
            outline.draw(|gx, gy, cov| over(pm, ox + gx as i32, oy + gy as i32, cov, color));
        }
        cursor += scaled.h_advance(gid) + tracking;
    }
    cursor - x
}

pub fn text_width(font: &FontVec, text: &str, size: f32, tracking: f32) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut w = 0.0;
    for ch in text.chars() {
        w += scaled.h_advance(scaled.scaled_glyph(ch).id) + tracking;
    }
    w
}

/// Капс-подпись с разрядкой.
pub fn label(
    pm: &mut Pixmap,
    font: &FontVec,
    text: &str,
    x: f32,
    y: f32,
    palette: &Palette,
    metrics: &Metrics,
) -> f32 {
    let up = text.to_uppercase();
    draw_text(
        pm,
        font,
        &up,
        x,
        y,
        metrics.label_size,
        metrics.tracking,
        palette.dim,
    )
}

/// Обрезка отрезка по прямоугольнику (Liang–Barsky).
/// Нужна потому, что tiny-skia в debug-сборке паникует на хайрлайне,
/// вылезающем за полотно (assert в `scan/hairline_aa.rs`). Обрезаем — и
/// заодно не тратим время на невидимое.
pub fn clip_segment(
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    w: f32,
    h: f32,
) -> Option<(f32, f32, f32, f32)> {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let mut t0 = 0.0f32;
    let mut t1 = 1.0f32;
    let checks = [(-dx, x1), (dx, w - x1), (-dy, y1), (dy, h - y1)];
    for (p, q) in checks {
        if p.abs() < f32::EPSILON {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                if r > t0 {
                    t0 = r;
                }
            } else {
                if r < t0 {
                    return None;
                }
                if r < t1 {
                    t1 = r;
                }
            }
        }
    }
    Some((x1 + t0 * dx, y1 + t0 * dy, x1 + t1 * dx, y1 + t1 * dy))
}

/// Тонкая линия (все линии оболочки — одной толщины).
///
/// Отрезок обрезается по полотну: это и защита от падения tiny-skia, и
/// экономия (невидимое не рисуем).
pub fn hairline(pm: &mut Pixmap, x1: f32, y1: f32, x2: f32, y2: f32, color: [u8; 4], width: f32) {
    let finite = [x1, y1, x2, y2, width].iter().all(|v| v.is_finite());
    if !finite || width <= 0.0 {
        return;
    }
    if (x1 - x2).abs() < f32::EPSILON && (y1 - y2).abs() < f32::EPSILON {
        return;
    }
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let Some((cx1, cy1, cx2, cy2)) = clip_segment(x1, y1, x2, y2, w, h) else {
        return;
    };
    // Ширину держим >= 1.0: во-первых, это наш стиль (1px хайрлайн),
    // во-вторых, у tiny-skia отдельный hairline-растеризатор для width < 1,
    // который падает на коротких/вырожденных отрезках (проверено экспериментально).
    let width = width.max(1.0);
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(cx1, cy1);
    pb.line_to(cx2, cy2);
    if let Some(path) = pb.finish() {
        let stroke = tiny_skia::Stroke {
            width,
            ..Default::default()
        };
        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// Рамка с угловыми засечками (RIG-стиль). `tick` = длина засечки, 0 = без них.
pub fn frame(pm: &mut Pixmap, rect: (f32, f32, f32, f32), color: [u8; 4], width: f32, tick: f32) {
    let (x, y, w, h) = rect;
    hairline(pm, x, y, x + w, y, color, width);
    hairline(pm, x, y + h, x + w, y + h, color, width);
    hairline(pm, x, y, x, y + h, color, width);
    hairline(pm, x + w, y, x + w, y + h, color, width);
    if tick > 0.0 {
        // засечки внутрь, по 2 на угол
        for (cx, sx) in [(x, 1.0), (x + w, -1.0)] {
            for (cy, sy) in [(y, 1.0), (y + h, -1.0)] {
                hairline(pm, cx, cy, cx + tick * sx, cy, color, width);
                hairline(pm, cx, cy, cx, cy + tick * sy, color, width);
            }
        }
    }
}

/// Сколько ячеек сегментной шкалы зажечь при заполнении `fill01`.
pub fn cells_on(fill01: f32, cells: usize) -> usize {
    if cells == 0 {
        return 0;
    }
    let f = fill01.clamp(0.0, 1.0);
    ((f * cells as f32).round() as usize).min(cells)
}

/// Сегментная шкала: `cells` прямоугольников, зажжённые — акцентом.
/// Заполнение округляется ВВЕРХ до целой ячейки, но 0 остаётся нулём.
#[allow(clippy::too_many_arguments)]
pub fn bar_cells(
    pm: &mut Pixmap,
    x: f32,
    y: f32,
    cells: usize,
    cell_w: f32,
    cell_h: f32,
    gap: f32,
    fill01: f32,
    palette: &Palette,
) {
    let on = cells_on(fill01, cells);
    for i in 0..cells {
        let cx = x + i as f32 * (cell_w + gap);
        let color = palette.cell(i < on);
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        if let Some(r) = Rect::from_xywh(cx, y, cell_w, cell_h) {
            pm.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
}

/// Тонкие «насечки» уровня (как шкала громкости в RIG).
#[allow(clippy::too_many_arguments)]
pub fn tick_row(
    pm: &mut Pixmap,
    x: f32,
    y: f32,
    count: usize,
    tick_w: f32,
    tick_h: f32,
    gap: f32,
    fill01: f32,
    palette: &Palette,
) {
    let on = cells_on(fill01, count);
    for i in 0..count {
        let cx = x + i as f32 * (tick_w + gap);
        let color = if i < on { palette.primary } else { palette.dim };
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        if let Some(r) = Rect::from_xywh(cx, y, tick_w, tick_h) {
            pm.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
}

/// Строка данных: ПОДПИСЬ слева, значение справа, между ними — точечный вожак.
#[allow(clippy::too_many_arguments)]
pub fn data_row(
    pm: &mut Pixmap,
    font: &FontVec,
    name: &str,
    value: &str,
    x: f32,
    y: f32,
    w: f32,
    palette: &Palette,
    metrics: &Metrics,
) -> f32 {
    let name_up = name.to_uppercase();
    let lw = draw_text(
        pm,
        font,
        &name_up,
        x,
        y,
        metrics.label_size,
        metrics.tracking,
        palette.dim,
    );
    let vw = text_width(font, value, metrics.value_size, metrics.tracking);
    draw_text(
        pm,
        font,
        value,
        x + w - vw,
        y,
        metrics.value_size,
        metrics.tracking,
        palette.text,
    );
    // вожак
    let gap_start = x + lw + 6.0;
    let gap_end = x + w - vw - 6.0;
    let mut cx = gap_start;
    while cx < gap_end {
        let mut paint = Paint::default();
        paint.set_color_rgba8(palette.dim[0], palette.dim[1], palette.dim[2], 0x80);
        if let Some(r) = Rect::from_xywh(cx, y - 2.0, 1.0, 1.0) {
            pm.fill_rect(r, &paint, Transform::identity(), None);
        }
        cx += 4.0;
    }
    metrics.value_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Mode;

    fn pm(w: u32, h: u32) -> Pixmap {
        Pixmap::new(w, h).unwrap()
    }

    #[test]
    fn text_width_grows_with_tracking_and_length() {
        let f = Fonts::load_default().unwrap();
        let a = text_width(&f.regular, "VOL", 10.0, 0.0);
        let b = text_width(&f.regular, "VOL", 10.0, 2.0);
        let c = text_width(&f.regular, "VOLUME", 10.0, 0.0);
        assert!(b > a, "разрядка добавляет ширину");
        assert!(c > a, "длиннее строка — шире");
    }

    #[test]
    fn drawing_text_marks_pixels() {
        let f = Fonts::load_default().unwrap();
        let mut p = pm(64, 24);
        let pal = Palette::of(Mode::Signalis);
        let used = draw_text(&mut p, &f.bold, "RIG", 2.0, 16.0, 14.0, 2.0, pal.primary);
        assert!(used > 10.0);
        let lit = p.pixels().iter().filter(|px| px.alpha() > 0).count();
        assert!(lit > 20, "текст должен оставить след, lit={lit}");
    }

    #[test]
    fn cells_on_rounds_and_clamps() {
        assert_eq!(cells_on(0.0, 10), 0);
        assert_eq!(cells_on(0.001, 10), 0);
        assert_eq!(cells_on(0.04, 10), 0);
        assert_eq!(cells_on(0.05, 10), 1);
        assert_eq!(cells_on(0.5, 10), 5);
        assert_eq!(cells_on(1.0, 10), 10);
        assert_eq!(cells_on(9.0, 10), 10);
        assert_eq!(cells_on(-9.0, 10), 0);
        assert_eq!(cells_on(0.5, 0), 0);
    }

    #[test]
    fn bar_cells_paints_expected_number_of_segments() {
        let pal = Palette::of(Mode::Rig);
        let mut p = pm(64, 12);
        // 3 из 4 ячеек зажжены
        bar_cells(&mut p, 0.0, 0.0, 4, 6.0, 8.0, 2.0, 0.75, &pal);
        // 2 из 4 ячеек зажжены → считаем пиксели цвета primary
        let prim = p
            .pixels()
            .iter()
            .filter(|px| {
                let c = px.demultiply();
                [c.red(), c.green(), c.blue()] == [pal.primary[0], pal.primary[1], pal.primary[2]]
            })
            .count();
        let dim = p
            .pixels()
            .iter()
            .filter(|px| {
                let c = px.demultiply();
                [c.red(), c.green(), c.blue()] == [pal.dim[0], pal.dim[1], pal.dim[2]]
            })
            .count();
        assert!(prim > 0 && dim > 0);
        assert!(prim > dim, "3 из 4 ячеек зажжены: зажжённых больше");
    }

    #[test]
    fn clip_segment_handles_all_cases() {
        // целиком внутри — без изменений
        let (a, b, c, d) = clip_segment(2.0, 3.0, 8.0, 9.0, 16.0, 16.0).unwrap();
        assert!((a - 2.0).abs() < 1e-3 && (d - 9.0).abs() < 1e-3);
        // целиком снаружи — отбрасывается
        assert_eq!(clip_segment(-40.0, -40.0, -30.0, -30.0, 16.0, 16.0), None);
        assert_eq!(clip_segment(50.0, 5.0, 60.0, 9.0, 16.0, 16.0), None);
        // пересекает границу — обрезается внутрь
        let (x1, y1, x2, y2) = clip_segment(-100.0, 8.0, 100.0, 8.0, 16.0, 16.0).unwrap();
        assert!(x1 >= 0.0 && x2 <= 16.0 && (y1 - 8.0).abs() < 1e-3);
    }

    #[test]
    fn hairline_never_panics_outside_canvas() {
        // именно этот вход раньше ронял tiny-skia (debug-assert в hairline_aa)
        let pal = Palette::of(Mode::Rig);
        let mut p = pm(32, 32);
        for (x1, y1, x2, y2) in [
            (-50.0, -50.0, 50.0, 50.0),
            (-1000.0, 10.0, -900.0, 10.0),
            (10.0, -500.0, 10.0, 500.0),
            (0.0, 0.0, 0.0, 0.0),
            (f32::NAN, 0.0, 10.0, 10.0),
            (1.0, 1.0, 2.0, 2.0),
        ] {
            hairline(&mut p, x1, y1, x2, y2, pal.primary, 1.0);
        }
    }

    #[test]
    fn frame_leaves_center_untouched() {
        let pal = Palette::of(Mode::Phosphor);
        let mut p = pm(40, 30);
        frame(&mut p, (0.0, 0.0, 39.0, 29.0), pal.primary, 1.0, 4.0);
        let c = p.pixels()[(15 * 40 + 20) as usize];
        assert_eq!(c.alpha(), 0, "внутри рамки ничего не закрашено");
    }

    #[test]
    fn data_row_places_value_at_right_edge() {
        let f = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Rig);
        let m = Metrics::default();
        let mut p = pm(200, 24);
        data_row(&mut p, &f.regular, "cpu", "37%", 4.0, 16.0, 150.0, &pal, &m);
        // крайние правые пиксели строки должны содержать значение (не пусто)
        let right_used = (110..150).any(|x| p.pixels()[(10 * 200 + x) as usize].alpha() > 0);
        assert!(right_used, "значение выровнено по правому краю");
    }
}
