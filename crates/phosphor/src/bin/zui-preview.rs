//! zui-preview — ЖИВОЕ окно холста (winit + softbuffer, чистый CPU, без GL).
//!
//! Зачем: композитор требует GL/seat'а, а оболочку надо щупать сейчас.
//! Здесь тот же canvas-движок и та же оболочка (phosphor), но кадр идёт в
//! обычное окно через softbuffer.
//!
//! Оптимизация 2-го раунда (чтобы держать 60 fps *дёшево*):
//!   * карточки окон и панель КЭШИРУЮТСЯ как растр — текст не растеризуется
//!     каждый кадр (это была главная статья расходов);
//!   * блит в softbuffer — без ветвления по альфе, полосами в потоках;
//!   * CRT-конвейер fused в 2 прохода (см. fx.rs);
//!   * телеметрия кадра + АДАПТИВНОЕ качество: если кадр не влезает в бюджет,
//!     эффекты ужимаются автоматически (полезно на батарее).
//!
//! Запуск: `cargo run --release -p phosphor --bin zui-preview`
//! Клавиши: `:`/`/` меню · `?`/F1 справка · F2 HUD · F3 focus · F4 виталы ·
//! F5 развёртка · F6 качество · 1/2/3 тема · L лаунчер · Space home ·
//! W overview · M fit · S suspend · Tab MRU · ←↑→↓ прыжок · Esc выход.
//! Мышь: ЛКМ по окну — фокус+перенос, за край — ресайз, по пустому — панорама,
//! колесо — панорама, Mod+колесо — зум, клик по меню — выполнить.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use canvas_engine::{
    canvas_to_screen, layout_windows, Camera, Dir, Interact, Mode, Place, ResizeHandle, Scene,
    WindowId,
};
use cgmath::{Point2, Vector2};
use phosphor::bg::{self, BgKind};
use phosphor::config::ShellConfig;
use phosphor::demo::{self, WinState};
use phosphor::fx::{crt, CrtParams};
use phosphor::hud::{self, Ecg, Toast};
use phosphor::menu::{self, Action, MenuState, ThemeId};
use phosphor::panel::{draw_top_panel, hint_bar, PanelData};
use phosphor::theme::{Metrics, Mode as ThemeMode, Palette};
use phosphor::widgets::Fonts;
use tiny_skia::Pixmap;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId as WinitWindowId};

const SNAP_PX: f32 = 26.0;
const RESIZE_MARGIN_PX: f32 = 9.0;
const MIN_WINDOW: Vector2<f32> = Vector2::new(120.0, 80.0);
/// Бюджет кадра при 60 fps.
const FRAME_BUDGET_MS: f32 = 16.6;
/// Бюджет кэша растров карточек окон (держим мегабайты, а не «64 штуки»).
const CARD_CACHE_BYTES: usize = 12 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quality {
    Auto,
    Rich,
    Lean,
}

struct Mock {
    title: &'static str,
    app: &'static str,
    state: WinState,
}

struct App {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    fonts: Option<Fonts>,
    metrics: Metrics,

    scene: Scene,
    camera: Camera,
    interact: Interact,
    mocks: HashMap<WindowId, Mock>,

    mode: ThemeMode,
    ecg: Ecg,
    toast: Option<Toast>,
    started: Instant,
    mods: ModifiersState,
    pointer: Point2<f64>,
    launcher: bool,
    menu: Option<MenuState>,
    help: bool,
    osd: Option<(String, f32, String, Instant)>,

    // настройки/удобство
    cfg: ShellConfig,
    hud_on: bool,
    vitals_on: bool,
    beam_on: bool,
    focus_mode: bool,
    quality: Quality,

    // производительность
    frame_ms: Vec<f32>,
    render_mode: &'static str,
    card_cache: HashMap<(u64, u32, u32, u8), (Pixmap, u64)>,
    card_cache_bytes: usize,
    /// Кадровый буфер переиспользуется: не аллоцируем 5.8 МБ каждый кадр.
    frame_buf: Option<Pixmap>,
    frame_counter: u64,
    /// Кэш «хрома»: верхняя панель и нижняя строка подсказок.
    /// Они меняются раз в секунду/по событию, а растеризация текста —
    /// самая дорогая часть кадра (замер: compose ~10 мс из 14).
    chrome: Option<(Pixmap, Pixmap, u64)>,
    /// Подфазы compose для телеметрии (мс).
    sub: [f32; 5],
    bg_kind: Option<BgKind>,
    bg_wallpaper: Option<String>,
    backdrop: Option<(Pixmap, (u32, u32))>,

    size: (u32, u32),
    last_frame: Instant,
    frames: u64,
    last_fps: f32,
    fps_t: Instant,
}

