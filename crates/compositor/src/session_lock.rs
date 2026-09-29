//! # Session Lock
//!
//! ext_session_lock_v1 protocol — блокировка экрана на уровне композитора.
//! Реализуется после миграции на smithay 0.5 (см. docs/UPGRADE_TO_SMITHAY_0.5.md).

use crate::state::CompositorState;

pub fn handle_lock(state: &mut CompositorState) {
    state.locked = true;
    tracing::info!("Session locked");
}

pub fn handle_unlock(state: &mut CompositorState) {
    state.locked = false;
    tracing::info!("Session unlocked");
}

pub fn is_locked(state: &CompositorState) -> bool {
    state.locked
}
