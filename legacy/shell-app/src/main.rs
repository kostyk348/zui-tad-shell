//! # ZUI-TAD Shell — интерактивный GUI
//!
//! Полноценный оконный интерфейс на winit + softbuffer.
//! CPU-рендеринг через skia-renderer → Pixmap → softbuffer Surface.
//!
//! Поддерживает: pan, zoom, click, drag VO, file drop, горячие клавиши,
//! запуск внешних приложений (terminal/browser/editor/files/calc).

mod demo_scene;
mod actions;
mod hotkeys;
mod wm_bridge;
mod external_tools;
mod config;
mod ui;
mod native;

use ahash::AHashMap;
use anyhow::Result;
use cgmath::{Point2, Vector2};
use de_common::launcher::Launcher;
use de_common::workspaces::WorkspaceManager;
use editor_core::{EditorRegistry, FocusManager, FocusTarget, PortalSystem, TextEditor, ToolCommand};
use parking_lot::Mutex;
use skia_renderer::SkiaRenderer;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tad_core::{GraphStore, RealObject, RoId, Segment, VirtualObject, VoId};
use tracing_subscriber::EnvFilter;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use hotkeys::{dispatch, HotkeyAction};
use wm_bridge::{AppKind, WmBridge};
use external_tools::{ExternalTools, ToolsConfig};

pub struct ShellState {
    pub camera: canvas_engine::Camera,
    pub store: Arc<Mutex<GraphStore>>,
    pub db: sled::Db,
    pub ro_index: HashMap<RoId, RealObject>,
    pub all_vos: Vec<VirtualObject>,
    pub focus: FocusManager,
    pub editors: EditorRegistry,
    pub portals: PortalSystem,
    pub wm: WmBridge,
    pub external: ExternalTools,

    // DE компоненты.
    pub workspaces: WorkspaceManager,
    pub launcher: Launcher,
    pub launcher_visible: bool,
    pub launcher_query: String,
    pub launcher_selected: usize,
    pub current_workspace: u8,

    // Окно и рендеринг.
    pub window: Option<Arc<Window>>,
    pub context: Option<softbuffer::Context<Arc<Window>>>,
    pub surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    pub renderer: Option<SkiaRenderer>,
    pub viewport: (u32, u32),
    pub last_frame: Instant,

    // Состояние мыши.
    pub cursor_screen: Vector2<f32>,
    pub panning: bool,
    pub drag_vo: Option<VoId>,
    pub drag_offset: Vector2<f32>,
    pub last_pan_pos: Vector2<f32>,

    // UI.
    pub show_help: bool,
    pub dirty: bool,
}

impl ShellState {
    pub fn new(store: Arc<Mutex<GraphStore>>, db: sled::Db) -> Self {
        let mut ro_index = HashMap::new();
        let mut all_vos = Vec::new();
        for ro in store.lock().all_ro().unwrap_or_default() {
            ro_index.insert(ro.id, ro);
        }
        let ro_ids: Vec<_> = ro_index.keys().copied().collect();
        for parent in ro_ids {
            all_vos.extend(store.lock().children_of(parent).unwrap_or_default());
        }

        Self {
            camera: canvas_engine::Camera::new(Vector2::new(1600, 1000)),
            store, db,
            ro_index, all_vos,
            focus: FocusManager::default(),
            editors: EditorRegistry::default(),
            portals: PortalSystem::default(),
            wm: WmBridge::new(),
            external: ExternalTools::new(ToolsConfig::default()),
            workspaces: WorkspaceManager::new(9),
            launcher: Launcher::load_all(),
            launcher_visible: false,
            launcher_query: String::new(),
            launcher_selected: 0,
            current_workspace: 0,
            window: None, context: None, surface: None, renderer: None,
            viewport: (1600, 1000),
            last_frame: Instant::now(),
            cursor_screen: Vector2::new(0.0, 0.0),
            panning: false, drag_vo: None, drag_offset: Vector2::new(0.0, 0.0),
            last_pan_pos: Vector2::new(0.0, 0.0),
            show_help: false, dirty: true,
        }
    }

    /// Перечитать RO из store (после правок редактором).
    pub fn reload_ro(&mut self, id: RoId) {
        if let Some(ro) = self.store.lock().get_ro(id).unwrap_or(None) {
            self.ro_index.insert(id, ro);
        }
    }

    /// Перезалить VO из store.
    pub fn reload_vos(&mut self) {
        self.all_vos = self.store.lock().all_vo().unwrap_or_default();
    }

    /// Создать новый RO (текст/mindmap/таблица) и разместить VO под курсором.
    pub fn create_new(&mut self, kind: tad_core::ObjectKind) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?.as_secs();
        let title = match kind {
            tad_core::ObjectKind::Text => "New Text",
            tad_core::ObjectKind::Mindmap => "New Mindmap",
            tad_core::ObjectKind::Table => "New Table",
            _ => "New",
        };
        let mut doc = tad_core::TadDocument::new();
        match kind {
            tad_core::ObjectKind::Text => {
                doc.push(Segment::Heading { level: 1, text: title.into() }, 20.0, 20.0, 760.0, 40.0);
                doc.push(Segment::Text { text: "Type here...".into() }, 20.0, 70.0, 760.0, 24.0);
            }
            tad_core::ObjectKind::Mindmap => {
                doc.push(Segment::Vector { shapes: vec![tad_core::VectorShape {
                    kind: tad_core::VectorKind::Rect { x: 300.0, y: 250.0, w: 200.0, h: 60.0 },
                    stroke: Some([0.95, 0.85, 0.3, 1.0]),
                    fill: Some([0.4, 0.3, 0.1, 0.8]),
                }]}, 0.0, 0.0, 800.0, 600.0);
            }
            tad_core::ObjectKind::Table => {
                doc.push(Segment::Heading { level: 1, text: title.into() }, 20.0, 20.0, 760.0, 40.0);
                doc.push(Segment::Table { rows: vec![
                    vec!["A".into(), "B".into(), "C".into()],
                    vec!["1".into(), "2".into(), "3".into()],
                ]}, 20.0, 70.0, 760.0, 80.0);
            }
            _ => {}
        }
        let ro = RealObject {
            id: uuid::Uuid::new_v4(),
            title: title.into(),
            kind,
            document: doc,
            meta: Default::default(),
            doc_size: (800.0, 600.0),
            created_at: now, updated_at: now,
        };
        self.store.lock().put_ro(&ro)?;

