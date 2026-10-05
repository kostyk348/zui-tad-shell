//! Wayland protocol handlers (smithay 0.5).

use std::os::unix::io::OwnedFd;

use crate::state::{ClientData, CompositorState};
use smithay::{
    input::{pointer::CursorImageStatus, Seat, SeatHandler, SeatState},
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_output::WlOutput, wl_seat, wl_surface::WlSurface},
        Client,
    },
    utils::Serial,
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState as SmithayCompositorState,
        },
        foreign_toplevel_list::{ForeignToplevelListHandler, ForeignToplevelListState},
        output::OutputHandler,
        selection::{
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
            SelectionHandler,
        },
        session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker},
        shell::{
            wlr_layer::{Layer, LayerSurface, WlrLayerShellHandler, WlrLayerShellState},
            xdg::{PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState},
        },
        shm::{ShmHandler, ShmState},
    },
};
use wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use wayland_protocols::xdg::shell::server::xdg_toplevel;

impl BufferHandler for CompositorState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl CompositorHandler for CompositorState {
    fn compositor_state(&mut self) -> &mut SmithayCompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientData>().unwrap().compositor
    }

    fn commit(&mut self, surface: &WlSurface) {
        smithay::backend::renderer::utils::on_commit_buffer_handler::<Self>(surface);
        // Обновить bbox окна (нужно для корректного render/hit-test).
        if let Some(entry) = self.entry_by_surface(surface) {
            entry.window.on_commit();
        }
        // Клиентская поверхность должна «войти» в выход, иначе не поедут кадры.
        self.enter_outputs(surface);
    }
}

