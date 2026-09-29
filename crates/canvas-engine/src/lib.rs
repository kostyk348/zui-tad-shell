//! # canvas-engine
//!
//! Бесконечный холст: камера, frustum culling, semantic LOD, canvas-scene,
//! раскладка на экран и state machine взаимодействия.
//! GPU-агностик — чистая логика. Рендеринг вынесен в `skia-renderer`.

pub mod camera;
pub mod culling;
pub mod interact;
pub mod layout;
pub mod scene;

pub use camera::{Aabb, Camera};
pub use culling::{cull, hit_test, CulledVo, ScreenRect};
pub use interact::{resize_rect, Interact, Mode, ResizeHandle};
pub use layout::{canvas_to_screen, layout_windows, screen_to_canvas, Quad, WindowQuad};
pub use scene::{
    place_new, CanvasWindow, ClusterId, Dir, FitPlan, OverviewPlan, Place, Scene, SnapResult,
    WindowId,
};
