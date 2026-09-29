//! # CPU-рендерер бесконечного холста
//!
//! Использует `tiny-skia` (CPU 2D-растеризатор) + `ab_glyph` (растеризация TTF).
//! Работает headless — идеально для screenshot-бинарника, а также как
//! фолбэк-рендерер в рантайме, когда wgpu недоступен.
//!
//! Конвейер кадра:
//!   1. Очистить фон тёмно-серым.
//!   2. Для каждого CulledVo (по LOD):
//!      - Icon:     цветной прямоугольник + заголовок шрифтом.
//!      - Preview:  прямоугольник с лёгкой прозрачностью + заголовок.
//!      - Live:     рисуем сегменты TAD — текст, таблицы, вектор, ссылки.
//!      - Focused:  то же что Live + жёлтая рамка.
//!   3. Заголовок VO + (опц.) FPS.

use crate::fonts::FontCache;
use crate::painter::Painter;
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use ahash::AHashMap;
use canvas_engine::{Camera, CulledVo};
use cgmath::Point2;
use std::collections::HashMap;
use tad_core::{DisplayMode, RealObject, RoId, Segment, VectorKind};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

pub mod fonts;
pub mod painter;
/// Результат рендера: pixmap + метаданные.
pub struct CanvasFrame {
    pub pixmap: Pixmap,
    pub visible_vos: usize,
    pub fps: f32,
}

pub struct SkiaRenderer {
    pub width: u32,
    pub height: u32,
    pub fonts: FontCache,
    pub preview_cache: AHashMap<RoId, Pixmap>,
}

impl SkiaRenderer {
    pub fn new(width: u32, height: u32) -> anyhow::Result<Self> {
        let fonts = FontCache::load_default()?;
        Ok(Self {
            width,
            height,
            fonts,
            preview_cache: AHashMap::new(),
        })
    }

    /// Главный кадр: рисует всё холст в Pixmap.
    pub fn render(
        &mut self,
        camera: &Camera,
        culled: &[CulledVo],
        ro_index: &HashMap<RoId, RealObject>,
    ) -> CanvasFrame {
        let mut pm = Pixmap::new(self.width, self.height).unwrap();

        // 1. Фон.
        let mut bg = Paint::default();
        bg.set_color_rgba8(0x1a, 0x1b, 0x20, 0xff);
        pm.fill_rect(
            Rect::from_xywh(0.0, 0.0, self.width as f32, self.height as f32).unwrap(),
            &bg,
            Transform::identity(),
            None,
        );

        // 2. Сетка фона (для ощущения бесконечного пространства).
        self.draw_grid(&mut pm, camera);

        // 3. Все VO.
        let mut visible = 0;
        for c in culled {
            if let Some(ro) = ro_index.get(&c.vo.target_ro) {
                self.draw_vo(&mut pm, c, ro, camera);
                visible += 1;
            }
        }

        // 4. HUD: координаты камеры + зум.
        self.draw_hud(&mut pm, camera, visible);

        CanvasFrame {
            pixmap: pm,
            visible_vos: visible,
            fps: 0.0,
        }
    }

    fn draw_grid(&self, pm: &mut Pixmap, camera: &Camera) {
        // Размер ячейки в мировых координатах.
        let world_step = 100.0_f32;
        // В пикселях на текущем зуме:
        let px_step = world_step * camera.zoom;
        if px_step < 8.0 {
            return;
        } // слишком плотно

        let view = camera.view_aabb_world();
        let start_x = (view.min.x / world_step).floor() * world_step;
        let start_y = (view.min.y / world_step).floor() * world_step;

        let mut paint = Paint::default();
        paint.set_color_rgba8(0x2a, 0x2c, 0x33, 0xff);

        let mut x = start_x;
        while x <= view.max.x {
            let sx = camera.world_to_screen(Point2::new(x, 0.0)).x;
            let path = make_line_path(sx, 0.0, sx, self.height as f32);
            pm.stroke_path(
                &path,
                &paint,
                &Stroke::default(),
                Transform::identity(),
                None,
            );
            x += world_step;
        }
        let mut y = start_y;
        while y <= view.max.y {
            let sy = camera.world_to_screen(Point2::new(0.0, y)).y;
            let path = make_line_path(0.0, sy, self.width as f32, sy);
            pm.stroke_path(
                &path,
                &paint,
                &Stroke::default(),
                Transform::identity(),
                None,
            );
            y += world_step;
        }
    }