impl XdgShellHandler for CompositorState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // title/app_id живут в XdgToplevelSurfaceData, достаём через with_states.
        let (title, app_id) =
            smithay::wayland::compositor::with_states(surface.wl_surface(), |states| {
                let attrs = states
                    .data_map
                    .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                    .expect("xdg toplevel must have XdgToplevelSurfaceData")
                    .lock()
                    .unwrap();
                (attrs.title.clone(), attrs.app_id.clone())
            });
        let title = title.unwrap_or_else(|| format!("window-{}", self.entries.len() + 1));
        let app_id = app_id.unwrap_or_else(|| "wayland-client".to_string());

        // Новое окно получает нативный размер; холст сам решит, где его положить.
        let (w, h) = (900i32, 600i32);
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Activated);
            state.size = Some(smithay::utils::Size::from((w, h)));
        });
        surface.send_configure();

        let handle = self
            .foreign_toplevel_list
            .new_toplevel::<CompositorState>(&title, &app_id);
        self.toplevel_handles.write().push(handle);

        let id = self.insert_toplevel(surface, title.clone(), app_id, (w as u32, h as u32));
        // Новое окно сразу получает фокус ввода (как в любом WM) — иначе
        // горячие клавиши не доходят до нас, пока не кликнешь мышью.
        let serial = self.next_serial();
        self.focus_window(id, serial);
        tracing::info!("New toplevel: {title} canvas_id={id}");
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        let _ = surface;
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    /// Клиент просит интерактивное перемещение (тащит свой CSD-заголовок).
    /// Именно это делает окна «как везде»: drag заголовка работает.
    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        if let Some(id) = self.id_for_surface(surface.wl_surface()) {
            let pt = self.canvas_point();
            self.focus_window(id, serial);
            self.interact.begin_move(id, pt, self.pointer_pos);
            tracing::info!("move_request -> canvas drag id={id}");
        }
    }

    /// Клиент просит интерактивный ресайз (потянул за свой угол/край).
    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        // без `use ...::*`: варианты `None` затеняют Option::None
        let handle = match edges {
            xdg_toplevel::ResizeEdge::Top => canvas_engine::ResizeHandle::N,
            xdg_toplevel::ResizeEdge::Bottom => canvas_engine::ResizeHandle::S,
            xdg_toplevel::ResizeEdge::Left => canvas_engine::ResizeHandle::W,
            xdg_toplevel::ResizeEdge::Right => canvas_engine::ResizeHandle::E,
            xdg_toplevel::ResizeEdge::TopLeft => canvas_engine::ResizeHandle::Nw,
            xdg_toplevel::ResizeEdge::TopRight => canvas_engine::ResizeHandle::Ne,
            xdg_toplevel::ResizeEdge::BottomLeft => canvas_engine::ResizeHandle::Sw,
            xdg_toplevel::ResizeEdge::BottomRight => canvas_engine::ResizeHandle::Se,
            xdg_toplevel::ResizeEdge::None => return,
            _ => return,
        };
        if let Some(id) = self.id_for_surface(surface.wl_surface()) {
            let (pos, size) = match self.canvas.get(id) {
                Some(w) => (w.pos, w.size),
                None => return,
            };
            let pt = self.canvas_point();
            self.focus_window(id, serial);
            self.interact
                .begin_resize(id, handle, pos, size, pt, self.pointer_pos);
            tracing::info!("resize_request -> canvas resize id={id}");
        }
    }

    /// Maximize на холсте = fit-window (окно растягивается, камера центруется).
    fn maximize_request(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.id_for_surface(surface.wl_surface()) else {
            return;
        };
        if let Some(entry) = self.entry_by_id(id) {
            if entry.maximized {
                return;
            }
        }
        if let Some(w) = self.canvas.get(id) {
            let saved = (w.pos, w.size);
            if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
                entry.saved = Some(saved);
                entry.maximized = true;
            }
        }
        let viewport = self.viewport();
        if let Some(plan) = self.canvas.fit_window_plan(id, viewport, 0.0) {
            self.camera.center = plan.camera_center;
            self.camera.zoom = plan.zoom;
            self.canvas.move_to(id, plan.window_pos);
            self.canvas.resize(id, plan.window_size);
            configure_with(self, id, &[xdg_toplevel::State::Maximized], &[]);
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.id_for_surface(surface.wl_surface()) else {
            return;
        };
        let saved = self.entries.iter_mut().find(|e| e.id == id).and_then(|e| {
            e.maximized = false;
            e.saved.take()
        });
        if let Some((pos, size)) = saved {
            self.canvas.move_to(id, pos);
            self.canvas.resize(id, size);
            configure_with(self, id, &[], &[xdg_toplevel::State::Maximized]);
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        let Some(id) = self.id_for_surface(surface.wl_surface()) else {
            return;
        };
        if let Some(w) = self.canvas.get(id) {
            let saved = (w.pos, w.size);
            if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
                entry.saved = Some(saved);
                entry.fullscreen = true;
            }
        }
        let viewport = self.viewport();
        if let Some(plan) = self.canvas.fit_window_plan(id, viewport, 0.0) {
            self.camera.center = plan.camera_center;
            self.camera.zoom = 1.0;
            self.canvas.move_to(id, plan.window_pos);
            self.canvas.resize(id, plan.window_size);
            configure_with(self, id, &[xdg_toplevel::State::Fullscreen], &[]);
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.id_for_surface(surface.wl_surface()) else {
            return;
        };
        let saved = self.entries.iter_mut().find(|e| e.id == id).and_then(|e| {
            e.fullscreen = false;
            e.saved.take()
        });
        if let Some((pos, size)) = saved {
            self.canvas.move_to(id, pos);
            self.canvas.resize(id, size);
            configure_with(self, id, &[], &[xdg_toplevel::State::Fullscreen]);
        }
    }

    /// Minimize = suspend: окно уходит в плейсхолдер на своём месте холста.
    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self.id_for_surface(surface.wl_surface()) {
            self.canvas.suspend(id);
            tracing::info!("minimize_request -> suspend id={id}");
        }
    }

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
    }

    /// Клиент поменял заголовок — обновляем запись на холсте (панель/HUD/сессия).
    fn title_changed(&mut self, surface: ToplevelSurface) {
        let title = surface_role_strings(surface.wl_surface())
            .0
            .unwrap_or_default();
        if let Some(id) = self.id_for_surface(surface.wl_surface()) {
            if let Some(w) = self.canvas.get_mut(id) {
                if !title.is_empty() && w.title != title {
                    w.title = title;
                }
            }
        }
    }

    /// Клиент поменял app_id — важно для сессии (усыновление места) и правил окон.
    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        let app_id = surface_role_strings(surface.wl_surface())
            .1
            .unwrap_or_default();
        if let Some(id) = self.id_for_surface(surface.wl_surface()) {
            if let Some(w) = self.canvas.get_mut(id) {
                if !app_id.is_empty() && w.app_id != app_id {
                    tracing::info!("app_id: {id} -> {app_id}");
                    w.app_id = app_id;
                }
            }
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.remove_toplevel(&surface);
    }
}

impl WlrLayerShellHandler for CompositorState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell
    }

    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        _output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        tracing::info!("Layer surface: {namespace}");
        let _ = surface;
    }

    fn layer_destroyed(&mut self, _surface: LayerSurface) {}
}

impl SelectionHandler for CompositorState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for CompositorState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device
    }
}