        // Находим Desktop и создаём VO под курсором.
        let desktop = self.ro_index.values()
            .find(|r| r.kind == tad_core::ObjectKind::Folder)
            .map(|r| r.id)
            .unwrap_or(ro.id);
        let pos = self.camera.screen_to_world(self.cursor_screen);
        let vo = VirtualObject::new(ro.id, desktop, pos,
            cgmath::Vector2::new(800.0, 600.0));
        self.store.lock().put_vo(&vo)?;

        self.reload_ro(ro.id);
        self.reload_vos();
        self.dirty = true;
        Ok(())
    }

    /// Удалить VO под курсором (или текущий фокусный).
    pub fn delete_vo_under_cursor(&mut self) -> Result<()> {
        let culled = canvas_engine::cull(&self.camera, &self.all_vos);
        let world = self.camera.screen_to_world(self.cursor_screen);
        if let Some(c) = canvas_engine::hit_test(&culled, world) {
            let vo_id = c.vo.id;
            if self.ro_index.get(&c.vo.target_ro).map(|r| r.kind) == Some(tad_core::ObjectKind::Folder) {
                tracing::warn!("Нельзя удалить Desktop");
                return Ok(());
            }
            self.store.lock().delete_vo(vo_id)?;
            self.all_vos.retain(|v| v.id != vo_id);
            self.dirty = true;
            self.focus.blur();
            tracing::info!("Удалён VO {}", vo_id);
        }
        Ok(())
    }

    /// Дублировать VO под курсором.
    pub fn duplicate_vo_under_cursor(&mut self) -> Result<()> {
        let culled = canvas_engine::cull(&self.camera, &self.all_vos);
        let world = self.camera.screen_to_world(self.cursor_screen);
        if let Some(c) = canvas_engine::hit_test(&culled, world) {
            let mut new_vo = c.vo.clone();
            new_vo.id = uuid::Uuid::new_v4();
            new_vo.pos = Point2::new(c.vo.pos.x + 40.0, c.vo.pos.y + 40.0);
            self.store.lock().put_vo(&new_vo)?;
            self.all_vos.push(new_vo);
            self.dirty = true;
        }
        Ok(())
    }

    /// Импорт перетаскиваемого файла → создать RO + VO.
    pub fn import_dropped_file(&mut self, path: &std::path::Path) -> Result<()> {
        let blobs = tad_core::BlobStore::open(&self.db).ok();
        let ro = tad_core::import::import_file(path, blobs.as_ref())?;
        self.store.lock().put_ro(&ro)?;
        let desktop = self.ro_index.values()
            .find(|r| r.kind == tad_core::ObjectKind::Folder)
            .map(|r| r.id)
            .unwrap_or(ro.id);
        let pos = self.camera.screen_to_world(self.cursor_screen);
        let vo = VirtualObject::new(ro.id, desktop, pos,
            cgmath::Vector2::new(ro.doc_size.0, ro.doc_size.1));
        self.store.lock().put_vo(&vo)?;
        self.reload_ro(ro.id);
        self.reload_vos();
        self.dirty = true;
        tracing::info!("Импортирован файл: {:?}", path);
        Ok(())
    }

    /// Запустить встроенное TAD-приложение.
    pub fn launch_native(&mut self, kind: native_apps::NativeAppKind) -> Result<()> {
        let pos = self.camera.screen_to_world(self.cursor_screen);
        let desktop = native::desktop_ro(&self.ro_index);
        let (ro, vo) = native::spawn_native(kind, pos, &self.store.lock(), desktop)?;
        self.ro_index.insert(ro.id, ro);
        self.all_vos.push(vo);
        self.reload_vos();
        self.dirty = true;
        Ok(())
    }
        let pos = self.camera.screen_to_world(self.cursor_screen);
        let blobs = tad_core::BlobStore::open(&self.db).ok();
        let store = self.store.lock();
        let (_ro, vo) = self.wm.launch(kind, pos, (600.0, 400.0), &store, blobs.as_ref())?;
        drop(store);
        self.reload_vos();
        self.dirty = true;
        Ok(())
    }

    /// Сохранить все изменённые RO.
    pub fn save_all(&self) -> Result<()> {
        let s = self.store.lock();
        for ro in self.ro_index.values() {
            s.put_ro(ro)?;
        }
        for vo in &self.all_vos {
            s.put_vo(vo)?;
        }
        tracing::info!("Сохранено {} RO, {} VO", self.ro_index.len(), self.all_vos.len());
        Ok(())
    }

    /// Экспорт активного RO в markdown.
    pub fn export_active(&self) -> Result<()> {
        let active_ro = match self.focus.current {
            FocusTarget::Vo(vo_id) | FocusTarget::Segment(vo_id, _) => {
                self.all_vos.iter().find(|v| v.id == vo_id)
                    .and_then(|vo| self.ro_index.get(&vo.target_ro))
            }
            FocusTarget::None => None,
        };
        let Some(ro) = active_ro else { return Ok(()); };
        let md = tad_core::import::export_markdown(ro)?;
        let _ = std::fs::create_dir_all("data");
        let path = format!("data/export_{}.md",
            ro.title.replace(' ', "_"));
        std::fs::write(&path, md)?;
        tracing::info!("Экспорт в {}", path);
        Ok(())
    }

    /// Активировать текстовый редактор на фокусном VO.
    pub fn ensure_text_editor(&mut self, ro_id: RoId) {
        if !self.editors.text.contains_key(&ro_id) {
            let doc = self.ro_index.get(&ro_id).map(|r| r.document.clone()).unwrap_or_default();
            self.editors.text.insert(ro_id, TextEditor::new(doc));
        }
    }

    /// Применить действие горячей клавиши.
    pub fn apply_hotkey(&mut self, action: HotkeyAction) -> bool {
        match action {
            HotkeyAction::Quit => { self.wm.kill_all(); return false; }
            HotkeyAction::ResetView => {
                let ws = self.workspaces.current();
                self.camera.center = ws.camera_center;
                self.camera.zoom = ws.camera_zoom;
            }
            HotkeyAction::ZoomIn => self.camera.zoom_at(self.cursor_screen, 1.2),
            HotkeyAction::ZoomOut => self.camera.zoom_at(self.cursor_screen, 1.0 / 1.2),
            HotkeyAction::ZoomReset => self.camera.zoom_at(self.cursor_screen, 1.0 / self.camera.zoom),
            HotkeyAction::Blur => {
                if self.launcher_visible {
                    self.launcher_visible = false;
                    self.launcher_query.clear();
                } else {
                    self.focus.blur();
                }
            }
            HotkeyAction::FocusNext => {
                if let FocusTarget::Vo(vo_id) = self.focus.current {
                    if let Some(vo) = self.all_vos.iter().find(|v| v.id == vo_id).cloned() {
                        if let Some(ro) = self.ro_index.get(&vo.target_ro) {
                            let segs: Vec<_> = ro.document.root_segments.iter().map(|s| s.id).collect();
                            self.focus.tab_next(&segs);
                        }
                    }
                }
            }
            HotkeyAction::FlyToFocused => {
                if let FocusTarget::Vo(vo_id) = self.focus.current {
                    if let Some(vo) = self.all_vos.iter().find(|v| v.id == vo_id).cloned() {
                        let _ = self.portals.fly_to(vo.target_ro, &self.all_vos, &self.camera);
                    }
                }
            }
            HotkeyAction::ToggleHelp => self.show_help = !self.show_help,
            HotkeyAction::Screenshot => {
                if let Some(r) = self.renderer.as_mut() {
                    let culled = canvas_engine::cull(&self.camera, &self.all_vos);
                    let frame = r.render(&self.camera, &culled, &self.ro_index);
                    let _ = std::fs::create_dir_all("data");
                    let _ = frame.pixmap.save_png("data/screenshot.png");
                    tracing::info!("Скриншот сохранён в data/screenshot.png");
                }
            }
            HotkeyAction::LaunchTerminal => { let _ = self.launch_app(AppKind::Terminal); }
            HotkeyAction::LaunchBrowser  => { let _ = self.launch_app(AppKind::Browser); }
            HotkeyAction::LaunchEditor   => { let _ = self.launch_app(AppKind::Editor); }
            HotkeyAction::LaunchFiles => { let _ = self.launch_native(native_apps::NativeAppKind::Files); }
            HotkeyAction::LaunchCalc => { let _ = self.launch_native(native_apps::NativeAppKind::Calculator); }
            HotkeyAction::LaunchSettings => { let _ = self.launch_native(native_apps::NativeAppKind::Settings); }
            HotkeyAction::NewTextDoc => { let _ = self.create_new(tad_core::ObjectKind::Text); }
            HotkeyAction::NewMindmap => { let _ = self.create_new(tad_core::ObjectKind::Mindmap); }
            HotkeyAction::NewTable   => { let _ = self.create_new(tad_core::ObjectKind::Table); }
            HotkeyAction::DeleteVo => { let _ = self.delete_vo_under_cursor(); }
            HotkeyAction::DuplicateVo => { let _ = self.duplicate_vo_under_cursor(); }
            HotkeyAction::Save => { let _ = self.save_all(); }
            HotkeyAction::ExportMarkdown => { let _ = self.export_active(); }
            HotkeyAction::Lock => {
                self.external.lock_screen();
            }
            // DE: workspace switch
            HotkeyAction::SwitchWorkspace(id) => {
                self.workspaces.save_camera(self.camera.center, self.camera.zoom);
                self.workspaces.switch_to(id);
                self.current_workspace = id;
                let ws = self.workspaces.current();
                self.camera.center = ws.camera_center;
                self.camera.zoom = ws.camera_zoom;
                tracing::info!("Workspace: {}", id + 1);
            }
            // DE: app launcher
            HotkeyAction::ToggleLauncher => {
                // Если включён внешний launcher (rofi/wofi/fuzzel/etc.) — зовём его.
                if !matches!(self.external.config.launcher, external_tools::LauncherBackend::BuiltIn) {
                    if let Some(name) = self.external.launch_app() {
                        // Нашли .desktop по имени → запускаем Exec.
                        if let Some(exec) = external_tools::find_exec_by_name(&name) {
                            let clean = exec.replace("%f", "").replace("%F", "")
                                .replace("%u", "").replace("%U", "")
                                .replace("%i", "").replace("%c", "");
                            let parts: Vec<&str> = clean.split_whitespace().collect();
                            if !parts.is_empty() {
                                let _ = std::process::Command::new(parts[0]).args(&parts[1..]).spawn();
                            }
                        } else {
                            // Возможно, name — это просто команда.
                            let _ = std::process::Command::new(&name).spawn();
                        }
                    }
                    return true;
                }
                self.launcher_visible = !self.launcher_visible;
                self.launcher_query.clear();
                self.launcher_selected = 0;
            }
            // DE: launcher enter (запуск выбранного)
            HotkeyAction::Enter if self.launcher_visible => {
                if let Some(&idx) = self.launcher.filtered.get(self.launcher_selected) {
                    let _ = self.launcher.launch(idx);
                    self.launcher_visible = false;
                    self.launcher_query.clear();
                }
            }
            HotkeyAction::LauncherUp => {
                if self.launcher_selected > 0 { self.launcher_selected -= 1; }
            }
            HotkeyAction::LauncherDown => {
                if self.launcher_selected + 1 < self.launcher.filtered.len() {
                    self.launcher_selected += 1;
                }
            }
            _ => return false,  // остальное обрабатывается в handle_edit_action
        }
        self.dirty = true;
        true
    }

    /// Редакторские команды (undo/redo/стрелки/backspace/enter).
    pub fn handle_edit_action(&mut self, action: HotkeyAction) {
        // Native apps (Files, Settings, Calculator).
        if let FocusTarget::Vo(vo_id) | FocusTarget::Segment(vo_id, _) = self.focus.current {
            if let Some(vo) = self.all_vos.iter().find(|v| v.id == vo_id).cloned() {
                if let Some(ro) = self.ro_index.get_mut(&vo.target_ro) {
                    if native::active_native_kind(ro).is_some() {
                        if native::handle_native_key(ro, action.clone()) {
                            let ro_clone = native::persist_native(ro);
                            self.ro_index.insert(ro_clone.id, ro_clone.clone());
                            let _ = self.store.lock().put_ro(&ro_clone);
                            self.dirty = true;
                            return;
                        }
                    }
                }
            }
        }

        let FocusTarget::Segment(vo_id, seg_id) = self.focus.current else { return };
        let Some(vo) = self.all_vos.iter().find(|v| v.id == vo_id).cloned() else { return };
        let ro_id = vo.target_ro;
        self.ensure_text_editor(ro_id);
        let Some(editor) = self.editors.text.get_mut(&ro_id) else { return };
        editor.active_segment = Some(seg_id);

        let cmd = match action {
            HotkeyAction::Undo => { editor.undo(); self.save_editor_doc(ro_id); return; }
            HotkeyAction::Redo => { editor.redo(); self.save_editor_doc(ro_id); return; }
            HotkeyAction::Backspace => ToolCommand::Backspace,
            HotkeyAction::Delete => ToolCommand::Delete,
            HotkeyAction::Enter => ToolCommand::Enter,
            HotkeyAction::ArrowLeft => ToolCommand::ArrowLeft,
            HotkeyAction::ArrowRight => ToolCommand::ArrowRight,
            HotkeyAction::ArrowUp => ToolCommand::ArrowUp,
            HotkeyAction::ArrowDown => ToolCommand::ArrowDown,
            HotkeyAction::Home => ToolCommand::Home,
            HotkeyAction::End => ToolCommand::End,
            HotkeyAction::FocusNext => ToolCommand::Tab,
            _ => return,
        };
        editor.handle_command(&cmd);
        self.save_editor_doc(ro_id);
        self.dirty = true;
    }

    fn save_editor_doc(&mut self, ro_id: RoId) {
        if let Some(editor) = self.editors.text.get(&ro_id) {
            if let Some(ro) = self.ro_index.get_mut(&ro_id) {
                ro.document = editor.doc.clone();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                ro.updated_at = now;
                let ro_clone = ro.clone();
                let _ = self.store.lock().put_ro(&ro_clone);
            }
        }
    }

    /// Ввод символа в активный текстовый сегмент или в launcher.
    pub fn handle_char_input(&mut self, ch: char) {
        // Launcher имеет приоритет.
        if self.launcher_visible {
            if ch.is_control() { return; }
            self.launcher_query.push(ch);
            self.launcher.search(&self.launcher_query);
            self.launcher_selected = 0;
            self.dirty = true;
            return;
        }
        let FocusTarget::Segment(vo_id, _seg_id) = self.focus.current else { return };
        let Some(vo) = self.all_vos.iter().find(|v| v.id == vo_id).cloned() else { return };
        if let Some(ro) = self.ro_index.get_mut(&vo.target_ro) {
            if native::handle_native_char(ro, ch) {
                let ro_clone = native::persist_native(ro);
                self.ro_index.insert(ro_clone.id, ro_clone.clone());
                let _ = self.store.lock().put_ro(&ro_clone);
                self.dirty = true;
                return;
            }
        }
        let ro_id = vo.target_ro;
        self.ensure_text_editor(ro_id);
        let Some(editor) = self.editors.text.get_mut(&ro_id) else { return };
        editor.handle_char(ch);
        self.save_editor_doc(ro_id);
        self.dirty = true;
    }

    /// Backspace для launcher.
    pub fn handle_backspace(&mut self) {
        if self.launcher_visible {
            self.launcher_query.pop();
            self.launcher.search(&self.launcher_query);
            self.launcher_selected = 0;
            self.dirty = true;
        }
    }

    /// Перерисовать кадр в surface.
    pub fn render_frame(&mut self) {
        // Шаг портальной анимации + авто-фокус (до borrow renderer).
        let was_portal_active = self.portals.is_active();
        if was_portal_active {
            if let Some(cam) = self.portals.step(0.016, self.camera.viewport) {
                self.camera.center = cam.center;
                self.camera.zoom = cam.zoom;
            }
            self.dirty = true;
        }
        if was_portal_active && !self.portals.is_active() {
            if let Some(target_ro) = self.portals.pending_target_ro.take() {
                if let Some(vo) = self.all_vos.iter().find(|v| v.target_ro == target_ro).cloned() {
                    if let Some(seg) = self.ro_index.get(&target_ro)
                        .and_then(|r| r.document.root_segments.first()) {
                        self.focus.force_focus(FocusTarget::Segment(vo.id, seg.id));
                        self.ensure_text_editor(target_ro);
                    } else {
                        self.focus.force_focus(FocusTarget::Vo(vo.id));
                    }
                }
            }
        }

        let culled = canvas_engine::cull(&self.camera, &self.all_vos);
        let world = self.camera.screen_to_world(self.cursor_screen);
        if !self.launcher_visible && !self.portals.is_active() {
            if let Some(new_focus) = self.focus.update_from_camera(&culled, world) {
                if let FocusTarget::Vo(vo_id) = new_focus {
                    if let Some(vo) = self.all_vos.iter().find(|v| v.id == vo_id) {
                        if let Some(seg) = self.ro_index.get(&vo.target_ro)
                            .and_then(|r| r.document.root_segments.first()) {
                            self.focus.force_focus(FocusTarget::Segment(vo_id, seg.id));
                            self.ensure_text_editor(vo.target_ro);
                        }
                    }
                }
            }
        }

        let Some(renderer) = self.renderer.as_mut() else { return };
        let Some(surface) = self.surface.as_mut() else { return };

        let culled = canvas_engine::cull(&self.camera, &self.all_vos);
        let mut frame = renderer.render(&self.camera, &culled, &self.ro_index);

        // Кэшируем immutable данные для отрисовки overlay.
        let fonts_cache = skia_renderer::fonts::FontCache {
            regular: renderer.fonts.regular.clone(),
            bold: renderer.fonts.bold.clone(),
            mono: renderer.fonts.mono.clone(),
        };
        let width = frame.pixmap.width();
        let height = frame.pixmap.height();
        let workspaces_count = self.workspaces.count;
        let current_workspace = self.current_workspace;
        let win_count = self.wm.processes.lock().len();
        let launcher_visible = self.launcher_visible;
        let launcher_query = self.launcher_query.clone();
        let launcher_selected = self.launcher_selected;
        let launcher_filtered: Vec<usize> = self.launcher.filtered.clone();
        let launcher_entries_count = self.launcher.entries.len();
        let show_help = self.show_help;
        // Snapshot of launcher entries (name + exec).
        let launcher_entries: Vec<(String, String)> = self.launcher.entries.iter()
            .map(|e| (e.name.clone(), e.exec.clone()))
            .collect();

        // Top panel.
        draw_panel(&mut frame.pixmap, &fonts_cache, width, height,
            workspaces_count, current_workspace, win_count);

        // Launcher.
        if launcher_visible {
            draw_launcher_overlay(&mut frame.pixmap, &fonts_cache, width, height,
                &launcher_query, &launcher_filtered, &launcher_entries, launcher_selected);
        }

        // Help.
        if show_help {
            draw_help_overlay(&mut frame.pixmap, &fonts_cache);
        }

        // Context toolbar (Этап 3: динамические микро-редакторы).
        if !launcher_visible && !show_help {
            ui::draw_context_toolbar(
                &mut frame.pixmap,
                &fonts_cache,
                width,
                height,
                self.focus.current,
                &self.ro_index,
                &self.all_vos,
            );
        }

        // Копируем Pixmap в softbuffer.
        blit_to_surface(surface, &frame.pixmap, self.viewport);
        self.dirty = false;
    }
}