    fn draw_vo(&self, pm: &mut Pixmap, c: &CulledVo, ro: &RealObject, camera: &Camera) {
        let screen = &c.screen_rect;
        // Возможно, VO частично за экраном — обрежем для tiny-skia.
        if screen.w < 1.0 || screen.h < 1.0 {
            return;
        }

        let top_left = camera.world_to_screen(c.vo.pos);
        let x = top_left.x;
        let y = top_left.y;
        let w = c.vo.size.x * c.scale_factor;
        let h = c.vo.size.y * c.scale_factor;

        // Фон VO.
        let bg = self.bg_color_for(ro, c.display);
        let mut paint = Paint::default();
        paint.set_color_rgba8(bg[0], bg[1], bg[2], bg[3]);
        if let Some(rect) = Rect::from_xywh(x, y, w, h) {
            pm.fill_rect(rect, &paint, Transform::identity(), None);
        }

        // Рамка фокуса.
        if c.display == DisplayMode::Focused {
            self.draw_rect_border(pm, x, y, w, h, [0xff, 0xd7, 0x3a, 0xff], 2.5);
        } else {
            self.draw_rect_border(pm, x, y, w, h, [0x55, 0x58, 0x62, 0xff], 1.0);
        }

        // Контент по LOD.
        match c.display {
            DisplayMode::Icon => {
                self.draw_icon_mode(pm, x, y, w, h, ro);
            }
            DisplayMode::Preview => {
                self.draw_preview_mode(pm, x, y, w, h, ro);
            }
            DisplayMode::Live | DisplayMode::Focused => {
                self.draw_live_mode(pm, x, y, w, h, ro, c.scale_factor, c.display);
            }
        }

        // Заголовок VO (всегда виден сверху).
        self.draw_title_bar(pm, x, y, w, ro);
    }

    fn draw_icon_mode(&self, pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, ro: &RealObject) {
        // Иконка по типу объекта.
        let icon = self.icon_char_for(ro);
        let font = &self.fonts.bold;
        let size = w.min(h) * 0.5;
        let scale = PxScale::from(size);
        let scaled = font.as_scaled(scale);
        let advance = scaled.h_advance(scaled.scaled_glyph(icon).id);
        let tx = x + (w - advance) * 0.5;
        let ty = y + h * 0.5 + size * 0.35;
        self.paint_text(
            pm,
            &icon.to_string(),
            tx,
            ty,
            scale,
            [0xff, 0xff, 0xff, 0xff],
            font,
        );
    }

    fn draw_preview_mode(&self, pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, _ro: &RealObject) {
        // Полупрозрачная «тень» содержимого — рисуем упрощённо.
        let mut paint = Paint::default();
        paint.set_color_rgba8(0xff, 0xff, 0xff, 0x18);
        if let Some(rect) = Rect::from_xywh(x + 4.0, y + 24.0, w - 8.0, h - 28.0) {
            pm.fill_rect(rect, &paint, Transform::identity(), None);
        }
        // Несколько «строк текста» как полосы.
        for i in 0..5 {
            let line_y = y + 32.0 + (i as f32) * 8.0;
            if line_y > y + h - 8.0 {
                break;
            }
            let mut p = Paint::default();
            p.set_color_rgba8(0xaa, 0xae, 0xb8, 0x80);
            let lw = (w - 16.0) * (1.0 - i as f32 * 0.12);
            if let Some(r) = Rect::from_xywh(x + 8.0, line_y, lw, 2.0) {
                pm.fill_rect(r, &p, Transform::identity(), None);
            }
        }
    }

