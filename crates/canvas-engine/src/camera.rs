//! # Камера: преобразование мировых координат ↔ экранных
//!
//! Мировые координаты — бесконечная 2D-плоскость (f32).
//! Камера задаётся: центром (world_x, world_y), масштабом (zoom),
//! размером вьюпорта в пикселях.
//!
//! Важные инварианты:
//!   - zoom > 0 всегда. Минимум 0.01 ограничивает потерю точности.
//!   - Экранные координаты: origin = top-left, Y вниз.
//!   - Мировые координаты: origin = top-left, Y вниз (для простоты UI).
//!
//! Трансформация:
//!   screen = (world - camera_center) * zoom + screen_size/2
//!   world  = (screen - screen_size/2) / zoom + camera_center

use cgmath::{Point2, Vector2};

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// Центр камеры в мировых координатах.
    pub center: Point2<f32>,
    /// Масштаб: pixels per world unit.
    pub zoom: f32,
    /// Размер вьюпорта в пикселях.
    pub viewport: Vector2<u32>,
}

impl Camera {
    pub fn new(viewport: Vector2<u32>) -> Self {
        Self {
            center: Point2::new(0.0, 0.0),
            zoom: 1.0,
            viewport,
        }
    }

    /// Минимальный zoom (защита от деления на 0 и потери точности).
    pub const MIN_ZOOM: f32 = 0.01;
    /// Максимальный zoom (64x достаточно для любого LOD).
    pub const MAX_ZOOM: f32 = 64.0;

    /// Зум к точке на экране (как в Figma/Miro).
    /// `cursor_screen` остаётся под курсором, остальное масштабируется.
    pub fn zoom_at(&mut self, cursor_screen: Vector2<f32>, factor: f32) {
        let new_zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);

        // Мировая точка под курсором до зума:
        let world_under_cursor = self.screen_to_world(cursor_screen);
        // Применяем зум:
        self.zoom = new_zoom;
        // Корректируем центр так, чтобы та же мировая точка осталась под курсором:
        let new_world_under_cursor = self.screen_to_world(cursor_screen);
        self.center += world_under_cursor - new_world_under_cursor;
    }

    /// Панорамирование: сдвигаем камеру на дельту в пикселях.
    pub fn pan(&mut self, delta_screen: Vector2<f32>) {
        self.center -= delta_screen / self.zoom;
    }

    /// Плавное перемещение к новой точке (для порталов).
    /// Возвращает t∈[0,1] — долю пройденного пути.
    pub fn fly_to_step(&self, target_center: Point2<f32>, target_zoom: f32, t: f32) -> Camera {
        // ease-in-out кубическая интерполяция.
        let e = if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        };
        Camera {
            center: Point2::new(
                self.center.x + (target_center.x - self.center.x) * e,
                self.center.y + (target_center.y - self.center.y) * e,
            ),
            zoom: self.zoom + (target_zoom - self.zoom) * e,
            viewport: self.viewport,
        }
    }

    /// Экран → мир.
    pub fn screen_to_world(&self, screen: Vector2<f32>) -> Point2<f32> {
        Point2::new(
            (screen.x - self.viewport.x as f32 * 0.5) / self.zoom + self.center.x,
            (screen.y - self.viewport.y as f32 * 0.5) / self.zoom + self.center.y,
        )
    }

    /// Мир → экран.
    pub fn world_to_screen(&self, world: Point2<f32>) -> Vector2<f32> {
        Vector2::new(
            (world.x - self.center.x) * self.zoom + self.viewport.x as f32 * 0.5,
            (world.y - self.center.y) * self.zoom + self.viewport.y as f32 * 0.5,
        )
    }

    /// Видимая прямоугольная область в мировых координатах (для frustum culling).
    pub fn view_aabb_world(&self) -> Aabb {
        let half_w = self.viewport.x as f32 / (2.0 * self.zoom);
        let half_h = self.viewport.y as f32 / (2.0 * self.zoom);
        Aabb {
            min: Point2::new(self.center.x - half_w, self.center.y - half_h),
            max: Point2::new(self.center.x + half_w, self.center.y + half_h),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Point2<f32>,
    pub max: Point2<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_world_roundtrip_is_identity() {
        let mut cam = Camera::new(Vector2::new(800, 600));
        cam.center = Point2::new(120.0, -40.0);
        cam.zoom = 2.5;
        let pt = Vector2::new(133.0, 421.0);
        let back = cam.world_to_screen(cam.screen_to_world(pt));
        assert!((back.x - pt.x).abs() < 1e-3 && (back.y - pt.y).abs() < 1e-3);
    }

    #[test]
    fn zoom_at_keeps_world_point_under_cursor() {
        let mut cam = Camera::new(Vector2::new(1280, 720));
        cam.center = Point2::new(500.0, 500.0);
        let cursor = Vector2::new(900.0, 100.0);
        let before = cam.screen_to_world(cursor);
        cam.zoom_at(cursor, 1.75);
        let after = cam.screen_to_world(cursor);
        assert!((before.x - after.x).abs() < 1e-2, "{before:?} vs {after:?}");
        assert!((before.y - after.y).abs() < 1e-2);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut cam = Camera::new(Vector2::new(800, 600));
        for _ in 0..200 {
            cam.zoom_at(Vector2::new(400.0, 300.0), 1.5);
        }
        assert!(cam.zoom <= Camera::MAX_ZOOM);
        for _ in 0..400 {
            cam.zoom_at(Vector2::new(400.0, 300.0), 0.5);
        }
        assert!(cam.zoom >= Camera::MIN_ZOOM);
    }

    #[test]
    fn pan_moves_view_in_screen_pixel_terms() {
        let mut cam = Camera::new(Vector2::new(800, 600));
        cam.zoom = 2.0;
        let world = Point2::new(0.0, 0.0);
        let s0 = cam.world_to_screen(world);
        cam.pan(Vector2::new(40.0, 20.0));
        let s1 = cam.world_to_screen(world);
        // сдвиг камеры на 40px экрана = смещение точки на 40px экрана
        assert!((s1.x - (s0.x + 40.0)).abs() < 1e-3);
        assert!((s1.y - (s0.y + 20.0)).abs() < 1e-3);
    }

    #[test]
    fn view_aabb_is_viewport_size_over_zoom() {
        let mut cam = Camera::new(Vector2::new(800, 600));
        cam.zoom = 4.0;
        cam.center = Point2::new(100.0, 100.0);
        let v = cam.view_aabb_world();
        assert!((v.max.x - v.min.x - 200.0).abs() < 1e-3);
        assert!((v.max.y - v.min.y - 150.0).abs() < 1e-3);
        assert!(v.contains(Point2::new(100.0, 100.0)));
    }
}

impl Aabb {
    pub fn contains(&self, p: Point2<f32>) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// Пересечение с другим AABB (для culling).
    pub fn intersects(&self, other: &Aabb) -> bool {
        !(other.max.x < self.min.x
            || other.min.x > self.max.x
            || other.max.y < self.min.y
            || other.min.y > self.max.y)
    }

    pub fn from_rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            min: Point2::new(x, y),
            max: Point2::new(x + w, y + h),
        }
    }
}