impl ApplicationHandler for ShellState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(event_loop.create_window(
            winit::window::WindowAttributes::new()
                .with_title("ZUI-TAD Shell")
                .with_inner_size(winit::dpi::LogicalSize::new(1600, 1000))
        ).unwrap());
        self.window = Some(window.clone());

        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| tracing::error!("softbuffer context: {e}"))
            .ok();
        let mut surface = None;
        if let Some(ctx) = context.as_ref() {
            surface = softbuffer::Surface::new(ctx, window.clone())
                .map_err(|e| tracing::error!("softbuffer surface: {e}"))
                .ok();
        }
        if let Some(s) = surface.as_mut() {
            let _ = s.resize(std::num::NonZeroU32::new(1600).unwrap(),
                             std::num::NonZeroU32::new(1000).unwrap());
        }
        self.context = context;
        self.surface = surface;
        self.renderer = Some(SkiaRenderer::new(1600, 1000).unwrap());
        self.viewport = (1600, 1000);
        self.dirty = true;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.wm.kill_all();
                self.external.kill_all();
                let _ = self.save_all();
                event_loop.exit();
            }

            WindowEvent::Resized(PhysicalSize { width, height }) => {
                self.viewport = (width, height);
                self.camera.viewport = Vector2::new(width, height);
                if let Some(s) = self.surface.as_mut() {
                    if width > 0 && height > 0 {
                        let _ = s.resize(
                            std::num::NonZeroU32::new(width).unwrap(),
                            std::num::NonZeroU32::new(height).unwrap());
                    }
                }
                if let Some(r) = self.renderer.as_mut() {
                    r.width = width;
                    r.height = height;
                }
                self.dirty = true;
            }

            WindowEvent::CursorMoved { position, .. } => {
                let new_pos = Vector2::new(position.x as f32, position.y as f32);
                let delta = new_pos - self.cursor_screen;
                self.cursor_screen = new_pos;

                if self.panning {
                    self.camera.pan(delta);
                    self.dirty = true;
                } else if let Some(_vo) = self.drag_vo {
                    let world = self.camera.screen_to_world(new_pos);
                    let new_pos_world = Point2::new(world.x - self.drag_offset.x, world.y - self.drag_offset.y);
                    if let Some(vo) = self.all_vos.iter_mut().find(|v| v.id == self.drag_vo.unwrap()) {
                        vo.pos = new_pos_world;
                        let vo_clone = vo.clone();
                        drop(vo);
                        let _ = self.store.lock().put_vo(&vo_clone);
                    }
                    self.dirty = true;
                }
            }

            WindowEvent::MouseInput { button, state, .. } => {
                match (button, state) {
                    (MouseButton::Middle, ElementState::Pressed) => {
                        self.panning = true;
                        self.last_pan_pos = self.cursor_screen;
                    }
                    (MouseButton::Middle, ElementState::Released) => {
                        self.panning = false;
                    }
                    (MouseButton::Left, ElementState::Pressed) => self.on_left_click(),
                    (MouseButton::Left, ElementState::Released) => self.on_left_release(),
                    (MouseButton::Right, ElementState::Pressed) => self.on_right_click(),
                    _ => {}
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let factor = match delta {
                    MouseScrollDelta::LineDelta(_, y) => 1.1_f32.powf(y),
                    MouseScrollDelta::PixelDelta(p) => 1.0 + (p.y as f32 / 1000.0),
                };
                self.camera.zoom_at(self.cursor_screen, factor);
                self.dirty = true;
            }

            WindowEvent::Ime(ime) => {
                if let winit::event::Ime::Preedit(s, _) | winit::event::Ime::Commit(s) = &ime {
                    if !s.is_empty() {
                        for ch in s.chars() {
                            self.handle_char_input(ch);
                        }
                    }
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                // winit 0.30: модификаторы приходят в event, читаем через state.
                let mods = winit::keyboard::ModifiersState::all();
                if let Some(action) = dispatch(&event, mods) {
                    // Если launcher открыт и pressed Backspace — обрабатываем тут.
                    if event.state == ElementState::Pressed
                        && self.launcher_visible
                        && matches!(action, HotkeyAction::Backspace) {
                        self.handle_backspace();
                        self.dirty = true;
                        return;
                    }
                    let keep_running = self.apply_hotkey(action.clone());
                    if !keep_running {
                        event_loop.exit();
                        return;
                    }
                    self.handle_edit_action(action);
                    self.dirty = true;
                }
            }

            WindowEvent::DroppedFile(path) => {
                let _ = self.import_dropped_file(&path);
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // ~60 FPS.
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame);
        if dt.as_millis() >= 16 {
            self.last_frame = now;
            // Периодически чистим мёртвые процессы.
            self.wm.reap_dead();
            self.render_frame();
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }
}

impl ShellState {
    fn link_at_point(&self, vo: &VirtualObject, world: Point2<f32>) -> Option<RoId> {
        let ro = self.ro_index.get(&vo.target_ro)?;
        let local_x = world.x - vo.pos.x;
        let local_y = world.y - vo.pos.y;
        for seg in &ro.document.root_segments {
            if let Segment::Link { target_ro, .. } = &seg.segment {
                if local_x >= seg.x && local_x <= seg.x + seg.w
                    && local_y >= seg.y && local_y <= seg.y + seg.h {
                    return Some(*target_ro);
                }
            }
        }
        None
    }

    fn on_left_click(&mut self) {
        let culled = canvas_engine::cull(&self.camera, &self.all_vos);
        let world = self.camera.screen_to_world(self.cursor_screen);
        if let Some(c) = canvas_engine::hit_test(&culled, world) {
            let target_ro = c.vo.target_ro;
            let vo_id = c.vo.id;
            let ro_kind = self.ro_index.get(&target_ro).map(|r| r.kind);
            let parent_kind = self.ro_index.get(&c.vo.parent_ro).map(|r| r.kind);
            let first_seg_id = self.ro_index.get(&target_ro)
                .and_then(|r| r.document.root_segments.first().map(|s| s.id));

            if ro_kind == Some(tad_core::ObjectKind::WaylandWindow) {
                let _ = self.wm.focus_app(vo_id);
                return;
            }

            // Link-сегмент или встроенный портал-VO.
            if let Some(link_target) = self.link_at_point(&c.vo, world) {
                let _ = self.portals.fly_to(link_target, &self.all_vos, &self.camera);
                return;
            }
            if parent_kind != Some(tad_core::ObjectKind::Folder)
                && c.vo.target_ro != c.vo.parent_ro
                || ro_kind == Some(tad_core::ObjectKind::Portal)
            {
                let _ = self.portals.fly_to(target_ro, &self.all_vos, &self.camera);
                return;
            }

            if let Some(seg_id) = first_seg_id {
                self.focus.force_focus(FocusTarget::Segment(vo_id, seg_id));
            } else {
                self.focus.force_focus(FocusTarget::Vo(vo_id));
            }
            self.drag_vo = Some(vo_id);
            self.drag_offset = Vector2::new(world.x - c.vo.pos.x, world.y - c.vo.pos.y);
            self.dirty = true;
        } else {
            self.focus.blur();
            self.dirty = true;
        }
    }

    fn on_left_release(&mut self) {
        if let Some(dragged) = self.drag_vo.take() {
            let culled = canvas_engine::cull(&self.camera, &self.all_vos);
            let world = self.camera.screen_to_world(self.cursor_screen);
            if let Some((vo_id, new_parent, local)) =
                ui::try_reparent_vo(dragged, world, &culled, &self.ro_index)
            {
                let reparented = self.store.lock().reparent_vo(vo_id, new_parent, local).is_ok();
                if reparented {
                    self.reload_vos();
                    tracing::info!("VO {vo_id} → parent {new_parent}");
                }
                self.dirty = true;
            }
        }
    }

    fn on_right_click(&mut self) {
        // ПКМ — снять drag или blur.
        self.drag_vo = None;
        self.focus.blur();
        self.dirty = true;
    }
}

/// Копирование Pixmap → softbuffer Surface.
fn blit_to_surface(
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pm: &tiny_skia::Pixmap,
    viewport: (u32, u32),
) {
    let (w, h) = viewport;
    if w == 0 || h == 0 { return; }
    let nz_w = match std::num::NonZeroU32::new(w) { Some(v) => v, None => return };
    let nz_h = match std::num::NonZeroU32::new(h) { Some(v) => v, None => return };
    if let Err(e) = surface.resize(nz_w, nz_h) {
        tracing::error!("resize: {e}");
        return;
    }
    let mut buffer = match surface.buffer_mut() {
        Ok(b) => b,
        Err(e) => { tracing::error!("buffer: {e}"); return; }
    };
    let pm_data = pm.data();
    let pm_w = pm.width() as usize;
    let pm_h = pm.height() as usize;
    for y in 0..(h as usize) {
        for x in 0..(w as usize) {
            let dst = y * w as usize + x;
            if x < pm_w && y < pm_h {
                let src_idx = (y * pm_w + x) * 4;
                let r = pm_data[src_idx];
                let g = pm_data[src_idx + 1];
                let b = pm_data[src_idx + 2];
                let a = pm_data[src_idx + 3];
                let ar = (r as u32 * a as u32) / 255;
                let ag = (g as u32 * a as u32) / 255;
                let ab = (b as u32 * a as u32) / 255;
                buffer[dst] = 0xFF000000 | (ar << 16) | (ag << 8) | ab;
            } else {
                buffer[dst] = 0xFF000000;
            }
        }
    }
    if let Err(e) = buffer.present() {
        tracing::error!("present: {e}");
    }
}

fn text_width(text: &str, font: &ab_glyph::FontVec, size: f32) -> f32 {
    use ab_glyph::{Font, PxScale, ScaleFont};
    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    let mut w = 0.0_f32;
    let mut last: Option<ab_glyph::GlyphId> = None;
    for ch in text.chars() {
        let gid = scaled.scaled_glyph(ch).id;
        w += scaled.h_advance(gid);
        if let Some(prev) = last {
            w += scaled.kern(prev, gid);
        }
        last = Some(gid);
    }
    w
}

/// Верхняя панель DE.
fn draw_panel(
    pm: &mut tiny_skia::Pixmap,
    fonts: &skia_renderer::fonts::FontCache,
    width: u32,
    _height: u32,
    workspaces_count: u8,
    current_workspace: u8,
    win_count: usize,
) {
    use tiny_skia::{Paint, Rect, Transform};
    use ab_glyph::PxScale;
    let width = width as f32;
    const H: f32 = 28.0;

    let mut bg = Paint::default();
    bg.set_color_rgba8(0x10, 0x11, 0x15, 0xFF);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, width, H) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    let mut line = Paint::default();
    line.set_color_rgba8(0xff, 0xd7, 0x3a, 0xFF);
    if let Some(r) = Rect::from_xywh(0.0, H - 1.0, width, 1.0) {
        pm.fill_rect(r, &line, Transform::identity(), None);
    }

    let mut x = 12.0;
    let y = 19.0;

    skia_renderer::painter::Painter::draw_text(pm, &fonts.bold, "ZUI-TAD",
        x, y, PxScale::from(14.0), [0xff, 0xd7, 0x3a, 0xff]);
    x += 70.0;

    for i in 0..workspaces_count {
        let is_current = i == current_workspace;
        let label = (i + 1).to_string();
        let color = if is_current { [0xff, 0xd7, 0x3a, 0xff] } else { [0x80, 0x84, 0x90, 0xff] };
        if is_current {
            let mut p = Paint::default();
            p.set_color_rgba8(0xff, 0xd7, 0x3a, 0x30);
            if let Some(r) = Rect::from_xywh(x - 4.0, 4.0, 22.0, H - 8.0) {
                pm.fill_rect(r, &p, Transform::identity(), None);
            }
        }
        skia_renderer::painter::Painter::draw_text(pm, &fonts.regular, &label,
            x, y, PxScale::from(13.0), color);
        x += 24.0;
    }

    x += 16.0;
    if win_count > 0 {
        skia_renderer::painter::Painter::draw_text(pm, &fonts.regular,
            &format!("Apps: {}", win_count), x, y, PxScale::from(12.0),
            [0xaa, 0xae, 0xb8, 0xff]);
    }

    let now = chrono::Local::now();
    let clock = now.format("%a %d %b  %H:%M").to_string();
    let clock_w = text_width(&clock, &fonts.regular, 13.0);
    skia_renderer::painter::Painter::draw_text(pm, &fonts.regular, &clock,
        width - clock_w - 12.0, y, PxScale::from(13.0),
        [0xff, 0xff, 0xff, 0xff]);
}

