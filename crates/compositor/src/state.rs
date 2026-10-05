//! # CompositorState — Wayland DE state (smithay 0.5)
//!
//! Здесь живёт МОСТ между Wayland и холстом: каждый `xdg_toplevel` получает
//! `WindowId` в `canvas_engine::Scene` (мировые координаты), а экранные
//! координаты считаются на лету в `layout_windows`. Ключевое отличие от
//! стекинг-композитора: у окна нет «экранной позиции», у него есть холстовая.

use ahash::AHashMap;
use canvas_engine::{Camera, Interact, Place, Scene, WindowId};
use cgmath::Point2;
use de_common::workspaces::WorkspaceManager;
use parking_lot::{Mutex, RwLock};
use smithay::{
    desktop::Window,
    input::{keyboard::KeyboardHandle, pointer::PointerHandle, Seat, SeatState},
    output::Output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Display, DisplayHandle},
    utils::Serial,
    wayland::{
        compositor::CompositorState as SmithayCompositorState,
        content_type::ContentTypeState,
        foreign_toplevel_list::{ForeignToplevelHandle, ForeignToplevelListState},
        idle_notify::IdleNotifierState,
        output::OutputManagerState,
        seat::WaylandFocus,
        selection::{data_device::DataDeviceState, primary_selection::PrimarySelectionState},
        session_lock::SessionLockManagerState,
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{decoration::XdgDecorationState, ToplevelSurface, XdgShellState},
        },
        shm::ShmState,
        single_pixel_buffer::SinglePixelBufferState,
        viewporter::ViewporterState,
        xdg_activation::XdgActivationState,
    },
};
use std::sync::Arc;
use std::time::Instant;
use tad_core::{GraphStore, RoId, VirtualObject, VoId};
use uuid::Uuid;

use crate::shell::ShellLayer;
use std::cell::RefCell;
use std::rc::Rc;

/// Графический бэкенд (winit+GLES), которым владеет calloop-цикл.
pub type Gfx = smithay::backend::winit::WinitGraphicsBackend<
    smithay::backend::renderer::gles::GlesRenderer,
>;
use crate::windows::WindowTracker;

/// Per-client compositor data (required by smithay 0.5).
#[derive(Default)]
pub struct ClientData {
    pub compositor: smithay::wayland::compositor::CompositorClientState,
}

impl wayland_server::backend::ClientData for ClientData {
    fn initialized(&self, _client_id: wayland_server::backend::ClientId) {}
    fn disconnected(
        &self,
        _client_id: wayland_server::backend::ClientId,
        _reason: wayland_server::backend::DisconnectReason,
    ) {
    }
}

/// Окно композитора + его идентификатор на холсте.
pub struct WindowEntry {
    pub window: Window,
    pub id: WindowId,
    /// Прямоугольник до maximize/fullscreen — чтобы было куда вернуться.
    pub saved: Option<(Point2<f32>, cgmath::Vector2<f32>)>,
    pub maximized: bool,
    pub fullscreen: bool,
}

pub struct CompositorState {
    pub display_handle: DisplayHandle,
    pub store: Arc<Mutex<GraphStore>>,
    pub all_vos: RwLock<Vec<VirtualObject>>,
    pub ro_index: RwLock<AHashMap<RoId, tad_core::RealObject>>,

    pub compositor: SmithayCompositorState,
    pub xdg_shell: XdgShellState,
    pub layer_shell: WlrLayerShellState,
    pub shm: ShmState,
    pub data_device: DataDeviceState,
    pub foreign_toplevel_list: ForeignToplevelListState,
    pub session_lock: SessionLockManagerState,
    /// xdg-decoration: просим клиентов рисовать свои декорации (CSD).
    pub decoration: XdgDecorationState,
    /// primary selection (средняя кнопка мыши / выделение).
    pub primary_selection: PrimarySelectionState,
    /// xdg-activation: «открой то окно» (ссылки, .desktop, мессенджеры).
    pub activation: XdgActivationState,
    /// wp_viewporter — ОБЯЗАТЕЛЕН для xwayland-satellite (иначе X11 не поднять).
    pub viewporter: ViewporterState,
    /// wp_single_pixel_buffer_v1 — клиенты рисуют сплошные поверхности дешевле.
    pub single_pixel_buffer: SinglePixelBufferState,
    /// wp_content_type_v1 — клиенты сообщают «это видео/игра» (нужно для полноэкранного).
    pub content_type: ContentTypeState,
    /// wlr-screencopy: очередь запросов на кадр (обрабатывается CPU-композитором).
    pub screencopy: crate::screencopy::ScreencopyState,
    /// Менеджер выходов + xdg-output: нужен клиентам (grim, wlr-randr), чтобы
    /// узнать размеры выходов, а не «угадывать» их.
    pub output_manager: OutputManagerState,
    /// idle-notify: `ext-idle-notify-v1` (таймеры вставлены в calloop-цикл).
    pub idle: Option<IdleNotifierState<CompositorState>>,
    /// Графический бэкенд для calloop-цикла (Rc<RefCell>, т.к. calloop даёт
    /// только `&mut state`, а рендерер нужен снаружи состояния).
    pub gfx: Option<Rc<RefCell<Gfx>>>,

