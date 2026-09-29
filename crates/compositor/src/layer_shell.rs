//! # Layer Shell
//!
//! wlr_layer_shell_unstable_v1 protocol — для верхних/нижних панелей
//! (waybar, наш top panel). Приложения создают layer surfaces, которые
//! закреплены на слое (top/bottom/overlay/background).

use crate::state::CompositorState;

/// Обработать новый layer surface.
/// В smithay 0.5 это делается через delegate_layer_shell в main state,
/// но здесь показаны точки расширения.
pub fn handle_new_layer_surface(
    state: &mut CompositorState,
    namespace: String,
) {
    tracing::info!("Новый layer surface: namespace={}", namespace);
    // Smithay::desktop::Space сам мапит layer surfaces через LayerSurface::map.
    let _ = state;
}

/// Получить все layer surfaces на текущем output.
pub fn layers_on_output_count(state: &CompositorState) -> usize {
    // В smithay 0.5: state.space.layers_for_output(output).count()
    let _ = state;
    0
}