/// App launcher overlay.
fn draw_launcher_overlay(
    pm: &mut tiny_skia::Pixmap,
    fonts: &skia_renderer::fonts::FontCache,
    width: u32,
    _height: u32,
    query: &str,
    filtered: &[usize],
    entries: &[(String, String)],
    selected: usize,
) {
    use tiny_skia::{Paint, Rect, Transform};
    use ab_glyph::PxScale;
    let width = width as f32;

    let mut overlay = Paint::default();
    overlay.set_color_rgba8(0x00, 0x00, 0x00, 0x80);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, width, pm.height() as f32) {
        pm.fill_rect(r, &overlay, Transform::identity(), None);
    }

    let lw = width * 0.6;
    let lh = 460.0;
    let lx = (width - lw) * 0.5;
    let ly = 80.0;
    let mut bg = Paint::default();
    bg.set_color_rgba8(0x1a, 0x1d, 0x25, 0xFF);
    if let Some(r) = Rect::from_xywh(lx, ly, lw, lh) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    let mut border = Paint::default();
    border.set_color_rgba8(0xff, 0xd7, 0x3a, 0xFF);
    if let Some(r) = Rect::from_xywh(lx, ly, lw, 2.0) {
        pm.fill_rect(r, &border, Transform::identity(), None);
    }

    skia_renderer::painter::Painter::draw_text(pm, &fonts.bold, ">>",
        lx + 16.0, ly + 36.0, PxScale::from(22.0), [0xff, 0xd7, 0x3a, 0xff]);
    skia_renderer::painter::Painter::draw_text(pm, &fonts.bold, query,
        lx + 44.0, ly + 36.0, PxScale::from(22.0), [0xff, 0xff, 0xff, 0xff]);

    let mut y = ly + 70.0;
    for (i, &entry_idx) in filtered.iter().enumerate().take(15) {
        if entry_idx >= entries.len() { continue; }
        let (name, exec) = &entries[entry_idx];
        let is_selected = i == selected;
        if is_selected {
            let mut p = Paint::default();
            p.set_color_rgba8(0xff, 0xd7, 0x3a, 0x30);
            if let Some(r) = Rect::from_xywh(lx + 8.0, y - 16.0, lw - 16.0, 24.0) {
                pm.fill_rect(r, &p, Transform::identity(), None);
            }
        }
        let color = if is_selected { [0xff, 0xd7, 0x3a, 0xff] } else { [0xd0, 0xd3, 0xda, 0xff] };
        skia_renderer::painter::Painter::draw_text(pm, &fonts.regular, name,
            lx + 24.0, y, PxScale::from(16.0), color);
        let exec_short: String = exec.chars().take(40).collect();
        let w = text_width(&exec_short, &fonts.regular, 12.0);
        skia_renderer::painter::Painter::draw_text(pm, &fonts.regular, &exec_short,
            lx + lw - w - 16.0, y, PxScale::from(12.0), [0x60, 0x64, 0x70, 0xff]);
        y += 26.0;
    }

    skia_renderer::painter::Painter::draw_text(pm, &fonts.regular,
        "Up/Down select  Enter launch  Esc close",
        lx + 16.0, ly + lh - 16.0, PxScale::from(12.0), [0x80, 0x84, 0x90, 0xff]);
}