    pub seat_state: SeatState<CompositorState>,
    pub seat: Seat<CompositorState>,
    pub keyboard: Option<KeyboardHandle<CompositorState>>,
    pub pointer: Option<PointerHandle<CompositorState>>,

    pub outputs: Vec<Output>,
    pub windows: WindowTracker,
    pub workspaces: WorkspaceManager,
    pub started_at: Instant,
    pub launcher_visible: bool,
    pub launcher_query: String,
    pub current_workspace: u8,
    pub locked: bool,

    /// Мост Wayland ↔ холст.
    pub canvas: Scene,
    pub camera: Camera,
    /// Оболочка (панель/HUD/меню) — рисуется поверх окон.
    pub shell: ShellLayer,
    pub interact: Interact,
    pub entries: Vec<WindowEntry>,

    /// Курсор в физических пикселях вьюпорта.
    pub pointer_pos: Point2<f32>,
    /// Куда сохраняется сессия холста (JSON рядом с хранилищем).
    pub session_path: std::path::PathBuf,
    /// Когда последний раз сохраняли сессию.
    pub last_session_save: Instant,
    /// Подпись состояния холста (чтобы не писать файл без изменений).
    pub last_session_sig: u64,
    /// Сглаженное время кадра композитора (мс) — показываем в панели.
    pub shell_frame_ms: f32,
    /// Счётчик serial для seat-событий.
    pub serial_counter: u32,
    pub frame_time_ms: u32,
    /// Троттлинг xdg-configure при перетаскивании границ.
    pub last_configure: Instant,
    /// Детект двойного клика (время, окно) — заголовок = fit-window.
    pub last_click: Option<(Instant, WindowId)>,
    /// Клиент просит спрятать курсор.
    pub cursor_hidden: bool,
    /// Курсор-поверхность от клиента (свой битмап курсора).
    pub cursor_surface: Option<WlSurface>,
    /// Именованный курсор (cursor-shape-v1). Тип тот же, что у winit
    /// (smithay реэкспортирует `cursor_icon::CursorIcon`), поэтому маппинг не нужен.
    pub cursor_named: Option<smithay::input::pointer::CursorIcon>,
    pub cursor_hotspot: (i32, i32),

    /// Window title → foreign toplevel handle for waybar.
    pub toplevel_handles: RwLock<Vec<ForeignToplevelHandle>>,
}

impl CompositorState {
    pub fn new(display: &mut Display<CompositorState>, store: Arc<Mutex<GraphStore>>) -> Self {
        let display_handle = display.handle();

        let mut ro_index = AHashMap::new();
        let mut all_vos = Vec::new();
        for ro in store.lock().all_ro().unwrap_or_default() {
            ro_index.insert(ro.id, ro);
        }
        for parent in ro_index.keys().copied().collect::<Vec<_>>() {
            all_vos.extend(store.lock().children_of(parent).unwrap_or_default());
        }

        let compositor = SmithayCompositorState::new::<CompositorState>(&display_handle);
        let xdg_shell = XdgShellState::new::<CompositorState>(&display_handle);
        let layer_shell = WlrLayerShellState::new::<CompositorState>(&display_handle);
        let shm = ShmState::new::<CompositorState>(&display_handle, vec![]);
        let data_device = DataDeviceState::new::<CompositorState>(&display_handle);
        let foreign_toplevel_list =
            ForeignToplevelListState::new::<CompositorState>(&display_handle);
        let session_lock =
            SessionLockManagerState::new::<CompositorState, _>(&display_handle, |_| true);
        let decoration = XdgDecorationState::new::<CompositorState>(&display_handle);
        let primary_selection = PrimarySelectionState::new::<CompositorState>(&display_handle);
        let activation = XdgActivationState::new::<CompositorState>(&display_handle);
        let viewporter = ViewporterState::new::<CompositorState>(&display_handle);
        let single_pixel_buffer = SinglePixelBufferState::new::<CompositorState>(&display_handle);
        let content_type = ContentTypeState::new::<CompositorState>(&display_handle);
        let screencopy =
            crate::screencopy::ScreencopyState::new::<CompositorState>(&display_handle);
        let output_manager =
            OutputManagerState::new_with_xdg_output::<CompositorState>(&display_handle);

        let mut seat_state = SeatState::<CompositorState>::new();
        let seat = seat_state.new_wl_seat(&display_handle, "seat0");

        Self {
            display_handle,
            store,
            all_vos: RwLock::new(all_vos),
            ro_index: RwLock::new(ro_index),
            compositor,
            xdg_shell,
            layer_shell,
            shm,
            data_device,
            foreign_toplevel_list,
            session_lock,
            decoration,
            primary_selection,
            activation,
            viewporter,
            single_pixel_buffer,
            content_type,
            screencopy,
            output_manager,
            idle: None,
            gfx: None,
            seat_state,
            seat,
            keyboard: None,
            pointer: None,
            outputs: Vec::new(),
            windows: WindowTracker::new(),
            workspaces: WorkspaceManager::new(9),
            started_at: Instant::now(),
            launcher_visible: false,
            launcher_query: String::new(),
            current_workspace: 0,
            locked: false,
            canvas: Scene::new(),
            camera: Camera::new(cgmath::Vector2::new(1280, 800)),
            shell: ShellLayer::new(),
            interact: Interact::new(),
            entries: Vec::new(),
            pointer_pos: Point2::new(640.0, 400.0),
            session_path: std::path::PathBuf::from("data/session.json"),
            last_session_save: Instant::now(),
            last_session_sig: 0,
            shell_frame_ms: 0.0,
            serial_counter: 1,
            frame_time_ms: 0,
            last_configure: Instant::now() - std::time::Duration::from_secs(1),
            last_click: None,
            cursor_hidden: false,
            cursor_surface: None,
            cursor_named: None,
            cursor_hotspot: (0, 0),
            toplevel_handles: RwLock::new(Vec::new()),
        }
    }