impl App {
    fn new() -> Self {
        let cfg = ShellConfig::load();
        let mode = match cfg.theme_or_default() {
            "signalis" => ThemeMode::Signalis,
            "phosphor" => ThemeMode::Phosphor,
            _ => ThemeMode::Rig,
        };
        let quality = match cfg.quality_or_default() {
            "rich" => Quality::Rich,
            "lean" => Quality::Lean,
            _ => Quality::Auto,
        };

        let mut scene = Scene::new();
        let mut mocks = HashMap::new();
        let a = scene.insert(
            "alacritty",
            "terminal — build",
            Point2::new(120.0, 80.0),
            Vector2::new(620.0, 430.0),
            Place::Normal,
        );
        mocks.insert(
            a,
            Mock {
                title: "terminal — build",
                app: "alacritty",
                state: WinState::Focused,
            },
        );
        let b = scene.insert(
            "zui-core",
            "memory.rs",
            Point2::new(742.0, 80.0),
            Vector2::new(520.0, 430.0),
            Place::Normal,
        );
        mocks.insert(
            b,
            Mock {
                title: "memory.rs",
                app: "zui-core",
                state: WinState::Live,
            },
        );
        scene.attach(a, b);
        let c = scene.insert(
            "firefox",
            "browser — signalis wiki",
            Point2::new(640.0, 600.0),
            Vector2::new(440.0, 200.0),
            Place::Normal,
        );
        mocks.insert(
            c,
            Mock {
                title: "browser — signalis wiki",
                app: "firefox",
                state: WinState::Suspended,
            },
        );
        scene.suspend(c);
        let p = scene.insert(
            "mpv",
            "picture-in-picture",
            Point2::new(1210.0, 60.0),
            Vector2::new(360.0, 220.0),
            Place::PinnedToScreen,
        );
        mocks.insert(
            p,
            Mock {
                title: "pip",
                app: "mpv",
                state: WinState::Live,
            },
        );

        let mut camera = Camera::new(Vector2::new(1600, 900));
        camera.center = Point2::new(700.0, 380.0);

        // Фон считаем до перемещения `cfg` в поля (иначе borrow of moved value).
        let bg_value = cfg.bg_or_default();
        let bg_kind = match bg_value.as_str() {
            "off" => None,
            "starfield" => Some(BgKind::Starfield),
            "crt" => Some(BgKind::Crt),
            "blueprint" => Some(BgKind::Blueprint),
            _ => Some(BgKind::Hull),
        };
        let bg_wallpaper = std::env::var("ZUI_WALLPAPER").ok().or_else(|| {
            if ["hull", "starfield", "crt", "blueprint", "off"].contains(&bg_value.as_str()) {
                None
            } else {
                Some(bg_value.clone())
            }
        });

        Self {
            window: None,
            surface: None,
            fonts: Fonts::load_default().ok(),
            metrics: Metrics::default(),
            scene,
            camera,
            interact: Interact::new(),
            mocks,
            mode,
            ecg: Ecg::new(160),
            toast: Some(Toast::new("shell", "canvas online · CPU 60fps")),
            started: Instant::now(),
            mods: ModifiersState::empty(),
            pointer: Point2::new(0.0, 0.0),
            launcher: false,
            menu: None,
            help: cfg.help_on_start,
            osd: None,
            hud_on: cfg.hud,
            vitals_on: cfg.vitals,
            beam_on: cfg.beam,
            focus_mode: cfg.focus_mode,
            quality,
            cfg,
            frame_ms: Vec::with_capacity(120),
            render_mode: "RICH",
            card_cache: HashMap::new(),
            card_cache_bytes: 0,
            frame_buf: None,
            frame_counter: 0,
            chrome: None,
            sub: [0.0; 5],
            bg_kind,
            bg_wallpaper,
            backdrop: None,
            size: (1600, 900),
            last_frame: Instant::now(),
            frames: 0,
            last_fps: 0.0,
            fps_t: Instant::now(),
        }
    }

    fn save_cfg(&mut self) {
        self.cfg.theme = match self.mode {
            ThemeMode::Rig => "rig",
            ThemeMode::Signalis => "signalis",
            ThemeMode::Phosphor => "phosphor",
        }
        .into();
        self.cfg.quality = match self.quality {
            Quality::Auto => "auto",
            Quality::Rich => "rich",
            Quality::Lean => "lean",
        }
        .into();
        self.cfg.hud = self.hud_on;
        self.cfg.vitals = self.vitals_on;
        self.cfg.beam = self.beam_on;
        self.cfg.focus_mode = self.focus_mode;
        let _ = self.cfg.save();
    }

    fn palette(&self) -> Palette {
        Palette::of(self.mode)
    }

