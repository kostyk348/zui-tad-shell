//! # ZUI-TAD Compositor — Wayland DE (smithay 0.5)

pub use de_common::{launcher, workspaces};

use anyhow::Result;
use parking_lot::Mutex;
use std::sync::Arc;
use tad_core::GraphStore;

#[cfg(feature = "smithay")]
mod backend;
#[cfg(feature = "smithay")]
mod handlers;
#[cfg(feature = "smithay")]
mod shell;
#[cfg(feature = "smithay")]
mod state;
#[cfg(feature = "smithay")]
mod windows;

#[cfg(feature = "smithay")]
pub use state::CompositorState;

#[cfg(feature = "smithay")]
smithay::delegate_compositor!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_xdg_shell!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_shm!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_seat!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_data_device!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_layer_shell!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_foreign_toplevel_list!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_session_lock!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_output!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_xdg_decoration!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_primary_selection!(CompositorState);
#[cfg(feature = "smithay")]
smithay::delegate_xdg_activation!(CompositorState);

/// Start Wayland compositor or explain embedded mode.
pub fn run_compositor(store: Arc<Mutex<GraphStore>>, embedded: bool) -> Result<()> {
    #[cfg(feature = "smithay")]
    {
        return backend::run_compositor(store, embedded);
    }

    #[cfg(not(feature = "smithay"))]
    {
        let _ = (store, embedded);
        anyhow::bail!(
            "Wayland compositor not built. Use:\n\
             cargo build --release --features compositor/smithay\n\
             Or run embedded ZUI: zui-tad-shell"
        )
    }
}
