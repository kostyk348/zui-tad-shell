//! # Frustum culling + Semantic LOD
//!
//! Перед каждым кадром движок вызывает `cull()` — из всех VO сцены выбирает
//! только те, что попадают во вьюпорт камеры, и для каждого определяет
//! DisplayMode по Scale Factor (см. tad-core::DisplayMode::from_scale).
//!
//! Scale Factor — это «насколько крупно виден VO относительно своего
//! натурального размера». Если VO имеет размер 800×600 док-единиц и
//! отображается в 80×60 пикселей — scale = 0.1.

use crate::camera::{Aabb, Camera};
use cgmath::Point2;
use tad_core::{DisplayMode, VirtualObject};

/// Результат culling'а для одного VO.
#[derive(Debug, Clone)]
pub struct CulledVo {
    pub vo: VirtualObject,
    pub display: DisplayMode,
    /// Экранные координаты (для рендерера).
    pub screen_rect: ScreenRect,
    pub scale_factor: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Главная функция: фильтрует и классифицирует VO.
pub fn cull<'a>(camera: &Camera, vos: &'a [VirtualObject]) -> Vec<CulledVo> {
    let view = camera.view_aabb_world();
    let mut out = Vec::with_capacity(64);

    for vo in vos {
        // AABB VO в мировых координатах.
        let vo_aabb = Aabb::from_rect(
            vo.pos.x,
            vo.pos.y,
            vo.size.x * vo.scale,
            vo.size.y * vo.scale,
        );

        if !view.intersects(&vo_aabb) {
            continue;
        }

        // Scale factor: сколько пикселей на 1 док-единицу после применения zoom.
        let scale_factor = vo.scale * camera.zoom;
        let display = DisplayMode::from_scale(scale_factor);

        // Экранный прямоугольник (с обрезкой по вьюпорту для сверх-больших VO).
        let top_left = camera.world_to_screen(vo.pos);
        let screen_rect = ScreenRect {
            x: top_left.x,
            y: top_left.y,
            w: vo.size.x * scale_factor,
            h: vo.size.y * scale_factor,
        };

        out.push(CulledVo {
            vo: vo.clone(),
            display,
            screen_rect,
            scale_factor,
        });
    }

    // Сортируем по z-индексу для корректного наложения.
    out.sort_by_key(|c| c.vo.z);
    out
}

/// Найти VO под курсором (для hit-testing).
/// Идём с конца (верхние по z — последние в списке).
pub fn hit_test(culled: &[CulledVo], cursor: Point2<f32>) -> Option<&CulledVo> {
    culled.iter().rev().find(|c| {
        cursor.x >= c.screen_rect.x
            && cursor.x <= c.screen_rect.x + c.screen_rect.w
            && cursor.y >= c.screen_rect.y
            && cursor.y <= c.screen_rect.y + c.screen_rect.h
    })
}