    /// Подложка под холст: своя процедурная или картинка пользователя.
    /// Кэшируется по размеру вьюпорта; каждый кадр — один блит с параллаксом.
    fn ensure_backdrop(&mut self) {
        let (w, h) = self.size;
        if matches!(self.backdrop, Some((_, key)) if key == (w, h)) {
            return;
        }
        let pal = self.palette();
        let mut layer = Pixmap::new(w, h).unwrap();

        // 1. Источник: файл пользователя либо генератор.
        let src: Option<Pixmap> = if let Some(path) = &self.bg_wallpaper {
            std::fs::read(path)
                .ok()
                .and_then(|bytes| image::load_from_memory(&bytes).ok())
                .map(|img| {
                    let rgba = img.to_rgba8();
                    let (iw, ih) = (rgba.width(), rgba.height());
                    let mut pm = Pixmap::new(iw, ih).unwrap();
                    for (i, px) in rgba.pixels().enumerate() {
                        let dst = &mut pm.pixels_mut()[i];
                        if let Some(c) =
                            tiny_skia::PremultipliedColorU8::from_rgba(px[0], px[1], px[2], px[3])
                        {
                            *dst = c;
                        }
                    }
                    pm
                })
        } else {
            None
        };
        let src = src.or_else(|| {
            self.bg_kind
                .map(|k| bg::render(k, 960, 540, &pal, 20260929))
        });

        let Some(src) = src else {
            self.backdrop = Some((layer, (w, h)));
            return;
        };

        // 2. Cover-fit масштаб + затемнение, чтобы окна читались.
        let sx = w as f32 / src.width() as f32;
        let sy = h as f32 / src.height() as f32;
        let scale = sx.max(sy);
        let ts = tiny_skia::Transform::from_scale(scale, scale);
        layer.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            ts,
            None,
        );
        let mut dim = tiny_skia::Paint::default();
        dim.set_color_rgba8(pal.bg[0], pal.bg[1], pal.bg[2], 0x88);
        if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32) {
            layer.fill_rect(r, &dim, tiny_skia::Transform::identity(), None);
        }
        self.backdrop = Some((layer, (w, h)));
    }

    /// Отрисовать подложку с лёгким параллаксом (двигается медленнее холста).
    fn draw_backdrop(&mut self, pm: &mut Pixmap) {
        self.ensure_backdrop();
        let (w, h) = self.size;
        let Some((layer, _)) = &self.backdrop else {
            return;
        };
        let ox = (-self.camera.center.x * 0.12).clamp(-(w as f32) * 0.5, 0.0);
        let oy = (-self.camera.center.y * 0.12).clamp(-(h as f32) * 0.5, 0.0);
        // Подложка непрозрачна → обычное копирование строк вместо SourceOver.
        phosphor::blit::blit(pm, layer, ox.round() as i32, oy.round() as i32);
    }

    fn avg_frame_ms(&self) -> f32 {
        if self.frame_ms.is_empty() {
            0.0
        } else {
            self.frame_ms.iter().sum::<f32>() / self.frame_ms.len() as f32
        }
    }

    /// Эффективная «богатость» кадра: учёт ручного режима и авто-деградации.
    fn rich(&self) -> bool {
        match self.quality {
            Quality::Rich => true,
            Quality::Lean => false,
            Quality::Auto => self.avg_frame_ms() < FRAME_BUDGET_MS * 0.9,
        }
    }

    fn set_osd(&mut self, label: &str, value: f32, text: String) {
        self.osd = Some((label.to_string(), value, text, Instant::now()));
    }

    fn open_menu(&mut self) {
        self.menu = Some(MenuState::new());
    }

    // ---------------- ввод ----------------

    fn canvas_pt(&self) -> Point2<f32> {
        canvas_engine::screen_to_canvas(
            &self.camera,
            Point2::new(self.pointer.x as f32, self.pointer.y as f32),
        )
    }

    fn resize_zone(&self, id: WindowId, p: Point2<f32>) -> Option<ResizeHandle> {
        let w = self.scene.get(id)?;
        let margin = (RESIZE_MARGIN_PX / self.camera.zoom.max(0.05))
            .min(w.size.x / 3.0)
            .min(w.size.y / 3.0);
        ResizeHandle::from_point(w.size, Vector2::new(p.x - w.pos.x, p.y - w.pos.y), margin)
    }

    fn on_cursor(&mut self) {
        let screen = Point2::new(self.pointer.x as f32, self.pointer.y as f32);
        let canvas = self.canvas_pt();
        let (dw, ds) = self.interact.cursor(canvas, screen);
        match self.interact.mode {
            Mode::Move { id, .. } => {
                self.scene.nudge(id, dw);
                let thr = SNAP_PX / self.camera.zoom.max(0.05);
                self.scene.snap(id, thr);
            }
            Mode::Resize { id, .. } => {
                if let Some((pos, size)) = self.interact.resize_plan(MIN_WINDOW) {
                    self.scene.move_to(id, pos);
                    self.scene.resize(id, size);
                }
            }
            Mode::Pan => {
                self.camera.pan(Vector2::new(ds.x, ds.y));
            }
            _ => {}
        }
    }

    fn on_primary(&mut self, pressed: bool) {
        // меню перехватывает мышь
        if let Some(st) = self.menu.clone() {
            if pressed {
                let m = self.metrics;
                let geom = menu::geometry(
                    self.size.0 as f32,
                    self.size.1 as f32,
                    st.filtered().len().max(1),
                    &m,
                );
                let my = self.pointer.y as f32;
                if let Some(idx) = menu::hit(&geom, my, st.scroll, st.filtered().len()) {
                    // idx — позиция в отфильтрованном списке
                    let mut st2 = st.clone();
                    st2.sel = idx;
                    if let Some(real) = st2.selected() {
                        let action = menu::entries()[real].action;
                        self.menu = None;
                        self.run_action(action);
                        return;
                    }
                } else if my < geom.rect.1 || my > geom.rect.1 + geom.rect.3 {
                    self.menu = None; // клик мимо — закрыть
                    return;
                }
            }
            return;
        }
        let canvas = self.canvas_pt();
        let screen = Point2::new(self.pointer.x as f32, self.pointer.y as f32);
        if pressed {
            if self.mods.super_key() {
                self.interact.begin_pan(canvas, screen);
                return;
            }
            match self.scene.hit_test(canvas) {
                Some(id) => {
                    let zone = self.resize_zone(id, canvas);
                    self.scene.raise(id);
                    match zone {
                        Some(h) => {
                            if let Some(w) = self.scene.get(id) {
                                let (pos, size) = (w.pos, w.size);
                                self.interact.begin_resize(id, h, pos, size, canvas, screen);
                            }
                        }
                        None => self.interact.begin_move(id, canvas, screen),
                    }
                }
                None => self.interact.begin_pan(canvas, screen),
            }
        } else {
            if let Some(id) = self.interact.grab_id() {
                let thr = SNAP_PX / self.camera.zoom.max(0.05);
                self.scene.snap(id, thr);
            }
            self.interact.end();
        }
    }

    fn run_action(&mut self, action: Action) {
        match action {
            Action::SetTheme(t) => {
                self.mode = t.mode();
                self.card_cache.clear();
                self.card_cache_bytes = 0;
                self.backdrop = None;
                self.toast = Some(Toast::new("theme", t.label()));
                self.set_osd("theme", 0.5, t.label().into());
            }
            Action::ToggleHud => self.hud_on = !self.hud_on,
            Action::ToggleFocus => self.focus_mode = !self.focus_mode,
            Action::ToggleVitals => self.vitals_on = !self.vitals_on,
            Action::ToggleBeam => self.beam_on = !self.beam_on,
            Action::QualityAuto => {
                self.quality = match self.quality {
                    Quality::Auto => Quality::Rich,
                    Quality::Rich => Quality::Lean,
                    Quality::Lean => Quality::Auto,
                }
            }
            Action::QualityRich => {
                self.quality = Quality::Rich;
                self.card_cache.clear();
                self.card_cache_bytes = 0;
            }
            Action::QualityLean => self.quality = Quality::Lean,
            Action::Help => self.help = true,
            Action::Launcher => self.launcher = true,
            Action::Home => {
                self.camera.center = self.scene.origin;
                self.camera.zoom = 1.0;
            }
            Action::Overview => {
                let vp = self.camera.viewport;
                if let Some(plan) = self.scene.zoom_to_fit(vp, 64.0) {
                    self.camera.center = plan.camera_center;
                    self.camera.zoom = plan.zoom;
                }
            }
            Action::FitWindow => {
                if let Some(id) = self.scene.focus() {
                    let vp = self.camera.viewport;
                    if let Some(plan) = self.scene.fit_window_plan(id, vp, 24.0) {
                        self.camera.center = plan.camera_center;
                        self.camera.zoom = plan.zoom;
                        self.scene.move_to(id, plan.window_pos);
                        self.scene.resize(id, plan.window_size);
                    }
                }
            }
            Action::SuspendFocused => {
                if let Some(id) = self.scene.focus() {
                    self.scene.suspend(id);
                    self.toast = Some(Toast::new("suspend", "окно оставлено плейсхолдером"));
                }
            }
            Action::CycleWindows => {
                if let Some(next) = self.scene.mru_cycle(self.scene.focus(), true) {
                    self.scene.raise(next);
                }
            }
            Action::Quit => {
                self.save_cfg();
                std::process::exit(0);
            }
        }
        self.save_cfg();
    }

    fn on_key(&mut self, event_loop: &ActiveEventLoop, key: Key) {
        // --- режим меню ---
        if let Some(st) = self.menu.as_mut() {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.menu = None;
                    return;
                }
                Key::Named(NamedKey::ArrowDown) => st.move_sel(1),
                Key::Named(NamedKey::ArrowUp) => st.move_sel(-1),
                Key::Named(NamedKey::Backspace) => st.backspace(),
                Key::Named(NamedKey::Enter) => {
                    if let Some(real) = st.selected() {
                        let action = menu::entries()[real].action;
                        self.menu = None;
                        self.run_action(action);
                    }
                    return;
                }
                Key::Character(c) => {
                    for ch in c.chars() {
                        st.type_char(ch);
                    }
                }
                _ => {}
            }
            if let Some(st) = self.menu.as_mut() {
                let n = st.filtered().len();
                let (start, _) = menu::visible_range(st.sel, n, menu::VISIBLE_ROWS);
                st.scroll = start;
            }
            return;
        }

        if self.help {
            match key {
                Key::Named(NamedKey::Escape) | Key::Character(_) => {
                    self.help = false;
                    return;
                }
                _ => return,
            }
        }

        match key {
            Key::Named(NamedKey::Escape) => {
                if self.launcher {
                    self.launcher = false;
                } else {
                    self.save_cfg();
                    event_loop.exit();
                }
            }
            Key::Named(NamedKey::F1) => self.help = true,
            Key::Named(NamedKey::F2) => {
                self.hud_on = !self.hud_on;
                self.toast = Some(Toast::new("hud", if self.hud_on { "on" } else { "off" }));
            }
            Key::Named(NamedKey::F3) => {
                self.focus_mode = !self.focus_mode;
                self.toast = Some(Toast::new(
                    "focus",
                    if self.focus_mode {
                        "тихий режим"
                    } else {
                        "обычный"
                    },
                ));
            }
            Key::Named(NamedKey::F4) => self.vitals_on = !self.vitals_on,
            Key::Named(NamedKey::F5) => self.beam_on = !self.beam_on,
            Key::Named(NamedKey::F6) => self.run_action(Action::QualityAuto),
            Key::Named(NamedKey::Space) => self.run_action(Action::Home),
            Key::Named(NamedKey::ArrowLeft) => self.jump(Dir::Left),
            Key::Named(NamedKey::ArrowRight) => self.jump(Dir::Right),
            Key::Named(NamedKey::ArrowUp) => self.jump(Dir::Up),
            Key::Named(NamedKey::ArrowDown) => self.jump(Dir::Down),
            Key::Character(c) => match c.as_str() {
                "1" => self.run_action(Action::SetTheme(ThemeId::Rig)),
                "2" => self.run_action(Action::SetTheme(ThemeId::Signalis)),
                "3" => self.run_action(Action::SetTheme(ThemeId::Phosphor)),
                "?" => self.help = true,
                ":" | "/" | "p" => self.open_menu(),
                "l" => self.launcher = !self.launcher,
                "w" => self.run_action(Action::Overview),
                "m" => self.run_action(Action::FitWindow),
                "s" => self.run_action(Action::SuspendFocused),
                "=" | "+" => self.zoom(1.25),
                "-" => self.zoom(0.8),
                "0" => self.camera.zoom = 1.0,
                _ => {}
            },
            _ => {}
        }
        self.save_cfg();
    }

    fn zoom(&mut self, factor: f32) {
        let c = Vector2::new(
            self.camera.viewport.x as f32 * 0.5,
            self.camera.viewport.y as f32 * 0.5,
        );
        self.camera.zoom_at(c, factor);
        let z = self.camera.zoom;
        self.set_osd("zoom", (z / 8.0).clamp(0.0, 1.0), format!("{z:.2}x"));
    }

    fn jump(&mut self, dir: Dir) {
        let from = self.camera.center;
        if let Some(id) = self.scene.nearest_in_direction(from, dir) {
            self.scene.raise(id);
            if let Some(w) = self.scene.get(id) {
                self.camera.center = w.center();
            }
        }
    }

    // ---------------- кадр ----------------

    /// Нарисовать кадр в ПЕРЕДАННЫЙ буфер (CRT — отдельная фаза, см. RedrawRequested).
    fn compose_into(&mut self, pm: &mut Pixmap) {
        let (w, h) = self.size;
        self.frame_counter = self.frame_counter.wrapping_add(1);
        let pal = self.palette();
        let m = self.metrics;
        let Some(fonts) = self.fonts.take() else {
            demo::fill(pm, pal.bg);
            return;
        };

        let ts = Instant::now();
        demo::fill(pm, pal.bg);
        self.draw_backdrop(pm);
        self.sub[0] = ts.elapsed().as_secs_f32() * 1000.0;
        let ts = Instant::now();
        demo::canvas_grid_camera(pm, &pal, &self.camera, 64.0);
        self.sub[1] = ts.elapsed().as_secs_f32() * 1000.0;

        let quads = layout_windows(&self.scene, &self.camera, self.camera.viewport);
        let focus = self.scene.focus();
        let mut cluster_bbox: Option<(f32, f32, f32, f32)> = None;

        // Карточки окон: растр кэшируется по (id, размер, состояние).
        for q in &quads {
            if q.pinned {
                continue;
            }
            let Some(mock) = self.mocks.get(&q.id) else {
                continue;
            };
            let state = if q.suspended {
                WinState::Suspended
            } else if Some(q.id) == focus {
                WinState::Focused
            } else {
                mock.state
            };
            let cw = ((q.quad.w.max(8.0) / 8.0).round() * 8.0) as u32;
            let ch = ((q.quad.h.max(8.0) / 8.0).round() * 8.0) as u32;
            let key = (q.id, cw, ch, state as u8);
            let now = self.frame_counter;
            if !self.card_cache.contains_key(&key) {
                let mut card = Pixmap::new(cw.max(8), ch.max(8)).unwrap();
                demo::window_card(
                    &mut card,
                    &fonts,
                    &pal,
                    &m,
                    (0.0, 0.0, cw as f32, ch as f32),
                    mock.title,
                    mock.app,
                    state,
                );
                let sz = cw as usize * ch as usize * 4;
                // Бюджет по байтам: вытесняем самые давно не использованные.
                while self.card_cache_bytes + sz > CARD_CACHE_BYTES && !self.card_cache.is_empty() {
                    let victim = self
                        .card_cache
                        .iter()
                        .min_by_key(|(_, (_, used))| *used)
                        .map(|(k, _)| *k);
                    match victim {
                        Some(k) => {
                            if let Some((old, _)) = self.card_cache.remove(&k) {
                                self.card_cache_bytes = self.card_cache_bytes.saturating_sub(
                                    old.width() as usize * old.height() as usize * 4,
                                );
                            }
                        }
                        None => break,
                    }
                }
                self.card_cache_bytes += sz;
                self.card_cache.insert(key, (card, now));
            }
            if let Some((card, used)) = self.card_cache.get_mut(&key) {
                *used = now;
                phosphor::blit::blit(pm, card, q.quad.x.round() as i32, q.quad.y.round() as i32);
            }
            if self.scene.get(q.id).and_then(|win| win.cluster).is_some() {
                cluster_bbox = Some(match cluster_bbox {
                    None => (q.quad.x, q.quad.y, q.quad.w, q.quad.h),
                    Some((bx, by, bw, bh)) => {
                        let x1 = bx.min(q.quad.x);
                        let y1 = by.min(q.quad.y);
                        let x2 = (bx + bw).max(q.quad.x + q.quad.w);
                        let y2 = (by + bh).max(q.quad.y + q.quad.h);
                        (x1, y1, x2 - x1, y2 - y1)
                    }
                });
            }
        }
        if let Some((bx, by, bw, bh)) = cluster_bbox {
            let n = self
                .scene
                .clusters()
                .values()
                .next()
                .map(|v| v.len())
                .unwrap_or(2);
            demo::cluster_bracket(pm, &fonts, &pal, &m, (bx, by, bw, bh), n);
        }
        for q in &quads {
            if q.pinned && self.mocks.contains_key(&q.id) {
                demo::pip_card(
                    pm,
                    &fonts,
                    &pal,
                    &m,
                    (q.quad.x, q.quad.y, q.quad.w, q.quad.h),
                );
            }
        }

        self.sub[2] = ts.elapsed().as_secs_f32() * 1000.0; // карточки окон
        let ts = Instant::now();
        // Панель и строка подсказок — из кэша (растеризация текста дорога).
        self.draw_chrome(pm, &fonts);
        self.sub[3] = ts.elapsed().as_secs_f32() * 1000.0;

        let t = self.started.elapsed().as_secs_f32();
        self.ecg.push_beat(t, 64.0);

        let clusters = self.scene.clusters().len();
        let suspended = self.scene.windows().iter().filter(|x| x.suspended).count();
        let perf = format!("{:.1}ms", self.avg_frame_ms());
        let rows = [
            ("canvas", "∞".to_string()),
            ("windows", self.scene.len().to_string()),
            ("cluster", clusters.to_string()),
            ("suspended", suspended.to_string()),
            ("zoom", format!("{:.2}x", self.camera.zoom)),
            ("frame", perf),
            ("mode", self.render_mode.to_string()),
        ];
        let rows_ref: Vec<(&str, &str)> = rows.iter().map(|(a, b)| (*a, b.as_str())).collect();

        if self.hud_on && !self.focus_mode {
            let bounds = self
                .scene
                .content_bounds(false)
                .map(|b| {
                    (
                        b.min.x,
                        b.min.y,
                        (b.max.x - b.min.x).max(1.0),
                        (b.max.y - b.min.y).max(1.0),
                    )
                })
                .unwrap_or((0.0, 0.0, 1.0, 1.0));
            let wins: Vec<(f32, f32, f32, f32, bool)> = self
                .scene
                .windows()
                .iter()
                .filter(|x| !x.suspended)
                .map(|x| (x.pos.x, x.pos.y, x.size.x, x.size.y, Some(x.id) == focus))
                .collect();
            let vw = self.camera.viewport.x as f32 / self.camera.zoom.max(0.01);
            let vh = self.camera.viewport.y as f32 / self.camera.zoom.max(0.01);
            let vp = (
                self.camera.center.x - vw * 0.5,
                self.camera.center.y - vh * 0.5,
                vw,
                vh,
            );
            hud::minimap(
                pm,
                &fonts,
                &pal,
                &m,
                (24.0, (h as f32) - 194.0, 280.0, 170.0),
                bounds,
                &wins,
                vp,
            );
            hud::viewport_frame(pm, &pal, &m);
        }

        // Модуль телеметрии + виталы (в focus mode прячем, оставляем только панель и подсказку)
        let module_h = 40.0 + rows.len() as f32 * (m.value_size + 8.0) + 3.0 * 22.0 + 10.0;
        if !self.focus_mode {
            let my = if self.vitals_on && self.hud_on {
                (h as f32) - module_h - 24.0 - 74.0
            } else {
                (h as f32) - module_h - 24.0
            };
            demo::telemetry_module(
                pm,
                &fonts,
                &pal,
                &m,
                (w as f32) - 324.0,
                if self.hud_on {
                    my
                } else {
                    (h as f32) - module_h - 24.0
                },
                300.0,
                &rows_ref,
                &[
                    ("CPU", 0.37, false),
                    ("RAM", 0.62, false),
                    ("PWR", 0.12, true),
                ],
            );
            if self.vitals_on && self.hud_on {
                hud::vitals(
                    pm,
                    &fonts,
                    &pal,
                    &m,
                    (w as f32) - 324.0,
                    (h as f32) - module_h - 98.0,
                    300.0,
                    68.0,
                    &self.ecg,
                    64.0,
                );
            }
            // HUD окна под курсором
            let cpt = self.canvas_pt();
            if let Some(id) = self.scene.hit_test(cpt) {
                if let Some(win) = self.scene.get(id) {
                    let sq = canvas_to_screen(&self.camera, win.pos);
                    let app = self.mocks.get(&id).map(|mk| mk.app).unwrap_or("window");
                    hud::window_hud(
                        pm,
                        &fonts,
                        &pal,
                        &m,
                        (
                            sq.x,
                            sq.y,
                            win.size.x * self.camera.zoom,
                            win.size.y * self.camera.zoom,
                        ),
                        app,
                        (win.size.x, win.size.y),
                        self.camera.zoom,
                    );
                }
            }
        }

        if self.beam_on && self.rich() {
            hud::scan_beam(pm, &pal, (t % 7.0) / 7.0);
        }

        // Нижняя строка подсказок: шелл должен объяснять себя сам.
        if !self.focus_mode {
            let hint = if self.menu.is_some() {
                ": фильтр · ↑↓ выбор · ENTER выполнить · ESC закрыть"
            } else if self.help {
                "ESC — вернуться"
            } else {
                ": меню · ? справка · L лаунчер · W overview · M fit · S suspend · TAB окна · F2 HUD · F3 focus · 1/2/3 тема"
            };
        }

        if let Some(t) = &self.toast {
            if t.alive() {
                let _ = hud::toast(pm, &fonts, &pal, &m, w as f32, 40.0, t);
            }
        }

        if self.launcher {
            let lw = 560.0;
            demo::launcher_overlay(
                pm,
                &fonts,
                &pal,
                &m,
                ((w as f32) - lw) * 0.5,
                110.0,
                lw,
                "zui",
                &[
                    ("zui-terminal", "terminal", true),
                    ("zui-notes", "tad document", false),
                    ("zui-files", "canvas objects", false),
                    ("zui-settings", "shell config", false),
                ],
            );
        }
        if let Some(st) = &self.menu {
            let _ = menu::draw(pm, &fonts, &pal, &m, w as f32, h as f32, st);
        }
        if self.help {
            let rows = menu::help_rows();
            menu::draw_help(pm, &fonts, &pal, &m, w as f32, h as f32, &rows);
        }
        let osd_fresh = self
            .osd
            .as_ref()
            .map(|(_, _, _, at)| at.elapsed() < Duration::from_millis(2500))
            .unwrap_or(false);
        if osd_fresh {
            if let Some((label, value, text, _)) = &self.osd {
                let _ = phosphor::draw_osd(
                    pm, w as f32, h as f32, label, *value, text, &fonts, &pal, &m,
                );
            }
        } else if self.osd.is_some() {
            self.osd = None;
        }

        self.sub[4] = ts.elapsed().as_secs_f32() * 1000.0; // HUD (миникарта/виталы/рамка)
        self.fonts = Some(fonts);
    }

    /// Верхняя панель + нижняя строка: кэшируем полосы, накладываем копией.
    fn draw_chrome(&mut self, pm: &mut Pixmap, fonts: &Fonts) {
        let (w, h) = self.size;
        let pal = self.palette();
        let m = self.metrics;
        let clock = chrono::Local::now();
        let clusters = self.scene.clusters().len();
        let suspended = self.scene.windows().iter().filter(|x| x.suspended).count();
        let hint = if self.menu.is_some() {
            ": фильтр · ↑↓ выбор · ENTER выполнить · ESC закрыть"
        } else if self.help {
            "ESC — вернуться"
        } else {
            ": меню · ? справка · L лаунчер · W overview · M fit · S suspend · TAB окна · F2 HUD · F3 focus · 1/2/3 тема"
        };
        let right = format!("{:.0} FPS · {}", self.last_fps, self.render_mode);
        // Ключ: всё, что реально печатается в полосах. Мс округляем до 1 мс,
        // чтобы панель не пересобиралась каждый кадр.
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            w.hash(&mut hasher);
            (self.mode as u8).hash(&mut hasher);
            clock.format("%H:%M").to_string().hash(&mut hasher);
            self.render_mode.hash(&mut hasher);
            (self.avg_frame_ms() as u32).hash(&mut hasher);
            self.scene.len().hash(&mut hasher);
            clusters.hash(&mut hasher);
            suspended.hash(&mut hasher);
            ((self.camera.center.x / 8.0) as i32).hash(&mut hasher);
            ((self.camera.center.y / 8.0) as i32).hash(&mut hasher);
            ((self.camera.zoom * 20.0) as i32).hash(&mut hasher);
            self.hud_on.hash(&mut hasher);
            self.focus_mode.hash(&mut hasher);
            self.menu.is_some().hash(&mut hasher);
            self.help.hash(&mut hasher);
            hasher.finish()
        };
        if self.chrome.as_ref().map(|(_, _, k)| *k) != Some(key) {
            let panel_h = m.panel_h.max(1.0) as u32;
            let bar_h = 18u32;
            let mut top = Pixmap::new(w.max(1), panel_h).unwrap();
            let _ = draw_top_panel(
                &mut top,
                w as f32,
                fonts,
                &PanelData {
                    title: "zui-tad",
                    workspace: 3,
                    canvas_pos: (self.camera.center.x, self.camera.center.y),
                    zoom: self.camera.zoom,
                    windows: self.scene.len(),
                    clock: &clock.format("%H:%M").to_string(),
                    date: &clock.format("%d %b").to_string().to_uppercase(),
                    meters: [
                        (
                            "cpu",
                            (self.avg_frame_ms() / FRAME_BUDGET_MS).clamp(0.0, 1.0),
                            self.avg_frame_ms() > FRAME_BUDGET_MS,
                        ),
                        ("ram", 0.62, false),
                        ("vol", 0.80, false),
                        ("bat", 0.12, true),
                    ],
                },
                &pal,
                &m,
            );
            let mut bottom = Pixmap::new(w.max(1), bar_h).unwrap();
            hint_bar(
                &mut bottom,
                fonts,
                &pal,
                &m,
                w as f32,
                bar_h as f32,
                hint,
                &right,
            );
            self.chrome = Some((top, bottom, key));
        }
        if let Some((top, bottom, _)) = &self.chrome {
            phosphor::blit::blit(pm, top, 0, 0);
            phosphor::blit::blit(pm, bottom, 0, (h as i32) - bottom.height() as i32);
        }
    }

    fn blit(&mut self, pm: &Pixmap) {
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let (w, h) = self.size;
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        if surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        let data = pm.data();
        let (pm_w, pm_h) = (pm.width() as usize, pm.height() as usize);
        let (wu, hu) = (w as usize, h as usize);
        let rows_per_band = hu.div_ceil(4).max(1);
        std::thread::scope(|scope| {
            for (bi, band) in buffer.chunks_mut(rows_per_band * wu).enumerate() {
                let y0 = bi * rows_per_band;
                scope.spawn(move || {
                    for (r, row) in band.chunks_mut(wu).enumerate() {
                        let y = y0 + r;
                        if y >= pm_h {
                            row.fill(0xff00_0000);
                            continue;
                        }
                        let src = y * pm_w * 4;
                        for (x, cell) in row.iter_mut().enumerate() {
                            if x >= pm_w {
                                *cell = 0xff00_0000;
                                continue;
                            }
                            let i = src + x * 4;
                            // Кадр после compose непрозрачен → без ветвления по альфе.
                            *cell = 0xff00_0000
                                | ((data[i] as u32) << 16)
                                | ((data[i + 1] as u32) << 8)
                                | (data[i + 2] as u32);
                        }
                    }
                });
            }
        });
        let _ = buffer.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("ZUI-TAD · phosphor shell (CPU, no GL)")
            .with_inner_size(LogicalSize::new(1600.0, 900.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("zui-preview: create_window: {e}");
                event_loop.exit();
                return;
            }
        };
        match softbuffer::Context::new(window.clone())
            .and_then(|ctx| softbuffer::Surface::new(&ctx, window.clone()))
        {
            Ok(s) => self.surface = Some(s),
            Err(e) => {
                eprintln!("zui-preview: softbuffer: {e}");
                event_loop.exit();
                return;
            }
        }
        let size = window.inner_size();
        self.size = (size.width.max(2), size.height.max(2));
        self.camera.viewport = Vector2::new(self.size.0, self.size.1);
        self.window = Some(window);
        println!(
            "zui-preview: окно {}x{} · CPU · меню ':' · справка '?'",
            self.size.0, self.size.1
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WinitWindowId,
        event: WindowEvent,
    ) {
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.last_frame = Instant::now() - Duration::from_millis(64);
        }
        match event {
            WindowEvent::CloseRequested => {
                self.save_cfg();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.size = (size.width.max(2), size.height.max(2));
                self.camera.viewport = Vector2::new(self.size.0, self.size.1);
                self.card_cache.clear();
                self.card_cache_bytes = 0;
                self.backdrop = None;
            }
            WindowEvent::ModifiersChanged(mods) => self.mods = mods.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Point2::new(position.x, position.y);
                self.on_cursor();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.on_primary(state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 24.0, y * 24.0),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                if let Some(st) = self.menu.as_mut() {
                    if dy < 0.0 {
                        st.move_sel(1);
                    } else if dy > 0.0 {
                        st.move_sel(-1);
                    }
                    let n = st.filtered().len();
                    let (start, _) = menu::visible_range(st.sel, n, menu::VISIBLE_ROWS);
                    st.scroll = start;
                } else if self.mods.super_key() {
                    let cursor = Vector2::new(self.pointer.x as f32, self.pointer.y as f32);
                    let f = (1.0 - dy * 0.002).clamp(0.5, 2.0);
                    self.camera.zoom_at(cursor, f);
                    let z = self.camera.zoom;
                    self.set_osd("zoom", (z / 8.0).clamp(0.0, 1.0), format!("{z:.2}x"));
                } else {
                    self.camera.pan(Vector2::new(dx, dy));
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.on_key(event_loop, event.logical_key.clone());
            }
            WindowEvent::RedrawRequested => {
                let t0 = Instant::now();
                let (w, h) = self.size;
                // Кадровый буфер переиспользуем (было: аллокация 5.8 МБ каждый кадр).
                let mut pm = match self.frame_buf.take() {
                    Some(p) if p.width() == w && p.height() == h => p,
                    _ => Pixmap::new(w, h).unwrap(),
                };
                self.compose_into(&mut pm);
                let t_compose = t0.elapsed().as_secs_f32() * 1000.0;

                // Фосфор отдельной фазой: видно, сколько стоит CRT.
                let t1 = Instant::now();
                let rich = self.rich();
                let pal = self.palette();
                let mut params = CrtParams::from_palette(&pal, 1337);
                if !rich {
                    params.grain = 0.0;
                    params.dither_levels = 255;
                    params.chroma = 0.0;
                    self.render_mode = "LEAN";
                } else {
                    params.grain = pal.grain * 0.6;
                    self.render_mode = match self.quality {
                        Quality::Auto => "AUTO·RICH",
                        Quality::Rich => "RICH",
                        Quality::Lean => "LEAN",
                    };
                }
                crt(&mut pm, &params);
                let t_crt = t1.elapsed().as_secs_f32() * 1000.0;

                let t2 = Instant::now();
                self.blit(&pm);
                let t_blit = t2.elapsed().as_secs_f32() * 1000.0;
                self.frame_buf = Some(pm);
                let ms = t0.elapsed().as_secs_f32() * 1000.0;
                if self.frame_ms.len() >= 120 {
                    self.frame_ms.remove(0);
                }
                self.frame_ms.push(ms);
                self.frames += 1;
                if self.fps_t.elapsed() >= Duration::from_secs(1) {
                    let fps = self.frames as f32 / self.fps_t.elapsed().as_secs_f32();
                    self.last_fps = fps;
                    println!(
                        "zui-preview: {fps:.0} fps · frame {:.1}ms = compose {:.1} [bg {:.1} grid {:.1} cards {:.1} chrome {:.1} hud {:.1}] + crt {:.1} + blit {:.1} · {} · RSS {} МБ",
                        ms,
                        t_compose,
                        self.sub[0],
                        self.sub[1],
                        self.sub[2],
                        self.sub[3],
                        self.sub[4],
                        t_crt,
                        t_blit,
                        self.render_mode,
                        rss_mb()
                    );
                    self.frames = 0;
                    self.fps_t = Instant::now();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let due = self.last_frame + Duration::from_millis(16);
        if now >= due {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(due.max(now)));
    }
}

/// Текущий RSS процесса в МБ (для телеметрии в логе).
fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
        })
        .map(|kb| kb / 1024)
        .unwrap_or(0)
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    println!(
        "zui-preview: shell config: {}",
        ShellConfig::path().display()
    );
    event_loop.run_app(&mut app)?;
    Ok(())
}

// неиспользуемые, но полезные в отладке хелперы держим рядом
#[allow(dead_code)]
fn cursor_for(h: ResizeHandle) -> CursorIcon {
    match h {
        ResizeHandle::N | ResizeHandle::S => CursorIcon::NsResize,
        ResizeHandle::E | ResizeHandle::W => CursorIcon::EwResize,
        ResizeHandle::Nw | ResizeHandle::Se => CursorIcon::NwseResize,
        ResizeHandle::Ne | ResizeHandle::Sw => CursorIcon::NeswResize,
    }
}