    // ---------- вьюпорт / камера ----------

    /// Создать wl_output и опубликовать его клиентам.
    /// Без этого winit-клиенты (alacritty, foot) отказываются создавать окно.
    pub fn register_output(&mut self, w: u32, h: u32) {
        let output = Output::new(
            format!("ZUI-{}", self.outputs.len() + 1),
            smithay::output::PhysicalProperties {
                size: smithay::utils::Size::from((340, 190)),
                subpixel: smithay::output::Subpixel::Unknown,
                make: "ZUI-TAD".into(),
                model: "infinite-canvas".into(),
            },
        );
        output.create_global::<CompositorState>(&self.display_handle);
        let mode = smithay::output::Mode {
            size: smithay::utils::Size::from((w.max(1) as i32, h.max(1) as i32)),
            refresh: 60_000,
        };
        output.set_preferred(mode);
        output.change_current_state(Some(mode), None, None, None);
        self.outputs.push(output);
    }

    /// Обновить режим вывода (ресайз вложенного окна).
    pub fn update_output_mode(&mut self, w: u32, h: u32) {
        let mode = smithay::output::Mode {
            size: smithay::utils::Size::from((w.max(1) as i32, h.max(1) as i32)),
            refresh: 60_000,
        };
        for out in &self.outputs {
            out.set_preferred(mode);
            out.change_current_state(Some(mode), None, None, None);
        }
    }

    /// Клиентские поверхности «входят» во все выходы (нужно для фреймов клиента).
    pub fn enter_outputs(&self, surface: &WlSurface) {
        for out in &self.outputs {
            out.enter(surface);
        }
    }

    pub fn set_viewport(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        self.camera.viewport = cgmath::Vector2::new(w, h);
    }

    pub fn viewport(&self) -> cgmath::Vector2<u32> {
        self.camera.viewport
    }

    /// Размер вывода в физических пикселях (то, что отдаём клиентам).
    pub fn output_size(&self) -> (i32, i32) {
        let v = self.camera.viewport;
        (v.x.max(1) as i32, v.y.max(1) as i32)
    }

    pub fn next_serial(&mut self) -> Serial {
        self.serial_counter = self.serial_counter.wrapping_add(1);
        Serial::from(self.serial_counter)
    }

    /// Курсор → мировые координаты холста.
    pub fn canvas_point(&self) -> Point2<f32> {
        canvas_engine::screen_to_canvas(&self.camera, self.pointer_pos)
    }

    // ---------- окна ----------

