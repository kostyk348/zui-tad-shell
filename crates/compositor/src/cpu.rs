//! CPU-композитор: собрать кадр холста в `Pixmap` без GL.
//!
//! Два потребителя: `wlr-screencopy` (отдать кадр клиенту) и DRM/TTY
//! (отрисовать в dumb buffer и сделать page-flip).
//!
//! Работает потому, что мы НЕ регистрируем `zwp_linux_dmabuf`: все клиенты
//! рисуют через `wl_shm`, значит их пиксели читаются напрямую
//! (`with_buffer_contents`), без GL и без readback.

use canvas_engine::layout_windows;
use phosphor::blit;
use phosphor::texture::{texture, TexKind};
use phosphor::theme::Palette;
use phosphor::widgets::{draw_text, frame};
use smithay::reexports::wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm};
use smithay::wayland::compositor::{with_states, BufferAssignment, SurfaceAttributes};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shm::with_buffer_contents;
use tiny_skia::{Pixmap, PixmapPaint, Transform};

use crate::shell::ShellCtx;
use crate::state::CompositorState;

/// Собрать кадр холста в пиксмап заданного размера.
pub fn compose(state: &mut CompositorState, size: (u32, u32)) -> Pixmap {
    let (w, h) = size;
    let pal = Palette::of(state.shell.mode);
    let mut pm = Pixmap::new(w.max(1), h.max(1)).unwrap();
    blit::fill(&mut pm, pal.bg);
    let quads = layout_windows(&state.canvas, &state.camera, state.camera.viewport);

    for q in &quads {
        if q.suspended {
            continue;
        }
        let Some(entry) = state.entry_by_id(q.id) else {
            continue;
        };
        let Some(surface) = entry.window.wl_surface() else {
            continue;
        };
        let buffer = with_states(&surface, |states| {
            match states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .buffer
                .as_ref()
            {
                Some(BufferAssignment::NewBuffer(b)) => Some(b.clone()),
                _ => None,
            }
        });
        let Some(buffer) = buffer else { continue };
        let _ = with_buffer_contents(&buffer, |ptr, len, data| {
            if data.width <= 0 || data.height <= 0 || data.stride <= 0 {
                return;
            }
            // Безопасно: указатель валиден на время вызова (гарантия smithay).
            let src = unsafe { std::slice::from_raw_parts(ptr, len) };
            let swap = matches!(
                data.format,
                wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888
            );
            blit::blit_raw_scaled(
                &mut pm,
                src,
                data.width as u32,
                data.height as u32,
                data.stride as usize,
                q.quad.x.round() as i32,
                q.quad.y.round() as i32,
                q.quad.w.max(1.0) as u32,
                q.quad.h.max(1.0) as u32,
                swap,
            );
        });
    }

    // Усыплённые окна — те же плейсхолдеры, что рисует слой оболочки.
    let placeholders: Vec<(f32, f32, f32, f32, String)> = state
        .canvas
        .windows()
        .iter()
        .filter(|x| x.suspended)
        .map(|x| (x.pos.x, x.pos.y, x.size.x, x.size.y, x.title.clone()))
        .collect();
    if let Some(fonts) = state.shell.fonts() {
        let m = state.shell.metrics();
        let (cx, cy) = (state.camera.center.x, state.camera.center.y);
        let z = state.camera.zoom.max(0.02);
        for (wx, wy, ww, wh, title) in &placeholders {
            let sx = (wx - cx) * z + w as f32 * 0.5;
            let sy = (wy - cy) * z + h as f32 * 0.5;
            let (sw, sh) = (ww * z, wh * z);
            if sx > w as f32 || sy > h as f32 || sx + sw < 0.0 || sy + sh < 0.0 {
                continue;
            }
            let c = phosphor::demo::with_alpha(pal.cold, 0x99);
            frame(&mut pm, (sx, sy, sw, sh), c, m.line, 6.0);
            texture(&mut pm, (sx, sy, sw, sh), TexKind::Bands, c, 6.0, 3);
            if sh > 40.0 {
                draw_text(
                    &mut pm,
                    &fonts.bold,
                    &title.to_uppercase(),
                    sx + 10.0,
                    sy + 18.0,
                    m.value_size,
                    m.tracking,
                    c,
                );
            }
        }
    }

    // Слой оболочки (панель/HUD/меню) с альфой → SourceOver.
    let clock = chrono::Local::now();
    let ctx = ShellCtx {
        w,
        h,
        camera: (state.camera.center.x, state.camera.center.y),
        zoom: state.camera.zoom,
        windows: &[],
        bounds: (0.0, 0.0, 1.0, 1.0),
        viewport: (0.0, 0.0, w as f32, h as f32),
        suspended: placeholders.len(),
        clusters: state.canvas.clusters().len(),
        frame_ms: state.shell_frame_ms,
        quality: state.shell.quality,
        title: "zui-tad",
        clock: clock.format("%H:%M").to_string(),
        date: clock.format("%d %b").to_string().to_uppercase(),
        toast: None,
        placeholders: &placeholders,
    };
    let shell_pm = state.shell.render(&ctx);
    pm.draw_pixmap(
        0,
        0,
        shell_pm.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    pm
}

/// Записать кадр в буфер клиента (`wl_shm`) — для screencopy.
/// `region` — прямоугольник захвата в координатах кадра.
/// Возвращает true, если буфер принят и записан.
pub fn write_into_shm(
    buffer: &WlBuffer,
    frame: &Pixmap,
    region: Option<(i32, i32, i32, i32)>,
) -> bool {
    let fw = frame.width() as usize;
    let fh = frame.height() as usize;
    let src_data = frame.data().to_vec();
    with_buffer_contents(buffer, |ptr, len, bd| {
        if bd.width <= 0 || bd.height <= 0 || bd.stride <= 0 {
            return false;
        }
        let (bw, bh) = (bd.width as usize, bd.height as usize);
        let stride = bd.stride as usize;
        let offset = bd.offset.max(0) as usize;
        if len < offset + stride * bh {
            return false;
        }
        let swap = matches!(
            bd.format,
            wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888
        );
        // Безопасно: ptr валиден на время вызова (гарантия smithay).
        let dst = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, len) };
        let (rx, ry) = region.map(|(x, y, _, _)| (x, y)).unwrap_or((0, 0));
        for y in 0..bh {
            for x in 0..bw {
                let (sx, sy) = (rx + x as i32, ry + y as i32);
                if sx < 0 || sy < 0 || sx as usize >= fw || sy as usize >= fh {
                    continue;
                }
                let s = (sy as usize * fw + sx as usize) * 4;
                let d = offset + y * stride + x * 4;
                let (r, g, b) = (src_data[s], src_data[s + 1], src_data[s + 2]);
                if swap {
                    dst[d] = b;
                    dst[d + 1] = g;
                    dst[d + 2] = r;
                } else {
                    dst[d] = r;
                    dst[d + 1] = g;
                    dst[d + 2] = b;
                }
                if d + 3 < dst.len() {
                    dst[d + 3] = 255;
                }
            }
        }
        true
    })
    .unwrap_or(false)
}
