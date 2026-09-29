//! Painter — отрисовка текста через ab_glyph → tiny-skia.

use ab_glyph::{Font, FontVec, Glyph, PxScale, ScaleFont};
use tiny_skia::Pixmap;

pub struct Painter;

impl Painter {
    /// Нарисовать строку с top-left baseline (y — базовая линия).
    pub fn draw_text(
        pm: &mut Pixmap,
        font: &FontVec,
        text: &str,
        x: f32,
        y: f32,
        scale: PxScale,
        color: [u8; 4],
    ) {
        let scaled = font.as_scaled(scale);
        let mut cursor_x = x;
        let mut last: Option<ab_glyph::GlyphId> = None;

        for ch in text.chars() {
            let mut glyph: Glyph = scaled.scaled_glyph(ch);
            glyph.position = ab_glyph::point(cursor_x, y);

            // Kerning.
            if let Some(prev) = last {
                cursor_x += scaled.kern(prev, glyph.id);
                glyph.position = ab_glyph::point(cursor_x, y);
            }

            if let Some(outline) = font.outline_glyph(glyph.clone()) {
                let bounds = outline.px_bounds();
                let px = bounds.min;
                let (w, h) = (pm.width(), pm.height());
                let pixels = pm.pixels_mut();
                outline.draw(move |gx, gy, coverage| {
                    let abs_x = px.x as i32 + gx as i32;
                    let abs_y = px.y as i32 + gy as i32;
                    if abs_x < 0 || abs_y < 0 {
                        return;
                    }
                    if abs_x as u32 >= w || abs_y as u32 >= h {
                        return;
                    }
                    let alpha = (coverage * color[3] as f32) as u8;
                    let idx = abs_y as u32 * w + abs_x as u32;
                    let pixel = &mut pixels[idx as usize];
                    // Простой over-compositing: src над dst с alpha=coverage.
                    let src_a = alpha as f32 / 255.0;
                    let dst_r = pixel.red() as f32;
                    let dst_g = pixel.green() as f32;
                    let dst_b = pixel.blue() as f32;
                    let dst_a = pixel.alpha() as f32 / 255.0;
                    let out_a = src_a + dst_a * (1.0 - src_a);
                    if out_a > 0.001 {
                        let out_r = (color[0] as f32 * src_a + dst_r * (1.0 - src_a)) / out_a;
                        let out_g = (color[1] as f32 * src_a + dst_g * (1.0 - src_a)) / out_a;
                        let out_b = (color[2] as f32 * src_a + dst_b * (1.0 - src_a)) / out_a;
                        *pixel = tiny_skia::PremultipliedColorU8::from_rgba(
                            out_r as u8,
                            out_g as u8,
                            out_b as u8,
                            (out_a * 255.0) as u8,
                        )
                        .unwrap_or(*pixel);
                    }
                });
            }

            cursor_x += scaled.h_advance(glyph.id);
            last = Some(glyph.id);
        }
    }

    /// С переносом строк по ширине.
    pub fn draw_wrapped_text(
        pm: &mut Pixmap,
        font: &FontVec,
        text: &str,
        x: f32,
        y: f32,
        max_w: f32,
        scale: PxScale,
        color: [u8; 4],
    ) {
        let scaled = font.as_scaled(scale);
        let line_h = scaled.height();
        let mut cursor_y = y;
        let mut current_line = String::new();
        let mut current_w = 0.0_f32;

        for word in text.split_whitespace() {
            let word_w = Self::text_width(word, font, scale.x);
            let space_w = Self::text_width(" ", font, scale.x);
            let need = if current_line.is_empty() {
                word_w
            } else {
                space_w + word_w
            };

            if current_w + need > max_w && !current_line.is_empty() {
                Self::draw_text(pm, font, &current_line, x, cursor_y, scale, color);
                cursor_y += line_h;
                current_line.clear();
                current_w = 0.0;
            }
            if !current_line.is_empty() {
                current_line.push(' ');
                current_w += space_w;
            }
            current_line.push_str(word);
            current_w += word_w;
        }
        if !current_line.is_empty() {
            Self::draw_text(pm, font, &current_line, x, cursor_y, scale, color);
        }
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
}
