//! # Canvas Scene — окна живут на бесконечном холсте, экран = камера
//!
//! Модель как в driftwm/niri, но поверх нашей RO/VO-модели:
//! окно (Wayland toplevel) = элемент холста с НАТИВНЫМ размером и позицией
//! в мировых координатах. Камера (`camera.rs`) показывает часть холста.
//!
//! Здесь только ЧИСТАЯ логика — без Wayland, без GPU, без рендерера.
//! Всё, что ниже, проверяется юнит-тестами на хосте.
//!
//! Инварианты:
//!   * `z` уникален по сцене: raise() всегда кладёт окно выше всех.
//!   * `mru[0]` — текущий фокус; raise() перемещает окно в голову.
//!   * кластер существует ⇔ в нём ≥ 2 окна. Уехало в одиночку — кластер распался.
//!   * `Widget` рисуется ниже окон и не участвует в Alt-Tab/навигации.
//!   * `PinnedToScreen` игнорирует pan/zoom и не участвует в canvas hit-test.

use crate::camera::Aabb;
use cgmath::{InnerSpace, Point2, Vector2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type WindowId = u64;
pub type ClusterId = u64;

/// Порог «касания» при формировании кластера (px).
const TOUCH_EPS: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// Как окно живёт на холсте.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Place {
    /// Обычное окно: pan/zoom, Alt-Tab, snapping.
    Normal,
    /// Виджет: закреплён к холсту, ниже окон, вне Alt-Tab (часы, трей).
    Widget,
    /// Закреплено к экрану: игнорирует pan/zoom, выше окон (PiP, тулбары).
    PinnedToScreen,
}

#[derive(Debug, Clone)]
pub struct CanvasWindow {
    pub id: WindowId,
    pub app_id: String,
    pub title: String,
    /// Мировая позиция ЛЕВОГО-ВЕРХНЕГО угла, в логических px при zoom 1.
    pub pos: Point2<f32>,
    /// Нативный размер окна (логические px).
    pub size: Vector2<f32>,
    pub z: i32,
    pub cluster: Option<ClusterId>,
    pub place: Place,
    /// Окно «усыплено»: приложение закрыто, на холсте остался плейсхолдер.
    pub suspended: bool,
}

impl CanvasWindow {
    pub fn aabb(&self) -> Aabb {
        Aabb::from_rect(self.pos.x, self.pos.y, self.size.x, self.size.y)
    }

    pub fn center(&self) -> Point2<f32> {
        Point2::new(
            self.pos.x + self.size.x * 0.5,
            self.pos.y + self.size.y * 0.5,
        )
    }

    pub fn rect(&self) -> (f32, f32, f32, f32) {
        (self.pos.x, self.pos.y, self.size.x, self.size.y)
    }

    fn is_navigable(&self) -> bool {
        self.place == Place::Normal && !self.suspended
    }
}

/// Результат применения snap к перемещаемому окну.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    /// Итоговый сдвиг, применённый к окну (после прилипания).
    pub dx: f32,
    pub dy: f32,
    /// Окна, к которым прилипли (образовали/продлили кластер).
    pub neighbors: Vec<WindowId>,
    /// Итоговый кластер (None — окно стоит само).
    pub cluster: Option<ClusterId>,
}

/// План «fit window» (аналог maximize в driftwm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitPlan {
    /// Куда поставить камеру.
    pub camera_center: Point2<f32>,
    /// Прицел зума (всегда 1.0 — окно РАСТЯГИВАЕТСЯ, а не холст).
    pub zoom: f32,
    /// Новые мировая позиция и размер окна.
    pub window_pos: Point2<f32>,
    pub window_size: Vector2<f32>,
}

/// Куда летит камера при zoom-to-fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewPlan {
    pub camera_center: Point2<f32>,
    pub zoom: f32,
}