    fn draw_live_mode(
        &self,
        pm: &mut Pixmap,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        ro: &RealObject,
        sf: f32,
        mode: DisplayMode,
    ) {
        for seg in &ro.document.root_segments {
            let sx = x + seg.x * sf;
            let sy = y + seg.y * sf;
            let sw = seg.w * sf;
            let sh = seg.h * sf;
            match &seg.segment {
                Segment::Heading { level, text } => {
                    let size = match level {
                        1 => 24.0,
                        2 => 20.0,
                        _ => 16.0,
                    } * sf;
                    self.paint_text(
                        pm,
                        text,
                        sx,
                        sy + size,
                        PxScale::from(size),
                        [0xff, 0xff, 0xff, 0xff],
                        &self.fonts.bold,
                    );
                }
                Segment::Text { text } => {
                    let size = 14.0 * sf;
                    // Перенос строк по ширине.
                    self.paint_wrapped_text(
                        pm,
                        text,
                        sx,
                        sy + size,
                        sw,
                        PxScale::from(size),
                        [0xd0, 0xd3, 0xda, 0xff],
                        &self.fonts.regular,
                    );
                }
                Segment::Table { rows } => {
                    let row_h = (sh / rows.len().max(1) as f32).max(8.0);
                    for (i, row) in rows.iter().enumerate() {
                        let col_w = sw / row.len().max(1) as f32;
                        for (j, cell) in row.iter().enumerate() {
                            let cx = sx + j as f32 * col_w;
                            let cy = sy + i as f32 * row_h;
                            // Фон ячейки (зебра).
                            let mut p = Paint::default();
                            if i == 0 {
                                p.set_color_rgba8(0x33, 0x55, 0x88, 0xff);
                            } else if i % 2 == 0 {
                                p.set_color_rgba8(0x22, 0x24, 0x29, 0xff);
                            } else {
                                p.set_color_rgba8(0x1d, 0x1f, 0x24, 0xff);
                            }
                            if let Some(r) = Rect::from_xywh(cx, cy, col_w, row_h) {
                                pm.fill_rect(r, &p, Transform::identity(), None);
                            }
                            // Текст ячейки.
                            self.paint_text(
                                pm,
                                cell,
                                cx + 4.0,
                                cy + row_h * 0.7,
                                PxScale::from(row_h.min(14.0 * sf) * 0.7),
                                if i == 0 {
                                    [0xff, 0xff, 0xff, 0xff]
                                } else {
                                    [0xd0, 0xd3, 0xda, 0xff]
                                },
                                if i == 0 {
                                    &self.fonts.bold
                                } else {
                                    &self.fonts.regular
                                },
                            );
                            // Сетка.
                            self.draw_rect_border(
                                pm,
                                cx,
                                cy,
                                col_w,
                                row_h,
                                [0x44, 0x47, 0x52, 0xff],
                                1.0,
                            );
                        }
                    }
                }
                Segment::Vector { shapes } => {
                    for sh in shapes {
                        self.draw_vector_shape(pm, sh, sx, sy, sf);
                    }
                }
                Segment::Link { label, .. } => {
                    let size = 14.0 * sf;
                    self.paint_text(
                        pm,
                        ">>",
                        sx,
                        sy + size,
                        PxScale::from(size),
                        [0xff, 0xd7, 0x3a, 0xff],
                        &self.fonts.bold,
                    );
                    self.paint_text(
                        pm,
                        label,
                        sx + 24.0 * sf,
                        sy + size,
                        PxScale::from(size),
                        [0xff, 0xd7, 0x3a, 0xff],
                        &self.fonts.regular,
                    );
                    // Подчёркивание.
                    let mut p = Paint::default();
                    p.set_color_rgba8(0xff, 0xd7, 0x3a, 0xff);
                    if let Some(r) = Rect::from_xywh(sx, sy + size + 2.0, sw, 1.0) {
                        pm.fill_rect(r, &p, Transform::identity(), None);
                    }
                }
                Segment::Image {
                    blob_ref: _,
                    width,
                    height,
                } => {
                    let mut p = Paint::default();
                    p.set_color_rgba8(0x55, 0x35, 0x75, 0xff);
                    if let Some(r) =
                        Rect::from_xywh(sx, sy, *width as f32 * sf, *height as f32 * sf)
                    {
                        pm.fill_rect(r, &p, Transform::identity(), None);
                    }
                    self.paint_text(
                        pm,
                        "[Image]",
                        sx + 8.0,
                        sy + 16.0,
                        PxScale::from(12.0 * sf),
                        [0xff, 0xff, 0xff, 0xff],
                        &self.fonts.regular,
                    );
                }
                Segment::Custom { .. } => {}
            }
        }
        // Если фокус — рисуем курсор в активном текстовом сегменте.
        if mode == DisplayMode::Focused {
            // (только визуальная подсказка — реальный курсор редактора рисуется отдельно)
        }
    }