/// Оверлей справки (F1).
fn draw_help_overlay(pm: &mut tiny_skia::Pixmap, fonts: &skia_renderer::fonts::FontCache) {
    use tiny_skia::{Paint, Rect, Transform};
    let mut p = Paint::default();
    p.set_color_rgba8(0x00, 0x00, 0x00, 0xE0);
    if let Some(r) = Rect::from_xywh(80.0, 80.0, 1440.0, 840.0) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
    let mut p2 = Paint::default();
    p2.set_color_rgba8(0xff, 0xd7, 0x3a, 0xff);
    if let Some(r) = Rect::from_xywh(80.0, 80.0, 1440.0, 2.0) {
        pm.fill_rect(r, &p2, Transform::identity(), None);
    }

    let lines = hotkeys::HELP_TEXT.lines();
    let mut y = 110.0_f32;
    for line in lines {
        let is_header = line.trim().is_empty() || line == line.to_uppercase();
        let font = if is_header { &fonts.bold } else { &fonts.regular };
        let color = if is_header { [0xff, 0xd7, 0x3a, 0xff] } else { [0xff, 0xff, 0xff, 0xff] };
        skia_renderer::painter::Painter::draw_text(
            pm, font, line, 100.0, y,
            ab_glyph::PxScale::from(if is_header { 16.0 } else { 13.0 }),
            color,
        );
        y += if is_header { 22.0 } else { 16.0 };
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("zui=info".parse()?))
        .init();

    // Парсим аргументы: --embedded = winit backend (для теста внутри другой DE)
    let args: Vec<String> = std::env::args().collect();
    let embedded = args.iter().any(|a| a == "--embedded");

    // Если передан --compositor или запуск из TTY (без DISPLAY/WAYLAND_DISPLAY),
    // запускаем как настоящую DE (smithay DRM/winit backend).
    if args.iter().any(|a| a == "--compositor") || std::env::var("ZUI_AS_DE").is_ok() {
        tracing::info!("Запуск как настоящая DE (smithay compositor)");
        let store_path = std::env::var("ZUI_STORE_PATH")
            .unwrap_or_else(|_| "data/store.sled".to_string());
        if let Some(parent) = std::path::Path::new(&store_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let store = Arc::new(Mutex::new(GraphStore::open(&store_path)?));
        // Демо-сцена при пустом хранилище.
        {
            let s = store.lock();
            if s.all_ro().unwrap_or_default().is_empty() {
                let demo = demo_scene::build_demo_scene();
                for ro in &demo.ros { s.put_ro(ro)?; }
                for vo in &demo.vos { s.put_vo(vo)?; }
            }
        }
        return compositor::run_compositor(store, embedded);
    }

    // Иначе — встроенный GUI режим (softbuffer в окно).
    // 1. Открываем хранилище.
    let store_path = std::env::var("ZUI_STORE_PATH")
        .unwrap_or_else(|_| "data/store.sled".to_string());
    if let Some(parent) = std::path::Path::new(&store_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let db = sled::open(&store_path)?;
    let store = Arc::new(Mutex::new(GraphStore::open_with_db(db.clone())?));

    // 2. Демо-сцена при пустом хранилище.
    {
        let s = store.lock();
        if s.all_ro().unwrap_or_default().is_empty() {
            tracing::info!("Инициализация демо-сцены...");
            let demo = demo_scene::build_demo_scene();
            for ro in &demo.ros { s.put_ro(ro)?; }
            for vo in &demo.vos { s.put_vo(vo)?; }
            tracing::info!("Демо-сцена: {} RO, {} VO", demo.ros.len(), demo.vos.len());
        }
    }

    let mut state = ShellState::new(store, db);
    state.camera.center = Point2::new(800.0, 500.0);
    state.camera.zoom = 0.35;

    // Загружаем конфиг и инициализируем внешние инструменты.
    match config::Config::load() {
        Ok(cfg) => {
            tracing::info!("Конфиг: launcher={}, panel={}, locker={}",
                cfg.launcher.backend, cfg.panel.backend, cfg.locker.backend);
            state.external = ExternalTools::new(cfg.to_tools_config());
        }
        Err(e) => tracing::warn!("Конфиг не загружен: {e}, используем default"),
    }
    // Запускаем внешние инструменты (waybar/swaybg/mako/etc.), если они включены.
    state.external.start_all();

    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut state)?;
    Ok(())
}
