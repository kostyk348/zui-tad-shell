//! # Interaction — состояние указателя над холстом
//!
//! Чистая state machine для drag/pan/resize. Композитор кормит её мировыми и
//! экранными координатами курсора и получает план перемещения/ресайза.
//! Никакого Wayland/GPU — всё в тестах.
//!
//! Инварианты:
//!   * `Mode::None` — дельты нулевые.
//!   * Move/Resize помнят ТОЧКУ ЗАХВАТА и СТАРТОВЫЙ ПРЯМОУГОЛЬНИК, поэтому
//!     окно не «прыгает» и ресайз всегда считается от исходного состояния
//!     (без накопления ошибки), как в обычных DE.
//!   * Resize с левого/верхнего края ДВИГАЕТ окно (правый/нижний край стоит).
//!   * Pan работает в экранных px (камера сама делит на zoom).

use crate::scene::WindowId;
use cgmath::{Point2, Vector2};

/// Восемь зон ресайза — как у любого оконного менеджера.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    N,
    S,
    E,
    W,
    Ne,
    Nw,
    Se,
    Sw,
}

impl ResizeHandle {
    /// Зона по точке ВНУТРИ окна (координаты от левого-верхнего угла окна).
    /// `margin` — толщина зоны в тех же единицах (обычно экранные px / zoom).
    pub fn from_point(size: Vector2<f32>, p: Vector2<f32>, margin: f32) -> Option<Self> {
        if size.x <= 0.0 || size.y <= 0.0 || margin <= 0.0 {
            return None;
        }
        let left = p.x <= margin;
        let right = p.x >= size.x - margin;
        let top = p.y <= margin;
        let bottom = p.y >= size.y - margin;
        // Углы приоритетнее рёбер: они и есть самое удобное место.
        match (left, right, top, bottom) {
            (true, _, true, _) => Some(Self::Nw),
            (_, true, true, _) => Some(Self::Ne),
            (true, _, _, true) => Some(Self::Sw),
            (_, true, _, true) => Some(Self::Se),
            (true, _, _, _) => Some(Self::W),
            (_, true, _, _) => Some(Self::E),
            (_, _, true, _) => Some(Self::N),
            (_, _, _, true) => Some(Self::S),
            _ => None,
        }
    }

    /// (тянет_левый, тянет_верхний, тянет_правый, тянет_нижний)
    pub fn edges(&self) -> (bool, bool, bool, bool) {
        match self {
            Self::N => (false, true, false, false),
            Self::S => (false, false, false, true),
            Self::E => (false, false, true, false),
            Self::W => (true, false, false, false),
            Self::Ne => (false, true, true, false),
            Self::Nw => (true, true, false, false),
            Self::Se => (false, false, true, true),
            Self::Sw => (true, false, false, true),
        }
    }

    /// Какой курсор показывать.
    pub fn cursor_name(&self) -> &'static str {
        match self {
            Self::N | Self::S => "ns-resize",
            Self::E | Self::W => "ew-resize",
            Self::Nw | Self::Se => "nwse-resize",
            Self::Ne | Self::Sw => "nesw-resize",
        }
    }
}

