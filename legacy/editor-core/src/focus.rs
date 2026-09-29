//! # Система фокуса ввода
//!
//! Приближение к RO до scale > 0.8 автоматически переводит фокус на него.
//! Только один VO может быть в `Focused`-режиме одновременно.
//!
//! Распределение фокуса:
//!   - Камера → определяет кандидат-VO по scale_factor.
//!   - FocusManager → хранит текущий активный VO и шлёт события редакторам.
//!   - Редакторы → модульные микро-инструменты (текст, вектор, таблица).

use ahash::AHashMap;
use cgmath::Point2;
use tad_core::{DisplayMode, SegmentId, VirtualObject, VoId};

/// Событие ввода, маршрутизируемое focus-системой в активный редактор.
#[derive(Debug, Clone)]
pub enum InputEvent {
    Key {
        code: u32,
        modifiers: u8,
        pressed: bool,
    },
    Char(char),
    MouseDown {
        button: u8,
        x: f32,
        y: f32,
    },
    MouseUp {
        button: u8,
        x: f32,
        y: f32,
    },
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseWheel {
        delta: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    None,
    Vo(VoId),                 // фокус на весь VO
    Segment(VoId, SegmentId), // фокус на конкретный сегмент внутри VO
}

pub struct FocusManager {
    pub current: FocusTarget,
    /// История для Tab-навигации.
    history: Vec<FocusTarget>,
    /// Карта: VO → текущий выбранный сегмент (для sub-focus).
    pub vo_active_segment: AHashMap<VoId, SegmentId>,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self {
            current: FocusTarget::None,
            history: Vec::new(),
            vo_active_segment: AHashMap::new(),
        }
    }
}

impl FocusManager {
    /// Вызывается каждый кадр. Решает, нужно ли переключить фокус
    /// в зависимости от display mode VO под курсором / в центре камеры.
    pub fn update_from_camera(
        &mut self,
        culled: &[canvas_engine::CulledVo],
        cursor_world: Point2<f32>,
    ) -> Option<FocusTarget> {
        // Алгоритм:
        // 1. Если курсор над VO в режиме Focused — фокус на этот VO.
        // 2. Если в центре камеры есть Focused-VO — фокус на него.
        // 3. Иначе фокус снимается (но редактор не закрывается,
        //    чтобы можно было отдалить и увидеть контекст).

        // Сначала проверяем hit-test под курсором.
        for c in culled.iter().rev() {
            if c.display == DisplayMode::Focused && point_in_vo(cursor_world, &c.vo) {
                let new_target = FocusTarget::Vo(c.vo.id);
                if self.current != new_target {
                    self.history.push(self.current);
                    self.current = new_target;
                    return Some(new_target);
                }
                return None;
            }
        }
        None
    }

    /// Принудительный фокус (например, по двойному клику).
    pub fn force_focus(&mut self, target: FocusTarget) {
        if self.current != target {
            self.history.push(self.current);
            self.current = target;
        }
    }

    /// Tab — переход к следующему сегменту внутри текущего VO.
    pub fn tab_next(&mut self, segments: &[SegmentId]) {
        if let FocusTarget::Segment(vo, cur) = self.current {
            if let Some(idx) = segments.iter().position(|s| *s == cur) {
                let next = segments[(idx + 1) % segments.len()];
                self.current = FocusTarget::Segment(vo, next);
                self.vo_active_segment.insert(vo, next);
            }
        } else if let FocusTarget::Vo(vo) = self.current {
            if let Some(&first) = segments.first() {
                self.current = FocusTarget::Segment(vo, first);
                self.vo_active_segment.insert(vo, first);
            }
        }
    }

    pub fn blur(&mut self) {
        if self.current != FocusTarget::None {
            self.history.push(self.current);
            self.current = FocusTarget::None;
        }
    }
}

fn point_in_vo(p: Point2<f32>, vo: &VirtualObject) -> bool {
    p.x >= vo.pos.x
        && p.x <= vo.pos.x + vo.size.x * vo.scale
        && p.y >= vo.pos.y
        && p.y <= vo.pos.y + vo.size.y * vo.scale
}

/// Какой редактор подключить для данного сегмента.
pub fn editor_kind_for_segment(seg: &tad_core::Segment) -> EditorKind {
    use tad_core::Segment::*;
    match seg {
        Heading { .. } | Text { .. } => EditorKind::Text,
        Image { .. } => EditorKind::Image,
        Vector { .. } => EditorKind::Vector,
        Table { .. } => EditorKind::Table,
        Link { .. } => EditorKind::Portal,
        Custom { .. } => EditorKind::Custom,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKind {
    Text,
    Vector,
    Table,
    Image,
    Portal,
    Custom,
}
