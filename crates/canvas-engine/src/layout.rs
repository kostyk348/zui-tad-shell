//! # Layout: холст → физический экран
//!
//! Композитор не хранит экранные координаты окон — он каждый кадр раскладывает
//! `Scene` через `Camera` в список квадов. Это единственное место, где живёт
//! эта математика (тестируется без GPU/Wayland).
//!
//! Правила (совпадают с driftwm):
//!   * Normal/Widget: квад = мир→экран, размер × zoom; вне вьюпорта — culled.
//!   * PinnedToScreen: позиция уже в экранных px, zoom не применяется, не culled.
//!   * Порядок отрисовки = по возрастанию z (виджеты снизу).
//!   * suspended окно всё ещё раскладывается: на его месте рисуется плейсхолдер.

use crate::camera::Camera;
use crate::scene::{CanvasWindow, Place, Scene, WindowId};
use cgmath::{Point2, Vector2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Quad {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn intersects(&self, vw: f32, vh: f32) -> bool {
        self.right() > 0.0 && self.bottom() > 0.0 && self.x < vw && self.y < vh
    }

    /// Пересечение с вьюпортом (для отсечения при рендере).
    pub fn clip(&self, vw: f32, vh: f32) -> Option<Quad> {
        let x = self.x.max(0.0);
        let y = self.y.max(0.0);
        let r = self.right().min(vw);
        let b = self.bottom().min(vh);
        if r <= x || b <= y {
            return None;
        }
        Some(Quad {
            x,
            y,
            w: r - x,
            h: b - y,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowQuad {
    pub id: WindowId,
    pub quad: Quad,
    /// Итоговый масштаб содержимого (== zoom камеры для canvas-окон).
    pub zoom: f32,
    pub z: i32,
    pub pinned: bool,
    pub suspended: bool,
    /// Полностью ли окно внутри вьюпорта (false → нужен клип).
    pub fully_visible: bool,
}

/// Разложить сцену на кадр. Порядок — отрисовки (по z).
pub fn layout_windows(scene: &Scene, camera: &Camera, viewport: Vector2<u32>) -> Vec<WindowQuad> {
    let vw = viewport.x as f32;
    let vh = viewport.y as f32;

    let mut windows: Vec<&CanvasWindow> = scene.windows().iter().collect();
    windows.sort_by_key(|w| w.z);

    let mut out = Vec::with_capacity(windows.len());
    for w in windows {
        if w.place == Place::PinnedToScreen {
            out.push(WindowQuad {
                id: w.id,
                quad: Quad {
                    x: w.pos.x,
                    y: w.pos.y,
                    w: w.size.x,
                    h: w.size.y,
                },
                zoom: 1.0,
                z: w.z,
                pinned: true,
                suspended: w.suspended,
                fully_visible: true,
            });
            continue;
        }

        let zoom = camera.zoom;
        let tl = camera.world_to_screen(w.pos);
        let quad = Quad {
            x: tl.x,
            y: tl.y,
            w: w.size.x * zoom,
            h: w.size.y * zoom,
        };
        if !quad.intersects(vw, vh) {
            continue;
        }
        let fully = quad.x >= 0.0 && quad.y >= 0.0 && quad.right() <= vw && quad.bottom() <= vh;
        out.push(WindowQuad {
            id: w.id,
            quad,
            zoom,
            z: w.z,
            pinned: false,
            suspended: w.suspended,
            fully_visible: fully,
        });
    }
    out
}

/// Экранная точка → мировая (курсор для hit-test).
pub fn screen_to_canvas(camera: &Camera, screen: Point2<f32>) -> Point2<f32> {
    camera.screen_to_world(cgmath::Vector2::new(screen.x, screen.y))
}

/// Мировая точка → экранная (позиция курсора).
pub fn canvas_to_screen(camera: &Camera, canvas: Point2<f32>) -> Point2<f32> {
    let v = camera.world_to_screen(canvas);
    Point2::new(v.x, v.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Place;
    use cgmath::{Point2, Vector2};

    fn vp(x: u32, y: u32) -> Vector2<u32> {
        Vector2::new(x, y)
    }
    fn p(x: f32, y: f32) -> Point2<f32> {
        Point2::new(x, y)
    }
    fn v(x: f32, y: f32) -> Vector2<f32> {
        Vector2::new(x, y)
    }

    fn cam(center: Point2<f32>, zoom: f32) -> Camera {
        let mut c = Camera::new(vp(800, 600));
        c.center = center;
        c.zoom = zoom;
        c
    }

    #[test]
    fn zoom_scales_quad_size() {
        let mut s = Scene::new();
        let id = s.insert("t", "T", p(0.0, 0.0), v(200.0, 100.0), Place::Normal);
        let q = layout_windows(&s, &cam(p(0.0, 0.0), 2.0), vp(800, 600));
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].id, id);
        assert!((q[0].quad.w - 400.0).abs() < 1e-3);
        assert!((q[0].quad.h - 200.0).abs() < 1e-3);
        assert!((q[0].zoom - 2.0).abs() < 1e-3);
    }

    #[test]
    fn offscreen_window_is_culled() {
        let mut s = Scene::new();
        s.insert(
            "t",
            "T",
            p(50_000.0, 50_000.0),
            v(100.0, 100.0),
            Place::Normal,
        );
        assert!(layout_windows(&s, &cam(p(0.0, 0.0), 1.0), vp(800, 600)).is_empty());
    }

    #[test]
    fn partially_visible_window_is_kept_and_clipped() {
        let mut s = Scene::new();
        // центр камеры (0,0), вьюпорт 800x600 → мир [-400,-300 .. 400,300]
        s.insert("t", "T", p(-450.0, -100.0), v(200.0, 100.0), Place::Normal);
        let q = layout_windows(&s, &cam(p(0.0, 0.0), 1.0), vp(800, 600));
        assert_eq!(q.len(), 1);
        assert!(!q[0].fully_visible);
        let clip = q[0].quad.clip(800.0, 600.0).unwrap();
        assert!((clip.x - 0.0).abs() < 1e-3);
        assert!((clip.w - 150.0).abs() < 1e-3);
    }

    #[test]
    fn pinned_to_screen_ignores_camera_and_survives_culling() {
        let mut s = Scene::new();
        let id = s.insert(
            "mpv",
            "PiP",
            p(40.0, 40.0),
            v(320.0, 180.0),
            Place::PinnedToScreen,
        );
        let q = layout_windows(&s, &cam(p(9999.0, 9999.0), 8.0), vp(800, 600));
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].id, id);
        assert!(q[0].pinned);
        assert!((q[0].quad.x - 40.0).abs() < 1e-3);
        assert!((q[0].quad.w - 320.0).abs() < 1e-3);
        assert!((q[0].zoom - 1.0).abs() < 1e-3);
    }

    #[test]
    fn paint_order_is_by_z_widgets_first() {
        let mut s = Scene::new();
        let widget = s.insert("clock", "C", p(10.0, 10.0), v(100.0, 30.0), Place::Widget);
        let win = s.insert("term", "T", p(10.0, 10.0), v(400.0, 300.0), Place::Normal);
        let q = layout_windows(&s, &cam(p(200.0, 150.0), 1.0), vp(800, 600));
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].id, widget, "widget рисуется первым (снизу)");
        assert_eq!(q[1].id, win);
    }

    #[test]
    fn suspended_window_keeps_its_slot() {
        let mut s = Scene::new();
        let id = s.insert("t", "T", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        s.suspend(id);
        let q = layout_windows(&s, &cam(p(50.0, 50.0), 1.0), vp(800, 600));
        assert_eq!(q.len(), 1);
        assert!(q[0].suspended);
    }

    #[test]
    fn all_windows_inside_viewport_are_fully_visible() {
        let mut s = Scene::new();
        s.insert("a", "A", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        let q = layout_windows(&s, &cam(p(50.0, 50.0), 1.0), vp(800, 600));
        assert!(q[0].fully_visible);
    }

    #[test]
    fn canvas_screen_roundtrip_matches_camera() {
        let c = cam(p(123.0, -77.0), 1.5);
        let world = p(400.0, 200.0);
        let scr = canvas_to_screen(&c, world);
        let back = screen_to_canvas(&c, scr);
        assert!((back.x - world.x).abs() < 1e-2 && (back.y - world.y).abs() < 1e-2);
    }
}
