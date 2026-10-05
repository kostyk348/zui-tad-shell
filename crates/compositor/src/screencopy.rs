//! `wlr-screencopy`: скриншоты и шаринг экрана (grim, Firefox, OBS…).
//!
//! Кадр собирает НАШ CPU-композитор (`cpu::compose`) и пишет прямо в буфер
//! клиента: GL-readback не нужен, потому что dmabuf мы не регистрируем и все
//! клиенты идут через `wl_shm`. Клиент просит кадр → попадает в очередь →
//! обрабатывается после очередного рендера (там уже известен размер вывода).

use smithay::reexports::wayland_server::{
    backend::GlobalId,
    protocol::{wl_buffer, wl_shm},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use wayland_protocols_wlr::screencopy::v1::server::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
};

use crate::state::CompositorState;

/// Запрос на кадр: ждёт ближайшего рендера.
pub struct CaptureRequest {
    pub frame: ZwlrScreencopyFrameV1,
    pub buffer: wl_buffer::WlBuffer,
    /// Прямоугольник захвата (для capture_output_region).
    pub region: Option<(i32, i32, i32, i32)>,
    pub with_damage: bool,
}

pub struct ScreencopyState {
    global: GlobalId,
    pub queue: Vec<CaptureRequest>,
}

impl ScreencopyState {
    pub fn new<D>(dh: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<ZwlrScreencopyManagerV1, ()>
            + Dispatch<ZwlrScreencopyManagerV1, ()>
            + Dispatch<ZwlrScreencopyFrameV1, ()>
            + 'static,
    {
        Self {
            global: dh.create_global::<D, ZwlrScreencopyManagerV1, _>(3, ()),
            queue: Vec::new(),
        }
    }

    pub fn global(&self) -> &GlobalId {
        &self.global
    }
}

impl GlobalDispatch<ZwlrScreencopyManagerV1, ()> for CompositorState {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrScreencopyManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        // Bind обязан создать объект — иначе wayland-server паникует.
        let _manager = data_init.init(resource, ());
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for CompositorState {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput {
                frame,
                overlay_cursor: _,
                output: _,
            } => {
                // `frame` приходит как New<..> — материализуем объект.
                let frame = data_init.init(frame, ());
                let (w, h) = state.output_size();
                // Объявляем формат и размер кадра (только shm — dmabuf не умеем).
                frame.buffer(wl_shm::Format::Xrgb8888, w as u32, h as u32, (w * 4) as u32);
                if frame.version() >= 3 {
                    frame.buffer_done();
                }
            }
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                overlay_cursor: _,
                output: _,
                x,
                y,
                width,
                height,
            } => {
                let frame = data_init.init(frame, ());
                let (w, h) = state.output_size();
                frame.buffer(
                    wl_shm::Format::Xrgb8888,
                    width.max(1) as u32,
                    height.max(1) as u32,
                    (width.max(1) * 4) as u32,
                );
                if frame.version() >= 3 {
                    frame.buffer_done();
                }
                let _ = (x, y, w, h);
            }
            zwlr_screencopy_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for CompositorState {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => {
                queue_capture(state, resource, buffer, None, false)
            }
            zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => {
                queue_capture(state, resource, buffer, None, true)
            }
            zwlr_screencopy_frame_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

fn queue_capture(
    state: &mut CompositorState,
    frame: &ZwlrScreencopyFrameV1,
    buffer: wl_buffer::WlBuffer,
    region: Option<(i32, i32, i32, i32)>,
    with_damage: bool,
) {
    let (w, h) = state.output_size();
    let ok = smithay::wayland::shm::with_buffer_contents(&buffer, |_p, len, d| {
        matches!(
            d.format,
            wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888
        ) && d.width == w
            && d.height == h
            && len >= (d.stride as usize) * (d.height as usize)
    })
    .unwrap_or(false);
    if !ok {
        frame.failed();
        return;
    }
    state.screencopy.queue.push(CaptureRequest {
        frame: frame.clone(),
        buffer,
        region,
        with_damage,
    });
}

/// Обработать очередь захватов: собрать кадр CPU-композитором и отдать клиентам.
/// Вызывается из цикла рендера (только если очередь не пуста).
pub fn process(state: &mut CompositorState) {
    if state.screencopy.queue.is_empty() {
        return;
    }
    let (w, h) = state.output_size();
    let frame_pm = crate::cpu::compose(state, (w.max(1) as u32, h.max(1) as u32));
    let queued: Vec<CaptureRequest> = state.screencopy.queue.drain(..).collect();
    for req in queued {
        if crate::cpu::write_into_shm(&req.buffer, &frame_pm, req.region) {
            if req.with_damage && req.frame.version() >= 2 {
                let (rw, rh) = req.region.map(|(_, _, rw, rh)| (rw, rh)).unwrap_or((w, h));
                req.frame.damage(0, 0, rw as u32, rh as u32);
            }
            // ready(tv_sec_hi, tv_sec_lo, tv_nsec) — метка времени кадра.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let (secs, nanos) = (now.as_secs(), now.subsec_nanos());
            req.frame
                .ready((secs >> 32) as u32, (secs & 0xffff_ffff) as u32, nanos);
        } else {
            req.frame.failed();
        }
    }
}