    fn draw_vector_shape(
        &self,
        pm: &mut Pixmap,
        sh: &tad_core::VectorShape,
        ox: f32,
        oy: f32,
        sf: f32,
    ) {
        if let Some(f) = sh.fill {
            let mut paint = Paint::default();
            paint.set_color_rgba8(
                (f[0] * 255.0) as u8,
                (f[1] * 255.0) as u8,
                (f[2] * 255.0) as u8,
                (f[3] * 255.0) as u8,
            );
            let mut pb = PathBuilder::new();
            match &sh.kind {
                VectorKind::Rect { x, y, w, h } => {
                    pb.move_to(ox + x * sf, oy + y * sf);
                    pb.line_to(ox + (x + w) * sf, oy + y * sf);
                    pb.line_to(ox + (x + w) * sf, oy + (y + h) * sf);
                    pb.line_to(ox + x * sf, oy + (y + h) * sf);
                    pb.close();
                }
                VectorKind::Ellipse { cx, cy, rx, ry } => {
                    let cx = ox + cx * sf;
                    let cy = oy + cy * sf;
                    let rx = rx * sf;
                    let ry = ry * sf;
                    pb.move_to(cx - rx, cy);
                    pb.cubic_to(
                        cx - rx,
                        cy - ry * 0.5523,
                        cx - rx * 0.5523,
                        cy - ry,
                        cx,
                        cy - ry,
                    );
                    pb.cubic_to(
                        cx + rx * 0.5523,
                        cy - ry,
                        cx + rx,
                        cy - ry * 0.5523,
                        cx + rx,
                        cy,
                    );
                    pb.cubic_to(
                        cx + rx,
                        cy + ry * 0.5523,
                        cx + rx * 0.5523,
                        cy + ry,
                        cx,
                        cy + ry,
                    );
                    pb.cubic_to(
                        cx - rx * 0.5523,
                        cy + ry,
                        cx - rx,
                        cy + ry * 0.5523,
                        cx - rx,
                        cy,
                    );
                    pb.close();
                }
                _ => {}
            }
            if let Some(path) = pb.finish() {
                pm.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        if let Some(s) = sh.stroke {
            let mut paint = Paint::default();
            paint.set_color_rgba8(
                (s[0] * 255.0) as u8,
                (s[1] * 255.0) as u8,
                (s[2] * 255.0) as u8,
                (s[3] * 255.0) as u8,
            );
            let mut pb = PathBuilder::new();
            let width = match &sh.kind {
                VectorKind::Line { x1, y1, x2, y2, .. } => {
                    pb.move_to(ox + x1 * sf, oy + y1 * sf);
                    pb.line_to(ox + x2 * sf, oy + y2 * sf);
                    1.5
                }
                VectorKind::Rect { x, y, w, h } => {
                    pb.move_to(ox + x * sf, oy + y * sf);
                    pb.line_to(ox + (x + w) * sf, oy + y * sf);
                    pb.line_to(ox + (x + w) * sf, oy + (y + h) * sf);
                    pb.line_to(ox + x * sf, oy + (y + h) * sf);
                    pb.close();
                    1.0
                }
                VectorKind::Ellipse { cx, cy, rx, ry } => {
                    let cx = ox + cx * sf;
                    let cy = oy + cy * sf;
                    let rx = rx * sf;
                    let ry = ry * sf;
                    pb.move_to(cx - rx, cy);
                    pb.cubic_to(
                        cx - rx,
                        cy - ry * 0.5523,
                        cx - rx * 0.5523,
                        cy - ry,
                        cx,
                        cy - ry,
                    );
                    pb.cubic_to(
                        cx + rx * 0.5523,
                        cy - ry,
                        cx + rx,
                        cy - ry * 0.5523,
                        cx + rx,
                        cy,
                    );
                    pb.cubic_to(
                        cx + rx,
                        cy + ry * 0.5523,
                        cx + rx * 0.5523,
                        cy + ry,
                        cx,
                        cy + ry,
                    );
                    pb.cubic_to(
                        cx - rx * 0.5523,
                        cy + ry,
                        cx - rx,
                        cy + ry * 0.5523,
                        cx - rx,
                        cy,
                    );
                    pb.close();
                    1.0
                }
                VectorKind::Text { x, y, text, size } => {
                    self.paint_text(
                        pm,
                        text,
                        ox + x * sf,
                        oy + y * sf,
                        PxScale::from(size * sf),
                        [0xff, 0xff, 0xff, 0xff],
                        &self.fonts.regular,
                    );
                    return;
                }
                VectorKind::Path { points, .. } => {
                    if let Some(&(fx, fy)) = points.first() {
                        pb.move_to(ox + fx * sf, oy + fy * sf);
                        for &(px, py) in points.iter().skip(1) {
                            pb.line_to(ox + px * sf, oy + py * sf);
                        }
                    }
                    1.5
                }
            };
            if let Some(path) = pb.finish() {
                let stroke = Stroke {
                    width,
                    ..Default::default()
                };
                pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
    }

    fn draw_title_bar(&self, pm: &mut Pixmap, x: f32, y: f32, w: f32, ro: &RealObject) {
        // Тёмная полоска сверху.
        let mut p = Paint::default();
        p.set_color_rgba8(0x0e, 0x0f, 0x12, 0xe0);
        if let Some(r) = Rect::from_xywh(x, y - 18.0, w, 18.0) {
            pm.fill_rect(r, &p, Transform::identity(), None);
        }
        let icon = self.icon_char_for(ro);
        self.paint_text(
            pm,
            &format!("{} {}", icon, ro.title),
            x + 6.0,
            y - 4.0,
            PxScale::from(13.0),
            [0xff, 0xff, 0xff, 0xff],
            &self.fonts.bold,
        );
    }

    fn draw_rect_border(
        &self,
        pm: &mut Pixmap,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 4],
        width: f32,
    ) {
        let mut p = Paint::default();
        p.set_color_rgba8(color[0], color[1], color[2], color[3]);
        // 4 стороны.
        let sides = [
            Rect::from_xywh(x, y, w, width).unwrap(),
            Rect::from_xywh(x, y + h - width, w, width).unwrap(),
            Rect::from_xywh(x, y, width, h).unwrap(),
            Rect::from_xywh(x + w - width, y, width, h).unwrap(),
        ];
        for r in sides {
            pm.fill_rect(r, &p, Transform::identity(), None);
        }
    }

    fn draw_hud(&self, pm: &mut Pixmap, camera: &Camera, visible: usize) {
        let lines = [
            format!(
                "ZUI-TAD Shell  —  cam ({:.0}, {:.0})  zoom {:.2}",
                camera.center.x, camera.center.y, camera.zoom
            ),
            format!("Visible VOs: {}", visible),
            format!("Viewport: {}x{}", camera.viewport.x, camera.viewport.y),
        ];
        let mut p = Paint::default();
        p.set_color_rgba8(0x00, 0x00, 0x00, 0xc0);
        if let Some(r) = Rect::from_xywh(8.0, 8.0, 360.0, 16.0 * lines.len() as f32 + 12.0) {
            pm.fill_rect(r, &p, Transform::identity(), None);
        }
        for (i, line) in lines.iter().enumerate() {
            self.paint_text(
                pm,
                line,
                16.0,
                24.0 + i as f32 * 16.0,
                PxScale::from(13.0),
                [0xff, 0xff, 0xff, 0xff],
                &self.fonts.mono,
            );
        }
    }

    fn paint_text(
        &self,
        pm: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        scale: PxScale,
        color: [u8; 4],
        font: &FontVec,
    ) {
        Painter::draw_text(pm, font, text, x, y, scale, color);
    }

    fn paint_wrapped_text(
        &self,
        pm: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        max_w: f32,
        scale: PxScale,
        color: [u8; 4],
        font: &FontVec,
    ) {
        Painter::draw_wrapped_text(pm, font, text, x, y, max_w, scale, color);
    }

    fn bg_color_for(&self, ro: &RealObject, mode: DisplayMode) -> [u8; 4] {
        use tad_core::ObjectKind::*;
        let base: [u8; 4] = match ro.kind {
            Text => [0x1e, 0x2a, 0x4a, 0xff],
            Mindmap => [0x4a, 0x1e, 0x1e, 0xff],
            Table => [0x1e, 0x4a, 0x2a, 0xff],
            Image => [0x4a, 0x1e, 0x4a, 0xff],
            Portal => [0x4a, 0x3a, 0x1e, 0xff],
            WaylandWindow => [0x33, 0x33, 0x38, 0xff],
            Folder => [0x22, 0x22, 0x28, 0xff],
            Files => [0x1e, 0x3a, 0x4a, 0xff],
            Calculator => [0x3a, 0x2a, 0x1e, 0xff],
            Settings => [0x2a, 0x2a, 0x4a, 0xff],
        };
        match mode {
            DisplayMode::Icon => base,
            DisplayMode::Preview => [base[0] / 2, base[1] / 2, base[2] / 2, 0xff],
            DisplayMode::Live => [0x14, 0x16, 0x1c, 0xff],
            DisplayMode::Focused => [0x1a, 0x1d, 0x25, 0xff],
        }
    }

    fn icon_char_for(&self, ro: &RealObject) -> char {
        use tad_core::ObjectKind::*;
        match ro.kind {
            Text => 'T',
            Mindmap => 'M',
            Table => '#',
            Image => 'I',
            Portal => '>',
            WaylandWindow => 'W',
            Folder => 'F',
            Files => 'f',
            Calculator => 'C',
            Settings => 'S',
        }
    }
}

fn make_line_path(x1: f32, y1: f32, x2: f32, y2: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    pb.finish().unwrap()
}
