//! # Пространственные порталы
//!
//! Клик по VO-порталу запускает плавную анимацию полёта камеры к
//! физическим координатам целевого RO на глобальном холсте.
//!
//! Логика:
//!   1. Найти в графе VO, который ссылается на target_ro, и его мировые координаты.
//!   2. Если их несколько — выбрать ближайший к текущей камере.
//!   3. Запустить FlyAnimation: интерполирует camera.center и camera.zoom.
//!   4. По завершении — авто-фокус на target_ro.

use canvas_engine::Camera;
use cgmath::Point2;
use tad_core::{RoId, VirtualObject};

/// Анимация полёта.
#[derive(Debug, Clone)]
pub struct FlyAnimation {
    pub start_center: Point2<f32>,
    pub start_zoom: f32,
    pub target_center: Point2<f32>,
    pub target_zoom: f32,
    pub elapsed: f32,
    pub duration: f32,
}

impl FlyAnimation {
    pub fn new(start: &Camera, target_center: Point2<f32>, target_zoom: f32) -> Self {
        Self {
            start_center: start.center,
            start_zoom: start.zoom,
            target_center,
            target_zoom,
            elapsed: 0.0,
            duration: 0.6, // 600 мс
        }
    }

    /// Выполнить шаг анимации. Возвращает Some(camera) если анимация идёт,
    /// None — если завершена.
    pub fn step(&mut self, dt: f32) -> Option<Camera> {
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            return None;
        }
        let t = self.elapsed / self.duration;
        // ease-in-out кубическая.
        let e = if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        };
        Some(Camera {
            center: Point2::new(
                self.start_center.x + (self.target_center.x - self.start_center.x) * e,
                self.start_center.y + (self.target_center.y - self.start_center.y) * e,
            ),
            zoom: self.start_zoom + (self.target_zoom - self.start_zoom) * e,
            viewport: cgmath::Vector2::new(0, 0), // подставит вызывающий
        })
    }
}

/// Найти целевую позицию для портала.
/// Ищем VO, указывающий на `target_ro`, и берём его центр.
pub fn find_portal_target(
    target_ro: RoId,
    all_vos: &[VirtualObject],
    current_center: Point2<f32>,
) -> Option<(Point2<f32>, f32)> {
    // Все VO, ссылающиеся на target_ro.
    let candidates: Vec<&VirtualObject> = all_vos
        .iter()
        .filter(|vo| vo.target_ro == target_ro)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    // Ближайший к текущей камере.
    let closest = candidates.into_iter().min_by_key(|vo| {
        let dx = vo.pos.x - current_center.x;
        let dy = vo.pos.y - current_center.y;
        ((dx * dx + dy * dy) * 1000.0) as i64
    })?;
    let center = Point2::new(
        closest.pos.x + closest.size.x * 0.5,
        closest.pos.y + closest.size.y * 0.5,
    );
    // Зум такой, чтобы VO занимал ~40% экрана.
    let target_zoom = 400.0 / closest.size.x.max(closest.size.y);
    Some((center, target_zoom))
}

/// Менеджер активной анимации.
pub struct PortalSystem {
    pub active: Option<FlyAnimation>,
    pub pending_target_ro: Option<RoId>,
}

impl Default for PortalSystem {
    fn default() -> Self {
        Self {
            active: None,
            pending_target_ro: None,
        }
    }
}

impl PortalSystem {
    /// Запустить портал к target_ro.
    pub fn fly_to(&mut self, target_ro: RoId, all_vos: &[VirtualObject], current: &Camera) -> bool {
        let Some((target_center, target_zoom)) =
            find_portal_target(target_ro, all_vos, current.center)
        else {
            return false;
        };
        let mut anim = FlyAnimation::new(current, target_center, target_zoom);
        anim.elapsed = 0.0;
        self.active = Some(anim);
        self.pending_target_ro = Some(target_ro);
        true
    }

    /// Шаг анимации. Возвращает Some(camera) если идёт, None если завершена.
    pub fn step(&mut self, dt: f32, viewport: cgmath::Vector2<u32>) -> Option<Camera> {
        if let Some(mut anim) = self.active.take() {
            if let Some(mut cam) = anim.step(dt) {
                cam.viewport = viewport;
                self.active = Some(anim);
                return Some(cam);
            } else {
                // Завершение: камера в целевой точке.
                return Some(Camera {
                    center: anim.target_center,
                    zoom: anim.target_zoom,
                    viewport,
                });
            }
        }
        None
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
}
