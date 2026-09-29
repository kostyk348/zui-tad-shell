//! OSD (on-screen display): громкость, яркость, зум холста.
//!
//! Появляется по центру снизу, живёт секунды. Стиль: рамка с засечками,
//! капс-подпись, крупное значение, сегментная шкала во всю ширину блока.

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::panel::Rect4;
use crate::theme::{Metrics, Palette};
use crate::widgets::{bar_cells, draw_text, frame, text_width, Fonts};

#[allow(clippy::too_many_arguments)]
pub fn draw_osd(
    pm: &mut Pixmap,
    screen_w: f32,
    screen_h: f32,
    title: &str,
    value01: f32,
    value_text: &str,
    fonts: &Fonts,
    palette: &Palette,
    metrics: &Metrics,
) -> Rect4 {
    let w = (screen_w * 0.34).clamp(260.0, 460.0);
    let h = 64.0;
    let x = ((screen_w - w) * 0.5).round();
    let y = (screen_h - h - 56.0).round();

    // Плашка.
    let mut bg = Paint::default();
    bg.set_color_rgba8(
        palette.panel_bg[0],
        palette.panel_bg[1],
        palette.panel_bg[2],
        0xe6,
    );
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    frame(pm, (x, y, w, h), palette.primary, metrics.line, 6.0);

    // Подпись.
    draw_text(
        pm,
        &fonts.regular,
        &title.to_uppercase(),
        x + 12.0,
        y + 18.0,
        metrics.label_size,
        metrics.tracking,
        palette.dim,
    );

    // Значение (крупно).
    let vs = metrics.value_size * 1.6;
    let vw = text_width(&fonts.bold, value_text, vs, metrics.tracking);
    draw_text(
        pm,
        &fonts.bold,
        value_text,
        x + w - vw - 12.0,
        y + 22.0,
        vs,
        metrics.tracking,
        palette.primary,
    );

    // Сегментная шкала.
    let cells = 28usize;
    let inner = w - 24.0;
    let cell_w = (inner - (cells as f32 - 1.0) * 2.0) / cells as f32;
    bar_cells(
        pm,
        x + 12.0,
        y + h - 20.0,
        cells,
        cell_w,
        8.0,
        2.0,
        value01,
        palette,
    );

    Rect4 { x, y, w, h }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Mode;

    #[test]
    fn osd_is_centered_horizontally_and_inside_screen() {
        let fonts = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Rig);
        let m = Metrics::default();
        let mut pm = Pixmap::new(1920, 1080).unwrap();
        let r = draw_osd(
            &mut pm, 1920.0, 1080.0, "volume", 0.62, "62%", &fonts, &pal, &m,
        );
        assert!((r.x + r.w * 0.5 - 960.0).abs() <= 1.0, "по центру: {r:?}");
        assert!(r.y + r.h < 1080.0, "не вылезает за низ");
        assert!(r.w >= 260.0 && r.w <= 460.0);
    }

    #[test]
    fn osd_clamps_extreme_values_and_never_panics_on_tiny_screens() {
        let fonts = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Signalis);
        let m = Metrics::default();
        for (w, h, v) in [
            (320.0, 200.0, 5.0),
            (320.0, 200.0, -3.0),
            (100.0, 80.0, 0.5),
        ] {
            let mut pm = Pixmap::new(w as u32, h as u32).unwrap();
            let _ = draw_osd(&mut pm, w, h, "brightness", v, "??", &fonts, &pal, &m);
        }
    }

    #[test]
    fn osd_draws_accent_inside_its_box() {
        let fonts = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Phosphor);
        let m = Metrics::default();
        let mut pm = Pixmap::new(1200, 800).unwrap();
        let r = draw_osd(
            &mut pm, 1200.0, 800.0, "zoom", 0.5, "1.25x", &fonts, &pal, &m,
        );
        let inside =
            pm.pixels()[(r.y as u32 + 4) as usize * 1200 + (r.x as u32 + 4) as usize].alpha() > 0;
        assert!(inside, "внутри OSD что-то нарисовано");
    }
}
