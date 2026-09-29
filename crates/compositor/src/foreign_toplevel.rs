//! # Foreign Toplevel Management
//!
//! wlr_foreign_toplevel_management_v1 protocol — позволяет внешним приложениям
//! (waybar, taskbar) видеть список открытых окон, переключаться между ними,
//! закрывать, минимизировать.
//!
//! Без этого waybar не покажет список окон.

use crate::state::CompositorState;
use tad_core::RoId;

/// Зарегистрировать окно в foreign toplevel manager.
pub fn register_window(state: &CompositorState, ro_id: RoId, title: &str, app_id: &str) {
    let _ = state;
    let _ = ro_id;
    tracing::info!("Foreign toplevel: {} ({})", title, app_id);
    // В smithay 0.5 ForeignToplevelManagerState сам создаёт handle'ы
    // при появлении новых toplevel'ов — нужно только делегировать.
}

/// Закрыть окно.
pub fn close_window(state: &CompositorState) {
    let _ = state;
}

/// Активировать окно (поднять на передний план).
pub fn activate(state: &CompositorState) {
    let _ = state;
}

/// Минимизировать.
pub fn minimize(state: &CompositorState) {
    let _ = state;
}
