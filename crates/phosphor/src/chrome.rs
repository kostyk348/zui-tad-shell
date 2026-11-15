//! Рамка окна на холсте: полоса заголовка, акцент фокуса, размер.
//!
//! Зачем это в оболочке, а не в клиенте: на бесконечном холсте окно без рамки —
//! это просто пиксели. Не видно ни границы, ни того, за что тащить, ни того, какое
//! из окон активно. Клиентам мы декорации не навязываем (`xdg-decoration` просит
//! CSD), поэтому серверная рамка рисуется здесь и композитится по z-порядку окон.
//!
//! Полоса рисуется в `tiny_skia::Pixmap` в размере окна на экране и загружается в
//! GL как `MemoryRenderBuffer` — тот же путь, что у панели и HUD.

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::theme::Palette;
use crate::widgets::{draw_text, text_width, Fonts};

/// Высота полосы заголовка в экранных пикселях (до масштабирования камерой).
pub const STRIP_H: f32 = 22.0;

/// Что нарисовать на полосе.
pub struct Spec<'a> {
    /// Заголовок окна от клиента (`xdg_toplevel.title`), может быть пустым.
    pub title: &'a str,
    /// Идентификатор приложения (`app_id`), показывается, если заголовка нет.
    pub app_id: &'a str,
    pub focused: bool,
    /// Логический размер окна — печатается справа.
    pub size: (f32, f32),
    /// Масштаб камеры: при сильном отдалении подписи мельчают, и это честно.
    pub zoom: f32,
}

/// Обрезать строку так, чтобы она влезла в `max` пикселей, поставив многоточие.
pub fn fit(font: &ab_glyph::FontVec, text: &str, size: f32, tracking: f32, max: f32) -> String {
    if text_width(font, text, size, tracking) <= max {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}");
        if text_width(font, &format!("{candidate}…"), size, tracking) > max {
            break;
        }
        out.push(ch);
    }
    if out.is_empty() {
        return String::new();
    }
    format!("{out}…")
}

/// Нарисовать полосу заголовка в пиксмап ровно `w`×`STRIP_H`.
///
/// `zoom` влияет только на насыщенность подписей: сама полоса всегда рисуется в
/// экранных пикселях и растягивается рендерером, поэтому текст остаётся чётким
/// на 1:1 и предсказуемо мельчает при отдалении.
pub fn render(pm: &mut Pixmap, fonts: &Fonts, pal: &Palette, spec: &Spec) {
    let w = pm.width() as f32;
    let h = pm.height() as f32;

    let (bg, accent) = if spec.focused {
        (pal.panel_bg, pal.primary)
    } else {
        (pal.bg, pal.dim)
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(bg[0], bg[1], bg[2], 236);
    pm.fill_rect(
        Rect::from_xywh(0.0, 0.0, w, h).unwrap(),
        &paint,
        Transform::identity(),
        None,
    );

    // Акцентная полоса слева: цвет сразу говорит, какое окно в фокусе.
    let mut bar = Paint::default();
    bar.set_color_rgba8(accent[0], accent[1], accent[2], 255);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, 3.0, h) {
        pm.fill_rect(r, &bar, Transform::identity(), None);
    }

    // Рамка вокруг заголовка — тонкая, чтобы полоса читалась как «шапка».
    if let Some(r) = Rect::from_xywh(0.5, h - 1.0, w - 1.0, 1.0) {
        pm.fill_rect(r, &bar, Transform::identity(), None);
    }

    let text_color = if spec.focused { pal.text } else { pal.dim };

    // Справа — размер окна и масштаб: это то, что нельзя узнать по пикселям.
    let badge = format!("{}×{}", spec.size.0.round(), spec.size.1.round());
    let zoom_note = if (spec.zoom - 1.0).abs() > 0.02 {
        format!("  {:.2}×", spec.zoom)
    } else {
        String::new()
    };
    let right = format!("{badge}{zoom_note}");
    let right_w = text_width(&fonts.regular, &right, 10.0, 0.4);
    draw_text(
        pm,
        &fonts.regular,
        &right,
        w - right_w - 10.0,
        15.0,
        10.0,
        0.4,
        pal.dim,
    );

    // Заголовок: клиентский title, иначе app_id, иначе «окно».
    let raw = if !spec.title.is_empty() {
        spec.title
    } else if !spec.app_id.is_empty() {
        spec.app_id
    } else {
        "окно"
    };
    let room = (w - right_w - 24.0).max(0.0);
    let label = fit(&fonts.bold, raw, 11.0, 0.5, room);
    if !label.is_empty() {
        draw_text(pm, &fonts.bold, &label, 10.0, 15.5, 11.0, 0.5, text_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Mode;

    fn fonts() -> Fonts {
        Fonts::load_default().expect("шрифты из репозитория")
    }

    #[test]
    fn fit_never_exceeds_the_room_and_keeps_short_text() {
        let f = fonts();
        assert_eq!(fit(&f.regular, "kitty", 11.0, 0.5, 200.0), "kitty");
        let long = "очень длинный заголовок окна, который точно не влезет";
        let cut = fit(&f.regular, long, 11.0, 0.5, 80.0);
        assert!(cut.ends_with('…'), "обрезано с многоточием: {cut}");
        assert!(text_width(&f.regular, &cut, 11.0, 0.5) <= 80.0 + 1.0);
    }

    #[test]
    fn render_paints_within_the_buffer() {
        let f = fonts();
        let pal = Palette::of(Mode::Rig);
        let mut pm = Pixmap::new(240, STRIP_H as u32).unwrap();
        render(
            &mut pm,
            &f,
            &pal,
            &Spec {
                title: "kitty — zui-tad-shell",
                app_id: "kitty",
                focused: true,
                size: (900.0, 600.0),
                zoom: 1.0,
            },
        );
        // Полоса не пустая: есть непрозрачные пиксели, и они не за краями.
        let opaque = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(opaque > 240 * 10, "полоса нарисована: {opaque} пикселей");
        assert_eq!(pm.width(), 240);
    }

    #[test]
    fn a_focused_strip_differs_from_an_unfocused_one() {
        let f = fonts();
        let pal = Palette::of(Mode::Rig);
        let mut a = Pixmap::new(200, STRIP_H as u32).unwrap();
        let mut b = Pixmap::new(200, STRIP_H as u32).unwrap();
        let spec = |focused| Spec {
            title: "same",
            app_id: "",
            focused,
            size: (100.0, 100.0),
            zoom: 1.0,
        };
        render(&mut a, &f, &pal, &spec(true));
        render(&mut b, &f, &pal, &spec(false));
        assert_ne!(a.data(), b.data(), "фокус должен быть видно");
    }
}