    /// Зарегистрировать xdg_toplevel: создать Window, место на холсте, TAD-документ.
    pub fn insert_toplevel(
        &mut self,
        surface: ToplevelSurface,
        title: String,
        app_id: String,
        size: (u32, u32),
    ) -> WindowId {
        let window = Window::new_wayland_window(surface);
        let size_v = cgmath::Vector2::new(size.0 as f32, size.1 as f32);
        // Если такое приложение было в восстановленной сессии — оно «усыновляет»
        // своё место (dormant-плейсхолдер убираем), иначе кладём в центр камеры.
        let adopted = self
            .canvas
            .windows()
            .iter()
            .find(|w| w.suspended && w.app_id == app_id)
            .map(|w| (w.id, w.pos, w.size));
        let pos = match adopted {
            Some((old_id, p, s)) => {
                self.canvas.remove(old_id);
                tracing::info!("adopted session slot for app_id={app_id}");
                p
            }
            None => canvas_engine::place_new(&self.canvas, &self.camera, size_v, 32.0),
        };
        let _ = size_v;
        let id = self
            .canvas
            .insert(app_id.clone(), title.clone(), pos, size_v, Place::Normal);

        // Мир TAD: окно = RO + VO на десктопе (для порталов, поиска, restore).
        let (ro_id, vo_id) = self.register_window(title.clone(), app_id.clone(), size);
        self.windows
            .add(title, app_id, size, ro_id, vo_id, self.current_workspace);
        self.entries.push(WindowEntry {
            window,
            id,
            saved: None,
            maximized: false,
            fullscreen: false,
        });
        id
    }

    pub fn remove_toplevel(&mut self, surface: &ToplevelSurface) {
        let target = surface.wl_surface().clone();
        if let Some(idx) = self.entries.iter().position(|e| {
            e.window
                .wl_surface()
                .map(|s| s.as_ref() == &target)
                .unwrap_or(false)
        }) {
            let entry = self.entries.remove(idx);
            self.canvas.remove(entry.id);
        }
    }

    pub fn entry_by_id(&self, id: WindowId) -> Option<&WindowEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn id_for_surface(&self, surface: &WlSurface) -> Option<WindowId> {
        self.entry_by_surface(surface).map(|e| e.id)
    }

    pub fn entry_by_surface(&self, surface: &WlSurface) -> Option<&WindowEntry> {
        self.entries.iter().find(|e| {
            e.window
                .wl_surface()
                .map(|s| s.as_ref() == surface)
                .unwrap_or(false)
        })
    }

    /// Поднять + отдать ввод окну.
    pub fn focus_window(&mut self, id: WindowId, serial: Serial) {
        self.canvas.raise(id);
        let surface = self
            .entry_by_id(id)
            .and_then(|e| e.window.wl_surface())
            .map(|s| s.into_owned());
        if let (Some(kb), Some(surface)) = (self.keyboard.clone(), surface) {
            kb.set_focus(self, Some(surface), serial);
        }
    }

    pub fn focused_window(&self) -> Option<WindowId> {
        self.canvas.focus()
    }

    fn find_desktop_ro(&self) -> RoId {
        self.ro_index
            .read()
            .values()
            .find(|r| r.kind == tad_core::ObjectKind::Folder)
            .map(|r| r.id)
            .unwrap_or_else(Uuid::nil)
    }

    /// Мир TAD: окно как документ (плейсхолдер для suspend/session restore).
    pub fn register_window(&self, title: String, app_id: String, size: (u32, u32)) -> (RoId, VoId) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ro_id = Uuid::new_v4();
        let vo_id = Uuid::new_v4();

        let mut doc = tad_core::TadDocument::new();
        doc.push(
            tad_core::Segment::Heading {
                level: 1,
                text: title.clone(),
            },
            20.0,
            20.0,
            size.0 as f32 - 40.0,
            40.0,
        );
        doc.push(
            tad_core::Segment::Text {
                text: format!("Wayland client: {app_id}"),
            },
            20.0,
            70.0,
            size.0 as f32 - 40.0,
            24.0,
        );

        let ro = tad_core::RealObject {
            id: ro_id,
            title,
            kind: tad_core::ObjectKind::WaylandWindow,
            document: doc,
            meta: {
                let mut m = std::collections::HashMap::new();
                m.insert("app_id".into(), app_id);
                m.insert("workspace".into(), self.current_workspace.to_string());
                m
            },
            doc_size: (size.0 as f32, size.1 as f32),
            created_at: now,
            updated_at: now,
        };
        let _ = self.store.lock().put_ro(&ro);
        self.ro_index.write().insert(ro_id, ro);

        let desktop_ro = self.find_desktop_ro();
        let vo = VirtualObject {
            id: vo_id,
            target_ro: ro_id,
            parent_ro: desktop_ro,
            pos: Point2::new(
                100.0 + self.canvas.len() as f32 * 40.0,
                100.0 + self.canvas.len() as f32 * 40.0,
            ),
            size: cgmath::Vector2::new(size.0 as f32, size.1 as f32),
            scale: 1.0,
            display_mode: tad_core::DisplayMode::Live,
            rotation: 0.0,
            z: 1,
        };
        let _ = self.store.lock().put_vo(&vo);
        self.all_vos.write().push(vo);
        (ro_id, vo_id)
    }

    pub fn lock(&mut self) {
        self.locked = true;
        tracing::info!("Session locked");
    }

    pub fn unlock(&mut self) {
        self.locked = false;
        tracing::info!("Session unlocked");
    }
}
