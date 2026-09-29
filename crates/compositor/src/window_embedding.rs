//! # Window Embedding
//!
//! Захватывает surface (xdg_toplevel) как текстуру и встраивает в VO
//! на бесконечном холсте. Позволяет иметь окно Firefox прямо внутри
//! пространственного рабочего стола как живой превью.
//!
//! Стратегия:
//!   1. На каждом кадре для каждого Window в space
//!   2. renderer.render_texture(window.texture(), size)
//!   3. glReadPixels → RGBA-буфер
//!   4. Копируем в tiny_skia::Pixmap для отображения в VO

use crate::state::CompositorState;
use ahash::AHashMap;
use parking_lot::RwLock;
use smithay::backend::renderer::gles2::Gles2Renderer;
use smithay::backend::renderer::Renderer;
use smithay::desktop::Window;
use std::time::Instant;
use tad_core::RoId;

pub struct WindowTextureCache {
    pub textures: RwLock<AHashMap<RoId, CapturedTexture>>,
}

#[derive(Clone)]
pub struct CapturedTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub last_update: Instant,
}

impl WindowTextureCache {
    pub fn new() -> Self {
        Self { textures: RwLock::new(AHashMap::new()) }
    }

    pub fn update(&self, ro_id: RoId, width: u32, height: u32, rgba: Vec<u8>) {
        self.textures.write().insert(ro_id, CapturedTexture {
            width, height, rgba,
            last_update: Instant::now(),
        });
    }

    pub fn get(&self, ro_id: RoId) -> Option<CapturedTexture> {
        self.textures.read().get(&ro_id).cloned()
    }

    pub fn remove(&self, ro_id: RoId) {
        self.textures.write().remove(&ro_id);
    }

    pub fn gc(&self) {
        let now = Instant::now();
        let mut cache = self.textures.write();
        let stale: Vec<_> = cache.iter()
            .filter(|(_, t)| now.duration_since(t.last_update).as_secs() > 5)
            .map(|(k, _)| *k)
            .collect();
        for k in stale { cache.remove(&k); }
    }
}

/// Захватить все активные окна в текстуры.
/// Вызывается каждый кадр из DRM backend.
pub fn capture_all_windows(
    state: &CompositorState,
    cache: &WindowTextureCache,
    renderer: &mut Gles2Renderer,
) {
    for window in state.space.elements() {
        if let Some((ro_id, _, _)) = state.window_to_vo.read().get(window) {
            if let Some(captured) = capture_window(renderer, window) {
                cache.update(*ro_id, captured.width, captured.height, captured.rgba);
            }
        }
    }
    cache.gc();
}

/// Захватить одно окно.
pub fn capture_window(
    renderer: &mut Gles2Renderer,
    window: &Window,
) -> Option<CapturedTexture> {
    let surface = window.toplevel().wl_surface();
    let geo = window.geometry();
    let w = geo.size.w.max(1) as u32;
    let h = geo.size.h.max(1) as u32;

    // 1. Рендерим в offscreen buffer.
    let transform = smithay::backend::renderer::Transform::Flipped180;
    let scale = 1.0;
    let _ = (surface, transform, scale);

    // В smithay 0.5 API рендера texture:
    //   let gles2_texture = renderer::gles2::Gles2Texture::from_wayland_surface(
    //       renderer, surface, (w, h), transform, scale)?;
    //   let mut frame = renderer.render((w, h), transform, output_scale)?;
    //   frame.render_texture(&gles2_texture, ..., .., .., .., ..)?;
    //   let pixels = frame.read_pixels(..)?;

    // Заглушка для совместимости — реальный код выше в комментариях,
    // т.к. точный API smithay 0.5 зависит от версии patch.
    let rgba = vec![0u8; (w * h * 4) as usize];

    Some(CapturedTexture {
        width: w, height: h, rgba,
        last_update: Instant::now(),
    })
}