impl ClientDndGrabHandler for CompositorState {}
impl ServerDndGrabHandler for CompositorState {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl ShmHandler for CompositorState {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}

impl SeatHandler for CompositorState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<CompositorState> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}
    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        match image {
            CursorImageStatus::Hidden => {
                self.cursor_hidden = true;
                self.cursor_surface = None;
                self.cursor_named = None;
            }
            CursorImageStatus::Surface(surface) => {
                // Клиент рисует свой курсор — покажем его поверх всего.
                self.cursor_hidden = false;
                self.cursor_named = None;
                self.cursor_surface = Some(surface);
            }
            CursorImageStatus::Named(icon) => {
                // cursor-shape-v1: smithay и winit используют ОДИН тип
                // (cursor_icon::CursorIcon), поэтому просто пробрасываем иконку.
                self.cursor_hidden = false;
                self.cursor_surface = None;
                self.cursor_named = Some(icon);
            }
        }
    }
}

impl OutputHandler for CompositorState {}

impl ForeignToplevelListHandler for CompositorState {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.foreign_toplevel_list
    }
}

impl SessionLockHandler for CompositorState {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        self.lock();
        confirmation.lock();
    }

    fn unlock(&mut self) {
        self.unlock();
    }

    fn new_surface(&mut self, _surface: LockSurface, _output: WlOutput) {}
}

/// Прочитать (title, app_id) из роли xdg-toplevel: они живут в
/// `XdgToplevelSurfaceData`, а не в pending-state.
fn surface_role_strings(surface: &WlSurface) -> (Option<String>, Option<String>) {
    smithay::wayland::compositor::with_states(surface, |states| {
        match states
            .data_map
            .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
        {
            Some(data) => {
                let attrs = data.lock().unwrap();
                (attrs.title.clone(), attrs.app_id.clone())
            }
            None => (None, None),
        }
    })
}

/// Отправить клиенту новый размер холста + изменения состояний (maximize/fullscreen).
fn configure_with(
    state: &mut CompositorState,
    id: canvas_engine::WindowId,
    set: &[xdg_toplevel::State],
    unset: &[xdg_toplevel::State],
) {
    let Some((w, h)) = state
        .canvas
        .get(id)
        .map(|win| (win.size.x.round() as i32, win.size.y.round() as i32))
    else {
        return;
    };
    if let Some(entry) = state.entry_by_id(id) {
        if let Some(tl) = entry.window.toplevel() {
            tl.with_pending_state(|s| {
                s.size = Some(smithay::utils::Size::from((w, h)));
                for st in set {
                    s.states.set(*st);
                }
                for st in unset {
                    s.states.unset(*st);
                }
            });
            tl.send_configure();
        }
    }
}

impl smithay::wayland::shell::xdg::decoration::XdgDecorationHandler for CompositorState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        // Своих серверных декораций у нас нет (холст — не стекинг), поэтому
        // честно просим клиента рисовать CSD.
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(zxdg_toplevel_decoration_v1::Mode::ClientSide);
        });
        toplevel.send_configure();
    }

    fn request_mode(
        &mut self,
        toplevel: ToplevelSurface,
        _mode: zxdg_toplevel_decoration_v1::Mode,
    ) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(zxdg_toplevel_decoration_v1::Mode::ClientSide);
        });
        toplevel.send_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(zxdg_toplevel_decoration_v1::Mode::ClientSide);
        });
        toplevel.send_configure();
    }
}

impl smithay::wayland::selection::primary_selection::PrimarySelectionHandler for CompositorState {
    fn primary_selection_state(
        &self,
    ) -> &smithay::wayland::selection::primary_selection::PrimarySelectionState {
        &self.primary_selection
    }
}

impl smithay::wayland::xdg_activation::XdgActivationHandler for CompositorState {
    fn activation_state(&mut self) -> &mut smithay::wayland::xdg_activation::XdgActivationState {
        &mut self.activation
    }

    /// Клиент просит активировать поверхность («открой это окно»).
    fn request_activation(
        &mut self,
        token: smithay::wayland::xdg_activation::XdgActivationToken,
        _data: smithay::wayland::xdg_activation::XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if let Some(id) = self.id_for_surface(&surface) {
            let serial = self.next_serial();
            self.focus_window(id, serial);
            tracing::info!("xdg-activation: activated canvas_id={id}");
        } else {
            tracing::info!("xdg-activation: surface is not a canvas window");
        }
        let _ = self.activation.remove_token(&token);
    }
}

impl smithay::wayland::idle_notify::IdleNotifierHandler for CompositorState {
    fn idle_notifier_state(&mut self) -> &mut smithay::wayland::idle_notify::IdleNotifierState<Self> {
        self.idle.as_mut().expect("idle-notify не инициализирован")
    }
}