/// План ресайза: новая позиция + новый размер, посчитанные от СТАРТОВОГО
/// прямоугольника (а не от предыдущего кадра — иначе накапливается ошибка).
pub fn resize_rect(
    start_pos: Point2<f32>,
    start_size: Vector2<f32>,
    handle: ResizeHandle,
    grab: Point2<f32>,
    cursor: Point2<f32>,
    min: Vector2<f32>,
) -> (Point2<f32>, Vector2<f32>) {
    let d = cursor - grab;
    let (el, et, er, eb) = handle.edges();
    let (mut x, mut y) = (start_pos.x, start_pos.y);
    let (mut w, mut h) = (start_size.x, start_size.y);

    if er {
        w = (start_size.x + d.x).max(min.x);
    }
    if el {
        w = (start_size.x - d.x).max(min.x);
        x = start_pos.x + (start_size.x - w); // правый край стоит на месте
    }
    if eb {
        h = (start_size.y + d.y).max(min.y);
    }
    if et {
        h = (start_size.y - d.y).max(min.y);
        y = start_pos.y + (start_size.y - h); // нижний край стоит на месте
    }

    (Point2::new(x, y), Vector2::new(w, h))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    None,
    Pan,
    Move {
        id: WindowId,
        grab_world: Point2<f32>,
    },
    Resize {
        id: WindowId,
        handle: ResizeHandle,
        /// Стартовый прямоугольник окна на момент захвата.
        start_pos: Point2<f32>,
        start_size: Vector2<f32>,
        grab_world: Point2<f32>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Interact {
    pub mode: Mode,
    last_world: Point2<f32>,
    last_screen: Point2<f32>,
}

impl Default for Interact {
    fn default() -> Self {
        Self::new()
    }
}

impl Interact {
    pub fn new() -> Self {
        Self {
            mode: Mode::None,
            last_world: Point2::new(0.0, 0.0),
            last_screen: Point2::new(0.0, 0.0),
        }
    }

    pub fn is_dragging(&self) -> bool {
        matches!(self.mode, Mode::Move { .. } | Mode::Resize { .. })
    }

    pub fn begin_pan(&mut self, world: Point2<f32>, screen: Point2<f32>) {
        self.mode = Mode::Pan;
        self.record(world, screen);
    }

    pub fn begin_move(&mut self, id: WindowId, world: Point2<f32>, screen: Point2<f32>) {
        self.mode = Mode::Move {
            id,
            grab_world: world,
        };
        self.record(world, screen);
    }

    pub fn begin_resize(
        &mut self,
        id: WindowId,
        handle: ResizeHandle,
        start_pos: Point2<f32>,
        start_size: Vector2<f32>,
        world: Point2<f32>,
        screen: Point2<f32>,
    ) {
        self.mode = Mode::Resize {
            id,
            handle,
            start_pos,
            start_size,
            grab_world: world,
        };
        self.record(world, screen);
    }

    pub fn end(&mut self) {
        self.mode = Mode::None;
    }

    fn record(&mut self, world: Point2<f32>, screen: Point2<f32>) {
        self.last_world = world;
        self.last_screen = screen;
    }

    /// Сдвинуть курсор. Возвращает (мировая_дельта, экранная_дельта).
    /// В режиме `None` ничего не движется — только запоминается позиция.
    pub fn cursor(
        &mut self,
        world: Point2<f32>,
        screen: Point2<f32>,
    ) -> (Vector2<f32>, Vector2<f32>) {
        if self.mode == Mode::None {
            self.record(world, screen);
            return (Vector2::new(0.0, 0.0), Vector2::new(0.0, 0.0));
        }
        let dw = world - self.last_world;
        let ds = screen - self.last_screen;
        self.record(world, screen);
        (dw, ds)
    }

    /// Смещение от точки захвата (для перемещения окна целиком).
    pub fn drag_offset(&self, world: Point2<f32>) -> Vector2<f32> {
        match self.mode {
            Mode::Move { grab_world, .. } => world - grab_world,
            _ => Vector2::new(0.0, 0.0),
        }
    }

    /// План ресайза для текущего курсора (None, если это не resize).
    pub fn resize_plan(&self, min: Vector2<f32>) -> Option<(Point2<f32>, Vector2<f32>)> {
        match self.mode {
            Mode::Resize {
                handle,
                start_pos,
                start_size,
                grab_world,
                ..
            } => Some(resize_rect(
                start_pos,
                start_size,
                handle,
                grab_world,
                self.last_world,
                min,
            )),
            _ => None,
        }
    }

    pub fn grab_id(&self) -> Option<WindowId> {
        match self.mode {
            Mode::Move { id, .. } | Mode::Resize { id, .. } => Some(id),
            _ => None,
        }
    }

    pub fn resize_handle(&self) -> Option<ResizeHandle> {
        match self.mode {
            Mode::Resize { handle, .. } => Some(handle),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point2<f32> {
        Point2::new(x, y)
    }
    fn v(x: f32, y: f32) -> Vector2<f32> {
        Vector2::new(x, y)
    }
    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn idle_gives_zero_delta() {
        let mut i = Interact::new();
        let (dw, ds) = i.cursor(p(100.0, 100.0), p(10.0, 10.0));
        assert_eq!(dw, v(0.0, 0.0));
        assert_eq!(ds, v(0.0, 0.0));
        assert!(!i.is_dragging());
    }

    #[test]
    fn move_delta_is_relative_since_last_cursor() {
        let mut i = Interact::new();
        i.begin_move(7, p(0.0, 0.0), p(0.0, 0.0));
        let (dw, _) = i.cursor(p(30.0, 15.0), p(60.0, 30.0));
        assert_eq!(dw, v(30.0, 15.0));
        let (dw2, _) = i.cursor(p(35.0, 25.0), p(70.0, 50.0));
        assert_eq!(dw2, v(5.0, 10.0));
        assert_eq!(i.grab_id(), Some(7));
    }

    #[test]
    fn drag_offset_is_absolute_from_grab_point() {
        let mut i = Interact::new();
        i.begin_move(1, p(100.0, 100.0), p(0.0, 0.0));
        assert_eq!(i.drag_offset(p(100.0, 100.0)), v(0.0, 0.0));
        assert_eq!(i.drag_offset(p(140.0, 70.0)), v(40.0, -30.0));
        assert_eq!(i.drag_offset(p(0.0, 0.0)), v(-100.0, -100.0));
    }

    #[test]
    fn pan_uses_screen_delta_only() {
        let mut i = Interact::new();
        i.begin_pan(p(0.0, 0.0), p(400.0, 300.0));
        let (_, ds) = i.cursor(p(999.0, 999.0), p(430.0, 280.0));
        assert_eq!(ds, v(30.0, -20.0));
        assert!(!i.is_dragging(), "pan — не drag окна");
    }

    #[test]
    fn handle_from_point_hits_corners_first_and_edges() {
        let size = v(200.0, 100.0);
        assert_eq!(
            ResizeHandle::from_point(size, v(1.0, 1.0), 6.0),
            Some(ResizeHandle::Nw)
        );
        assert_eq!(
            ResizeHandle::from_point(size, v(199.0, 1.0), 6.0),
            Some(ResizeHandle::Ne)
        );
        assert_eq!(
            ResizeHandle::from_point(size, v(1.0, 99.0), 6.0),
            Some(ResizeHandle::Sw)
        );
        assert_eq!(
            ResizeHandle::from_point(size, v(199.0, 99.0), 6.0),
            Some(ResizeHandle::Se)
        );
        assert_eq!(
            ResizeHandle::from_point(size, v(2.0, 50.0), 6.0),
            Some(ResizeHandle::W)
        );
        assert_eq!(
            ResizeHandle::from_point(size, v(198.0, 50.0), 6.0),
            Some(ResizeHandle::E)
        );
        assert_eq!(
            ResizeHandle::from_point(size, v(100.0, 2.0), 6.0),
            Some(ResizeHandle::N)
        );
        assert_eq!(
            ResizeHandle::from_point(size, v(100.0, 98.0), 6.0),
            Some(ResizeHandle::S)
        );
        assert_eq!(
            ResizeHandle::from_point(size, v(100.0, 50.0), 6.0),
            None,
            "центр — не ресайз"
        );
    }

    #[test]
    fn resize_se_grows_from_top_left_anchor() {
        let (pos, size) = resize_rect(
            p(10.0, 20.0),
            v(200.0, 100.0),
            ResizeHandle::Se,
            p(0.0, 0.0),
            p(50.0, 25.0),
            v(50.0, 40.0),
        );
        assert_eq!(pos, p(10.0, 20.0));
        assert_eq!(size, v(250.0, 125.0));
    }

    #[test]
    fn resize_west_moves_origin_keeping_right_edge() {
        let (pos, size) = resize_rect(
            p(100.0, 100.0),
            v(200.0, 100.0),
            ResizeHandle::W,
            p(0.0, 0.0),
            p(-40.0, 0.0),
            v(50.0, 40.0),
        );
        assert!(approx(size.x, 240.0));
        assert!(approx(pos.x, 60.0));
        assert!(approx(pos.x + size.x, 300.0), "правый край не сдвинулся");
        assert!(approx(size.y, 100.0), "высота не тронута");
    }

    #[test]
    fn resize_north_moves_origin_keeping_bottom_edge() {
        let (pos, size) = resize_rect(
            p(0.0, 50.0),
            v(100.0, 200.0),
            ResizeHandle::N,
            p(0.0, 0.0),
            p(0.0, -30.0),
            v(10.0, 40.0),
        );
        assert!(approx(size.y, 230.0));
        assert!(approx(pos.y, 20.0));
        assert!(approx(pos.y + size.y, 250.0), "нижний край стоит");
    }

    #[test]
    fn resize_respects_min_size_and_does_not_flip() {
        // тянем сильно в минус — размер упирается в минимум, край фиксирован
        let (pos, size) = resize_rect(
            p(100.0, 100.0),
            v(200.0, 100.0),
            ResizeHandle::Se,
            p(0.0, 0.0),
            p(-500.0, -500.0),
            v(80.0, 60.0),
        );
        assert_eq!(size, v(80.0, 60.0));
        assert_eq!(pos, p(100.0, 100.0), "позиция не должна уехать");

        let (pos_w, size_w) = resize_rect(
            p(100.0, 100.0),
            v(200.0, 100.0),
            ResizeHandle::W,
            p(0.0, 0.0),
            p(900.0, 0.0),
            v(80.0, 60.0),
        );
        assert!(approx(size_w.x, 80.0));
        assert!(
            approx(pos_w.x + size_w.x, 300.0),
            "правый край фиксирован при клампе"
        );
    }

    #[test]
    fn resize_plan_is_absolute_not_accumulating() {
        let mut i = Interact::new();
        i.begin_resize(
            3,
            ResizeHandle::Se,
            p(0.0, 0.0),
            v(100.0, 100.0),
            p(0.0, 0.0),
            p(0.0, 0.0),
        );
        i.cursor(p(10.0, 10.0), p(10.0, 10.0));
        let first = i.resize_plan(v(1.0, 1.0)).unwrap();
        assert_eq!(first.1, v(110.0, 110.0));
        // курсор вернулся в точку захвата → размер снова исходный (без дрейфа)
        i.cursor(p(0.0, 0.0), p(0.0, 0.0));
        let back = i.resize_plan(v(1.0, 1.0)).unwrap();
        assert_eq!(back.1, v(100.0, 100.0));
        assert_eq!(i.resize_handle(), Some(ResizeHandle::Se));
    }

    #[test]
    fn cursor_names_are_standard() {
        assert_eq!(ResizeHandle::N.cursor_name(), "ns-resize");
        assert_eq!(ResizeHandle::E.cursor_name(), "ew-resize");
        assert_eq!(ResizeHandle::Nw.cursor_name(), "nwse-resize");
        assert_eq!(ResizeHandle::Ne.cursor_name(), "nesw-resize");
    }

    #[test]
    fn end_clears_mode_and_dead_mode_keeps_state() {
        let mut i = Interact::new();
        i.begin_move(9, p(0.0, 0.0), p(0.0, 0.0));
        i.end();
        assert_eq!(i.mode, Mode::None);
        assert_eq!(i.grab_id(), None);
        assert_eq!(i.resize_plan(v(1.0, 1.0)), None);
        assert_eq!(i.drag_offset(p(10.0, 10.0)), v(0.0, 0.0));
    }
}