pub struct Scene {
    windows: Vec<CanvasWindow>,
    next_id: WindowId,
    next_cluster: ClusterId,
    /// Recency-порядок. Голова = фокус.
    mru: Vec<WindowId>,
    /// Именованные точки холста — цели направленного прыжка (driftwm anchors).
    pub anchors: Vec<(String, Point2<f32>)>,
    /// 4 позиции камеры (driftwm bookmarks).
    pub bookmarks: [Option<Point2<f32>>; 4],
    /// Точка «дома» (origin).
    pub origin: Point2<f32>,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            next_id: 1,
            next_cluster: 1,
            mru: Vec::new(),
            anchors: Vec::new(),
            bookmarks: [None; 4],
            origin: Point2::new(0.0, 0.0),
        }
    }

    // ---------- доступ ----------

    pub fn windows(&self) -> &[CanvasWindow] {
        &self.windows
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn get(&self, id: WindowId) -> Option<&CanvasWindow> {
        self.windows.iter().find(|w| w.id == id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut CanvasWindow> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    pub fn focus(&self) -> Option<WindowId> {
        self.mru.first().copied()
    }

    pub fn by_app(&self, app_id: &str) -> Vec<WindowId> {
        self.windows
            .iter()
            .filter(|w| w.app_id == app_id)
            .map(|w| w.id)
            .collect()
    }

    fn top_z(&self) -> i32 {
        self.windows.iter().map(|w| w.z).max().unwrap_or(0)
    }

    // ---------- жизненный цикл ----------

    /// Добавить окно. Place учитывается при выборе z: обычные окна выше виджетов.
    pub fn insert(
        &mut self,
        app_id: impl Into<String>,
        title: impl Into<String>,
        pos: Point2<f32>,
        size: Vector2<f32>,
        place: Place,
    ) -> WindowId {
        let id = self.next_id;
        self.next_id += 1;
        let w = CanvasWindow {
            id,
            app_id: app_id.into(),
            title: title.into(),
            pos,
            size,
            z: 0,
            cluster: None,
            place,
            suspended: false,
        };
        self.windows.push(w);
        self.restack();
        if place != Place::Widget {
            self.mru.insert(0, id);
        }
        id
    }

    pub fn remove(&mut self, id: WindowId) -> Option<CanvasWindow> {
        let out = self.windows.iter().position(|w| w.id == id)?;
        let removed = self.windows.remove(out);
        self.mru.retain(|&w| w != id);
        if let Some(cluster) = removed.cluster {
            self.drop_if_single(cluster);
        }
        Some(removed)
    }

    /// Поднять и сфокусировать окно. Возвращает false, если окна нет.
    /// Виджет формально «поднимается», но restack держит его ниже окон.
    pub fn raise(&mut self, id: WindowId) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        let top = self.top_z();
        let is_widget = self.get(id).map(|w| w.place) == Some(Place::Widget);
        if let Some(w) = self.get_mut(id) {
            w.z = top + 1;
        }
        self.restack();
        self.mru.retain(|&w| w != id);
        if !is_widget {
            self.mru.insert(0, id);
        }
        true
    }

    /// Пересчитать z: виджеты всегда ниже обычных, порядок внутри групп по z.
    fn restack(&mut self) {
        self.windows.sort_by_key(|w| w.z);
        let mut z = 0;
        for w in self.windows.iter_mut() {
            if w.place == Place::Widget {
                w.z = z;
                z += 1;
            }
        }
        for w in self.windows.iter_mut() {
            if w.place != Place::Widget {
                w.z = z;
                z += 1;
            }
        }
    }

    /// Усыпить окно. Работает для любого `Place`: и обычное окно, и PiP
    /// (закреплённое к экрану) могут стать плейсхолдером — например при
    /// восстановлении сессии, где живых клиентов ещё нет.
    pub fn suspend(&mut self, id: WindowId) -> bool {
        let ok = matches!(self.get(id), Some(w) if !w.suspended);
        if !ok {
            return false;
        }
        let cluster = self.get(id).and_then(|w| w.cluster);
        if let Some(w) = self.get_mut(id) {
            w.suspended = true;
        }
        self.mru.retain(|&x| x != id);
        if let Some(c) = cluster {
            self.drop_if_single(c);
        }
        true
    }

    pub fn resume(&mut self, id: WindowId) -> bool {
        match self.get_mut(id) {
            Some(w) if w.suspended => {
                w.suspended = false;
                true
            }
            _ => false,
        }
    }

    // ---------- геометрия ----------

    pub fn move_to(&mut self, id: WindowId, pos: Point2<f32>) -> bool {
        match self.get_mut(id) {
            Some(w) => {
                w.pos = pos;
                true
            }
            None => false,
        }
    }

    pub fn nudge(&mut self, id: WindowId, delta: Vector2<f32>) -> bool {
        match self.get_mut(id) {
            Some(w) => {
                w.pos += delta;
                true
            }
            None => false,
        }
    }

    pub fn resize(&mut self, id: WindowId, size: Vector2<f32>) -> bool {
        match self.get_mut(id) {
            Some(w) => {
                w.size = Vector2::new(size.x.max(1.0), size.y.max(1.0));
                true
            }
            None => false,
        }
    }

    /// Границы содержимого (для overview и миникарты).
    pub fn content_bounds(&self, include_widgets: bool) -> Option<Aabb> {
        let mut it = self
            .windows
            .iter()
            .filter(|w| (include_widgets || w.place != Place::Widget) && !w.suspended);
        let first = it.next()?;
        let mut b = first.aabb();
        for w in it {
            let a = w.aabb();
            b.min.x = b.min.x.min(a.min.x);
            b.min.y = b.min.y.min(a.min.y);
            b.max.x = b.max.x.max(a.max.x);
            b.max.y = b.max.y.max(a.max.y);
        }
        Some(b)
    }

    // ---------- hit-test ----------

    /// Верхнее окно под точкой ХОЛСТА. `PinnedToScreen` пропускается
    /// (его тестирует композитор в экранных координатах).
    pub fn hit_test(&self, canvas_pt: Point2<f32>) -> Option<WindowId> {
        self.windows
            .iter()
            .filter(|w| w.place != Place::PinnedToScreen && !w.suspended)
            .filter(|w| w.aabb().contains(canvas_pt))
            .max_by_key(|w| w.z)
            .map(|w| w.id)
    }

    // ---------- навигация ----------

    /// Прыжок к ближайшему окну в направлении (driftwm `Mod+arrows`).
    /// Смещение по основной оси — обязательно положительное; ортогональный
    /// оффсет штрафуется ×2, чтобы «кривое» окно не перебивало прямое.
    pub fn nearest_in_direction(&self, from: Point2<f32>, dir: Dir) -> Option<WindowId> {
        let mut best: Option<(f32, WindowId)> = None;
        for w in self.windows.iter().filter(|w| w.is_navigable()) {
            let d = w.center() - from;
            let (primary, ortho) = match dir {
                Dir::Left => (-d.x, d.y.abs()),
                Dir::Right => (d.x, d.y.abs()),
                Dir::Up => (-d.y, d.x.abs()),
                Dir::Down => (d.y, d.x.abs()),
            };
            if primary <= 0.0 {
                continue;
            }
            let score = primary + ortho * 2.0;
            if best.map_or(true, |(s, _)| score < s) {
                best = Some((score, w.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Recency-список. Голова — фокус.
    pub fn mru(&self) -> &[WindowId] {
        &self.mru
    }

    /// Следующее окно в кольце MRU относительно `current` (Alt-Tab).
    pub fn mru_cycle(&self, current: Option<WindowId>, forward: bool) -> Option<WindowId> {
        let n = self.mru.len();
        if n < 2 {
            return None;
        }
        let idx = current
            .and_then(|c| self.mru.iter().position(|&x| x == c))
            .unwrap_or(0);
        let next = if forward {
            (idx + 1) % n
        } else {
            (idx + n - 1) % n
        };
        Some(self.mru[next])
    }

    /// Камера так, чтобы все окна поместились (driftwm zoom-to-fit, `Mod+W`).
    pub fn zoom_to_fit(&self, viewport: Vector2<u32>, padding: f32) -> Option<OverviewPlan> {
        let b = self.content_bounds(false)?;
        let vw = viewport.x as f32;
        let vh = viewport.y as f32;
        let bw = (b.max.x - b.min.x).max(1.0);
        let bh = (b.max.y - b.min.y).max(1.0);
        let zoom = ((vw - 2.0 * padding) / bw)
            .min((vh - 2.0 * padding) / bh)
            .clamp(0.01, 1.0);
        Some(OverviewPlan {
            camera_center: Point2::new((b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5),
            zoom,
        })
    }

    /// Zoom-to-fit только по одному кластеру (driftwm `Mod+Shift+W`).
    pub fn zoom_to_fit_cluster(
        &self,
        id: WindowId,
        viewport: Vector2<u32>,
        padding: f32,
    ) -> Option<OverviewPlan> {
        let w = self.get(id)?;
        let cluster = w.cluster?;
        let members: Vec<&CanvasWindow> = self
            .windows
            .iter()
            .filter(|x| x.cluster == Some(cluster) && !x.suspended)
            .collect();
        if members.is_empty() {
            return None;
        }
        let mut b = members[0].aabb();
        for m in members.iter().skip(1) {
            let a = m.aabb();
            b.min.x = b.min.x.min(a.min.x);
            b.min.y = b.min.y.min(a.min.y);
            b.max.x = b.max.x.max(a.max.x);
            b.max.y = b.max.y.max(a.max.y);
        }
        let vw = viewport.x as f32;
        let vh = viewport.y as f32;
        let bw = (b.max.x - b.min.x).max(1.0);
        let bh = (b.max.y - b.min.y).max(1.0);
        let zoom = ((vw - 2.0 * padding) / bw)
            .min((vh - 2.0 * padding) / bh)
            .clamp(0.01, 1.0);
        Some(OverviewPlan {
            camera_center: Point2::new((b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5),
            zoom,
        })
    }

    /// «Fit window» (driftwm `Mod+M`): окно растягивается на вьюпорт, zoom = 1.
    /// Камера центрируется на НЫНЕШНЕМ центре окна. План не применяет себя —
    /// композитор решает, отправить ли resize клиенту (xdg configure).
    pub fn fit_window_plan(
        &self,
        id: WindowId,
        viewport: Vector2<u32>,
        padding: f32,
    ) -> Option<FitPlan> {
        let w = self.get(id)?;
        let c = w.center();
        let size = Vector2::new(
            (viewport.x as f32 - 2.0 * padding).max(1.0),
            (viewport.y as f32 - 2.0 * padding).max(1.0),
        );
        Some(FitPlan {
            camera_center: c,
            zoom: 1.0,
            window_pos: Point2::new(c.x - size.x * 0.5, c.y - size.y * 0.5),
            window_size: size,
        })
    }

    // ---------- bookmarks / anchors ----------

    pub fn set_bookmark(&mut self, slot: usize, camera_center: Point2<f32>) -> bool {
        match self.bookmarks.get_mut(slot) {
            Some(b) => {
                *b = Some(camera_center);
                true
            }
            None => false,
        }
    }

    pub fn bookmark(&self, slot: usize) -> Option<Point2<f32>> {
        self.bookmarks.get(slot).copied().flatten()
    }

    pub fn add_anchor(&mut self, name: impl Into<String>, pos: Point2<f32>) {
        let name = name.into();
        self.anchors.retain(|(n, _)| n != &name);
        self.anchors.push((name, pos));
    }

    pub fn anchor(&self, name: &str) -> Option<Point2<f32>> {
        self.anchors
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| *p)
    }

    /// Ближайшая цель прыжка: окно либо anchor (driftwm: anchors — цели для
    /// направленного прыжка там, где окон нет).
    pub fn nearest_target_in_direction(&self, from: Point2<f32>, dir: Dir) -> Option<Point2<f32>> {
        let win = self
            .nearest_in_direction(from, dir)
            .and_then(|id| self.get(id).map(|w| w.center()));
        let anc = {
            let mut best: Option<(f32, Point2<f32>)> = None;
            for (_, p) in &self.anchors {
                let d = p - from;
                let (primary, ortho) = match dir {
                    Dir::Left => (-d.x, d.y.abs()),
                    Dir::Right => (d.x, d.y.abs()),
                    Dir::Up => (-d.y, d.x.abs()),
                    Dir::Down => (d.y, d.x.abs()),
                };
                if primary <= 0.0 {
                    continue;
                }
                let score = primary + ortho * 2.0;
                if best.map_or(true, |(s, _)| score < s) {
                    best = Some((score, *p));
                }
            }
            best.map(|(_, p)| p)
        };
        match (win, anc) {
            (Some(w), Some(a)) => {
                let dw = (w - from).magnitude();
                let da = (a - from).magnitude();
                if da < dw {
                    Some(a)
                } else {
                    Some(w)
                }
            }
            (Some(w), None) => Some(w),
            (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }

    // ---------- snapping / clusters ----------

    /// Прилипание к соседям по краям. Вызывается композитором на drag/resize
    /// ПОСЛЕ предварительного перемещения `nudge`/`move_to`.
    pub fn snap(&mut self, id: WindowId, threshold: f32) -> Option<SnapResult> {
        let moving = self.get(id)?.clone();
        let same_class: Vec<CanvasWindow> = self
            .windows
            .iter()
            .filter(|w| {
                w.id != id
                    && !w.suspended
                    && w.place != Place::PinnedToScreen
                    && (w.place == Place::Widget) == (moving.place == Place::Widget)
            })
            .cloned()
            .collect();

        let (x1, y1, w, h) = moving.rect();
        let x2 = x1 + w;
        let y2 = y1 + h;

        let mut best_dx: Option<f32> = None;
        let mut best_dy: Option<f32> = None;

        for o in &same_class {
            let (ox1, oy1, ow, oh) = o.rect();
            let ox2 = ox1 + ow;
            let oy2 = oy1 + oh;

            for d in [ox1 - x1, ox2 - x2, ox2 - x1, ox1 - x2] {
                if d.abs() <= threshold && best_dx.map_or(true, |b| d.abs() < b.abs()) {
                    best_dx = Some(d);
                }
            }
            for d in [oy1 - y1, oy2 - y2, oy2 - y1, oy1 - y2] {
                if d.abs() <= threshold && best_dy.map_or(true, |b| d.abs() < b.abs()) {
                    best_dy = Some(d);
                }
            }
        }

        let dx = best_dx.unwrap_or(0.0);
        let dy = best_dy.unwrap_or(0.0);
        if dx != 0.0 || dy != 0.0 {
            if let Some(m) = self.get_mut(id) {
                m.pos.x += dx;
                m.pos.y += dy;
            }
        }

        // Кто теперь касается (по итоговой позиции)?
        let cur = self.get(id)?.clone();
        let (nx1, ny1, nw, nh) = cur.rect();
        let nx2 = nx1 + nw;
        let ny2 = ny1 + nh;
        let neighbors: Vec<WindowId> = same_class
            .iter()
            .filter(|o| {
                let (ox1, oy1, ow, oh) = o.rect();
                let ox2 = ox1 + ow;
                let oy2 = oy1 + oh;
                let x_touch = (nx2 - ox1).abs() <= TOUCH_EPS || (ox2 - nx1).abs() <= TOUCH_EPS;
                let y_touch = (ny2 - oy1).abs() <= TOUCH_EPS || (oy2 - ny1).abs() <= TOUCH_EPS;
                let y_overlap = ny1 < oy2 - TOUCH_EPS && oy1 < ny2 - TOUCH_EPS;
                let x_overlap = nx1 < ox2 - TOUCH_EPS && ox1 < nx2 - TOUCH_EPS;
                (x_touch && y_overlap) || (y_touch && x_overlap)
            })
            .map(|o| o.id)
            .collect();

        let cluster = if neighbors.is_empty() {
            // Уехало в одиночку — выпало из кластера.
            let old = cur.cluster;
            if let Some(m) = self.get_mut(id) {
                m.cluster = None;
            }
            if let Some(c) = old {
                self.drop_if_single(c);
            }
            None
        } else {
            let mut ids: Vec<ClusterId> = neighbors
                .iter()
                .filter_map(|n| self.get(*n).and_then(|w| w.cluster))
                .collect();
            ids.sort_unstable();
            ids.dedup();
            let target = ids.first().copied().unwrap_or_else(|| self.alloc_cluster());
            for n in &neighbors {
                if let Some(w) = self.get_mut(*n) {
                    w.cluster = Some(target);
                }
            }
            if let Some(m) = self.get_mut(id) {
                m.cluster = Some(target);
            }
            // Слить остальные кластеры в target.
            for other in ids.into_iter().filter(|c| *c != target) {
                let members: Vec<WindowId> = self
                    .windows
                    .iter()
                    .filter(|w| w.cluster == Some(other))
                    .map(|w| w.id)
                    .collect();
                for m in members {
                    if let Some(w) = self.get_mut(m) {
                        w.cluster = Some(target);
                    }
                }
            }
            Some(target)
        };

        Some(SnapResult {
            dx,
            dy,
            neighbors,
            cluster,
        })
    }

    /// Очистить холст (для восстановления сессии).
    pub fn clear(&mut self) {
        self.windows.clear();
        self.mru.clear();
        self.bookmarks = [None; 4];
        self.anchors.clear();
    }

    /// Объединить уже существующие окна в кластер БЕЗ перемещения
    /// (в отличие от `attach`, который придвигает края). Нужен для restore.
    pub fn group(&mut self, ids: &[WindowId]) -> Option<ClusterId> {
        let alive: Vec<WindowId> = ids
            .iter()
            .copied()
            .filter(|id| self.get(*id).is_some())
            .collect();
        if alive.len() < 2 {
            return None;
        }
        let old: Vec<ClusterId> = alive
            .iter()
            .filter_map(|id| self.get(*id).and_then(|w| w.cluster))
            .collect();
        let cluster = match old.first() {
            Some(c) => *c,
            None => self.alloc_cluster(),
        };
        for id in &alive {
            if let Some(w) = self.get_mut(*id) {
                w.cluster = Some(cluster);
            }
        }
        for c in old {
            if c != cluster {
                let members: Vec<WindowId> = self
                    .windows
                    .iter()
                    .filter(|w| w.cluster == Some(c))
                    .map(|w| w.id)
                    .collect();
                for m in members {
                    if let Some(w) = self.get_mut(m) {
                        w.cluster = Some(cluster);
                    }
                }
            }
        }
        self.drop_if_single(cluster);
        if self.get(alive[0]).and_then(|w| w.cluster).is_some() {
            Some(cluster)
        } else {
            None
        }
    }

    fn alloc_cluster(&mut self) -> ClusterId {
        let c = self.next_cluster;
        self.next_cluster += 1;
        c
    }

    /// Явно сцепить два окна в кластер и выровнять их края (без drag-магии).
    ///
    /// Нужно там, где кластер объявляется, а не рождается из перетаскивания:
    /// демо-сцена, восстановление сессии, «собрать окна» по команде.
    /// Возвращает id кластера. Если у окон были другие кластеры — они
    /// растворяются (кластер существует ⇔ ≥2 участников).
    pub fn attach(&mut self, a: WindowId, b: WindowId) -> Option<ClusterId> {
        if a == b || self.get(a).is_none() || self.get(b).is_none() {
            return None;
        }
        // Выравниваем b справа от a и по верхнему краю.
        let (aright, atop) = {
            let wa = self.get(a)?;
            (wa.pos.x + wa.size.x, wa.pos.y)
        };
        if let Some(wb) = self.get_mut(b) {
            wb.pos = Point2::new(aright, atop);
        }

        let cluster = match (
            self.get(a).and_then(|w| w.cluster),
            self.get(b).and_then(|w| w.cluster),
        ) {
            (Some(c), _) | (None, Some(c)) => c,
            (None, None) => self.alloc_cluster(),
        };
        for id in [a, b] {
            if let Some(w) = self.get_mut(id) {
                w.cluster = Some(cluster);
            }
        }
        // Слить возможный второй кластер в общий.
        let others: Vec<ClusterId> = [a, b]
            .iter()
            .filter_map(|id| self.get(*id).and_then(|w| w.cluster))
            .filter(|c| *c != cluster)
            .collect();
        for other in others {
            let members: Vec<WindowId> = self
                .windows
                .iter()
                .filter(|w| w.cluster == Some(other))
                .map(|w| w.id)
                .collect();
            for m in members {
                if let Some(w) = self.get_mut(m) {
                    w.cluster = Some(cluster);
                }
            }
        }
        Some(cluster)
    }

    /// Если в кластере осталось < 2 окон — он перестаёт быть кластером.
    fn drop_if_single(&mut self, cluster: ClusterId) {
        let members: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.cluster == Some(cluster))
            .map(|w| w.id)
            .collect();
        if members.len() < 2 {
            for m in members {
                if let Some(w) = self.get_mut(m) {
                    w.cluster = None;
                }
            }
        }
    }

    /// Переместить окно вместе со всем кластером (driftwm: `Shift` + move).
    /// Возвращает список перемещённых окон.
    pub fn translate_cluster(&mut self, id: WindowId, delta: Vector2<f32>) -> Vec<WindowId> {
        let ids = self.cluster_members(id);
        for &m in &ids {
            if let Some(w) = self.get_mut(m) {
                w.pos += delta;
            }
        }
        ids
    }

    /// Окно + его соседи по кластеру (сам впереди).
    pub fn cluster_members(&self, id: WindowId) -> Vec<WindowId> {
        match self.get(id).and_then(|w| w.cluster) {
            None => vec![id],
            Some(c) => {
                let mut v = vec![id];
                v.extend(
                    self.windows
                        .iter()
                        .filter(|w| w.cluster == Some(c) && w.id != id)
                        .map(|w| w.id),
                );
                v
            }
        }
    }

    /// Все кластеры: id → участники (только размером ≥ 2).
    pub fn clusters(&self) -> HashMap<ClusterId, Vec<WindowId>> {
        let mut map: HashMap<ClusterId, Vec<WindowId>> = HashMap::new();
        for w in &self.windows {
            if let Some(c) = w.cluster {
                map.entry(c).or_default().push(w.id);
            }
        }
        map.retain(|_, v| v.len() >= 2);
        map
    }
}

/// Автоматическая расстановка новых окон: центр вьюпорта (driftwm default).
pub fn place_new(
    scene: &Scene,
    camera: &crate::camera::Camera,
    size: Vector2<f32>,
    offset_step: f32,
) -> Point2<f32> {
    let c = camera.center;
    let n = scene.len() as f32;
    // Каскад, чтобы окна не ложились идеально стопкой.
    Point2::new(
        c.x - size.x * 0.5 + n * offset_step,
        c.y - size.y * 0.5 + n * offset_step,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    fn v(x: f32, y: f32) -> Vector2<f32> {
        Vector2::new(x, y)
    }
    fn vp(x: u32, y: u32) -> Vector2<u32> {
        Vector2::new(x, y)
    }
    fn p(x: f32, y: f32) -> Point2<f32> {
        Point2::new(x, y)
    }
    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn scene3() -> (Scene, WindowId, WindowId, WindowId) {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        let b = s.insert("b", "B", p(300.0, 0.0), v(100.0, 100.0), Place::Normal);
        let c = s.insert("c", "C", p(0.0, 300.0), v(100.0, 100.0), Place::Normal);
        (s, a, b, c)
    }

    #[test]
    fn insert_focuses_and_raise_moves_to_head() {
        let (mut s, a, b, c) = scene3();
        assert_eq!(s.focus(), Some(c)); // последнее вставленное сверху
        assert_eq!(s.mru(), &[c, b, a]);
        assert!(s.raise(a));
        assert_eq!(s.focus(), Some(a));
        assert_eq!(s.mru(), &[a, c, b]);
    }

    #[test]
    fn raise_puts_window_on_top_for_hit_test() {
        let (mut s, a, b, _) = scene3();
        // окна не пересекаются — сдвинем b на a
        s.move_to(b, p(50.0, 50.0));
        assert_eq!(s.hit_test(p(60.0, 60.0)), Some(b));
        s.raise(a); // a теперь выше
        assert_eq!(s.hit_test(p(60.0, 60.0)), Some(a));
    }

    #[test]
    fn widgets_sit_below_normal_windows_and_stay_out_of_mru() {
        let mut s = Scene::new();
        let w = s.insert("clock", "Clock", p(0.0, 0.0), v(200.0, 50.0), Place::Widget);
        let n = s.insert("term", "Term", p(0.0, 0.0), v(200.0, 50.0), Place::Normal);
        let wz = s.get(w).unwrap().z;
        let nz = s.get(n).unwrap().z;
        assert!(wz < nz, "widget z={wz} must be below normal z={nz}");
        assert_eq!(s.mru(), &[n], "widgets are not Alt-Tab candidates");
        assert!(s.raise(w), "but a widget can still be raised");
        assert!(
            s.get(w).unwrap().z < s.get(n).unwrap().z,
            "restack keeps widgets below"
        );
    }

    #[test]
    fn pinned_to_screen_is_not_hit_by_canvas_point() {
        let mut s = Scene::new();
        let pip = s.insert(
            "mpv",
            "PiP",
            p(0.0, 0.0),
            v(320.0, 180.0),
            Place::PinnedToScreen,
        );
        assert_eq!(s.hit_test(p(10.0, 10.0)), None);
        assert_eq!(s.cluster_members(pip), vec![pip]);
    }

    #[test]
    fn directional_jump_prefers_aligned_window() {
        let mut s = Scene::new();
        let right = s.insert("r", "Right", p(300.0, 0.0), v(100.0, 100.0), Place::Normal);
        // окно ещё правее по X, но сильно смещено по Y — должно проиграть
        s.insert(
            "far",
            "Far",
            p(320.0, 2000.0),
            v(100.0, 100.0),
            Place::Normal,
        );
        let from = p(100.0, 50.0);
        assert_eq!(s.nearest_in_direction(from, Dir::Right), Some(right));
        assert_eq!(s.nearest_in_direction(p(400.0, 50.0), Dir::Right), None);
    }

    #[test]
    fn directional_jump_skips_suspended_and_widgets() {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(300.0, 0.0), v(100.0, 100.0), Place::Normal);
        let b = s.insert("b", "B", p(500.0, 0.0), v(100.0, 100.0), Place::Normal);
        s.suspend(a);
        let from = p(0.0, 0.0);
        assert_eq!(s.nearest_in_direction(from, Dir::Right), Some(b));
    }

    #[test]
    fn mru_cycle_wraps_both_ways() {
        // кольцо [c, b, a]: c — фокус, a — самый старый
        let (s, a, b, c) = scene3();
        assert_eq!(s.mru_cycle(Some(c), true), Some(b), "forward = next older");
        assert_eq!(s.mru_cycle(Some(a), true), Some(c), "wrap oldest -> newest");
        assert_eq!(
            s.mru_cycle(Some(a), false),
            Some(b),
            "backward = next newer"
        );
        let one = Scene::new();
        assert_eq!(one.mru_cycle(None, true), None);
    }

    #[test]
    fn overview_contains_every_window() {
        let (s, ..) = scene3();
        let viewport = vp(800, 600);
        let plan = s.zoom_to_fit(viewport, 32.0).unwrap();
        let mut cam = Camera::new(viewport);
        cam.center = plan.camera_center;
        cam.zoom = plan.zoom;
        for w in s.windows() {
            let tl = cam.world_to_screen(w.pos);
            let br = cam.world_to_screen(p(w.pos.x + w.size.x, w.pos.y + w.size.y));
            assert!(tl.x >= -1.0 && tl.y >= -1.0, "window {w:?} outside: {tl:?}");
            assert!(br.x <= viewport.x as f32 + 1.0 && br.y <= viewport.y as f32 + 1.0);
        }
        assert!(plan.zoom <= 1.0, "overview never magnifies");
    }

    #[test]
    fn fit_window_plan_fills_viewport_and_centers_camera() {
        let (s, a, ..) = scene3();
        let viewport = vp(1920, 1080);
        let plan = s.fit_window_plan(a, viewport, 0.0).unwrap();
        assert_eq!(plan.zoom, 1.0);
        assert!(approx(plan.window_size.x, 1920.0));
        assert!(approx(plan.window_size.y, 1080.0));
        let c = s.get(a).unwrap().center();
        assert!(approx(plan.camera_center.x, c.x));
        assert!(approx(plan.camera_center.y, c.y));
        assert!(approx(plan.window_pos.x + plan.window_size.x * 0.5, c.x));
    }

    #[test]
    fn snap_aligns_right_edge_to_neighbour_left_edge() {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        let b = s.insert("b", "B", p(105.0, 3.0), v(100.0, 100.0), Place::Normal);
        let r = s.snap(b, 10.0).unwrap();
        assert!(approx(r.dx, -5.0), "dx={}", r.dx);
        assert!(approx(r.dy, -3.0), "dy={}", r.dy);
        assert_eq!(r.neighbors, vec![a]);
        assert!(r.cluster.is_some(), "touching windows form a cluster");
        assert_eq!(s.get(b).unwrap().cluster, s.get(a).unwrap().cluster);
        assert_eq!(s.clusters().len(), 1);
    }

    #[test]
    fn snap_does_not_touch_far_windows() {
        let mut s = Scene::new();
        s.insert("a", "A", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        let b = s.insert("b", "B", p(1000.0, 1000.0), v(100.0, 100.0), Place::Normal);
        let r = s.snap(b, 10.0).unwrap();
        assert!(approx(r.dx, 0.0) && approx(r.dy, 0.0));
        assert!(r.neighbors.is_empty());
        assert_eq!(r.cluster, None);
    }

    #[test]
    fn translate_cluster_moves_all_members_preserving_gap() {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        let b = s.insert("b", "B", p(100.0, 0.0), v(100.0, 100.0), Place::Normal);
        s.snap(b, 1.0).unwrap();
        let moved = s.translate_cluster(a, v(50.0, 25.0));
        assert_eq!(moved.len(), 2);
        assert!(approx(s.get(a).unwrap().pos.x, 50.0));
        assert!(approx(s.get(b).unwrap().pos.x, 150.0));
        assert!(approx(s.get(a).unwrap().pos.y, 25.0));
        assert!(approx(s.get(b).unwrap().pos.y, 25.0));
    }

    #[test]
    fn closing_one_of_two_dissolves_cluster() {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        let b = s.insert("b", "B", p(100.0, 0.0), v(100.0, 100.0), Place::Normal);
        s.snap(b, 1.0).unwrap();
        assert!(s.get(a).unwrap().cluster.is_some());
        s.remove(b);
        assert_eq!(
            s.get(a).unwrap().cluster,
            None,
            "a group of one is not a group"
        );
        assert!(s.clusters().is_empty());
    }

    #[test]
    fn moving_away_releases_window_from_cluster() {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        let b = s.insert("b", "B", p(100.0, 0.0), v(100.0, 100.0), Place::Normal);
        s.snap(b, 1.0).unwrap();
        s.move_to(b, p(900.0, 900.0));
        let r = s.snap(b, 8.0).unwrap();
        assert!(r.neighbors.is_empty());
        assert_eq!(s.get(b).unwrap().cluster, None);
        assert_eq!(s.get(a).unwrap().cluster, None);
    }

    #[test]
    fn suspend_removes_from_navigation_and_resume_returns() {
        let (mut s, a, ..) = scene3();
        assert!(s.suspend(a));
        assert!(!s.mru().contains(&a));
        assert!(!s.suspend(a), "double suspend is a no-op");
        assert!(s.resume(a));
        assert!(!s.resume(a), "double resume is a no-op");
    }

    #[test]
    fn anchors_are_jump_targets_and_nearest_wins() {
        let mut s = Scene::new();
        s.add_anchor("home", p(0.0, 0.0));
        let far = s.insert("f", "Far", p(4000.0, 0.0), v(100.0, 100.0), Place::Normal);
        let from = p(0.0, 0.0);
        // старт в origin: сначала anchor "home"? он сзади — берём окно
        assert_eq!(
            s.nearest_target_in_direction(from, Dir::Right),
            Some(s.get(far).unwrap().center())
        );
        s.add_anchor("right-near", p(500.0, 0.0));
        assert_eq!(
            s.nearest_target_in_direction(from, Dir::Right),
            Some(p(500.0, 0.0))
        );
        assert_eq!(s.anchor("home"), Some(p(0.0, 0.0)));
        assert_eq!(s.anchor("missing"), None);
    }

    #[test]
    fn bookmarks_store_and_recall_camera_positions() {
        let mut s = Scene::new();
        assert_eq!(s.bookmark(0), None);
        assert!(s.set_bookmark(0, p(10.0, 20.0)));
        assert_eq!(s.bookmark(0), Some(p(10.0, 20.0)));
        assert!(!s.set_bookmark(9, p(0.0, 0.0)), "out of range");
    }

    #[test]
    fn new_windows_place_around_viewport_center_with_cascade() {
        let mut s = Scene::new();
        let mut cam = Camera::new(vp(800, 600));
        cam.center = p(0.0, 0.0);
        let p1 = place_new(&s, &cam, v(400.0, 300.0), 30.0);
        assert!(approx(p1.x, -200.0) && approx(p1.y, -150.0));
        s.insert("a", "A", p1, v(400.0, 300.0), Place::Normal);
        let p2 = place_new(&s, &cam, v(400.0, 300.0), 30.0);
        assert!(approx(p2.x, -170.0) && approx(p2.y, -120.0));
    }

    #[test]
    fn attach_groups_and_aligns_edges() {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(0.0, 0.0), v(200.0, 120.0), Place::Normal);
        let b = s.insert("b", "B", p(900.0, 700.0), v(150.0, 120.0), Place::Normal);
        let c = s.attach(a, b).unwrap();
        assert_eq!(s.get(a).unwrap().cluster, Some(c));
        assert_eq!(s.get(b).unwrap().cluster, Some(c));
        assert!(
            approx(s.get(b).unwrap().pos.x, 200.0),
            "b прижат справа от a"
        );
        assert!(approx(s.get(b).unwrap().pos.y, 0.0), "по верхнему краю");
        assert_eq!(s.clusters().len(), 1);
        // перемещение кластера тащит оба
        let moved = s.translate_cluster(a, v(10.0, 5.0));
        assert_eq!(moved.len(), 2);
    }

    #[test]
    fn attach_rejects_degenerate_input() {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(0.0, 0.0), v(10.0, 10.0), Place::Normal);
        assert_eq!(s.attach(a, a), None, "сам с собой — не кластер");
        assert_eq!(s.attach(a, 9999), None, "несуществующего окна нет");
        assert_eq!(s.attach(a, 9999), None);
    }

    #[test]
    fn zoom_to_fit_cluster_only_covers_cluster() {
        let mut s = Scene::new();
        let a = s.insert("a", "A", p(0.0, 0.0), v(100.0, 100.0), Place::Normal);
        let b = s.insert("b", "B", p(100.0, 0.0), v(100.0, 100.0), Place::Normal);
        s.insert(
            "far",
            "F",
            p(9000.0, 9000.0),
            v(100.0, 100.0),
            Place::Normal,
        );
        s.snap(b, 1.0).unwrap();
        let plan = s.zoom_to_fit_cluster(b, vp(800, 600), 20.0).unwrap();
        assert!(plan.camera_center.x < 200.0 && plan.camera_center.y < 200.0);
        let _ = a;
    }
}
