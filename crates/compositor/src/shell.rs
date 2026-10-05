//! Слой оболочки внутри композитора: панель, HUD, подсказки, меню, справка.
//!
//! Рисуется в `tiny_skia::Pixmap` (тот же код `phosphor`, что и в превью) и
//! загружается в GL как `MemoryRenderBuffer`. Обновляется ТОЛЬКО когда
//! изменилось содержимое (секунда часов, состояние меню, набор окон) — в
//! простое это один блит текстуры на кадр.
//!
//! Зачем внутри композитора: без этого оболочка существовала бы только в
//! превью, а в реальной сессии поверх приложений не было бы ни панели, ни меню.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use phosphor::config::ShellConfig;
use phosphor::hud::{self, Toast};
use phosphor::menu::{self, Action, MenuState, ThemeId};
use phosphor::panel::{draw_top_panel, hint_bar, PanelData};
use phosphor::texture::{texture, TexKind};
use phosphor::theme::{Metrics, Mode, Palette};
use phosphor::widgets::Fonts;
use phosphor::widgets::{draw_text, frame};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::{Buffer, Rectangle, Size, Transform};
use tiny_skia::Pixmap;

/// Что оболочке нужно знать о сессии, чтобы нарисовать себя.
pub struct ShellCtx<'a> {
    pub w: u32,
    pub h: u32,
    pub camera: (f32, f32),
    pub zoom: f32,
    /// Прямоугольники окон на холсте + фокус (для миникарты).
    pub windows: &'a [(f32, f32, f32, f32, bool)],
    pub bounds: (f32, f32, f32, f32),
    pub viewport: (f32, f32, f32, f32),
    pub suspended: usize,
    pub clusters: usize,
    pub frame_ms: f32,
    pub quality: &'a str,
    pub title: &'a str,
    pub clock: String,
    pub date: String,
    pub toast: Option<(String, String, f32)>,
    /// Усыплённые окна: (x, y, w, h, заголовок) в мировых координатах.
    pub placeholders: &'a [(f32, f32, f32, f32, String)],
}

/// Клавиши, которыми управляется оболочка (не приложение).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKey {
    Up,
    Down,
    Enter,
    Esc,
    Backspace,
    Char(char),
    ToggleMenu,
    ToggleHelp,
    ToggleHud,
    ToggleFocus,
    NextQuality,
}

pub struct ShellLayer {
    pub mode: Mode,
    pub menu: Option<MenuState>,
    pub help: bool,
    pub hud: bool,
    pub focus_mode: bool,
    pub vitals: bool,
    pub beam: bool,
    pub quality: &'static str,
    pub buffer: Option<MemoryRenderBuffer>,
    size: (u32, u32),
    last_key: u64,
    pub toast: Option<Toast>,
    started: Instant,
    cfg: ShellConfig,
    fonts: Option<Fonts>,
    metrics: Metrics,
}

impl Default for ShellLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellLayer {
    pub fn new() -> Self {
        let cfg = ShellConfig::load();
        let mode = match cfg.theme_or_default() {
            "signalis" => Mode::Signalis,
            "phosphor" => Mode::Phosphor,
            _ => Mode::Rig,
        };
        Self {
            mode,
            menu: None,
            help: false,
            hud: cfg.hud,
            focus_mode: cfg.focus_mode,
            vitals: cfg.vitals,
            beam: cfg.beam,
            quality: "AUTO·RICH",
            buffer: None,
            size: (0, 0),
            last_key: 0,
            toast: Some(Toast::new("shell", "canvas compositor online")),
            started: Instant::now(),
            cfg,
            fonts: Fonts::load_default().ok(),
            metrics: Metrics::default(),
        }
    }

    /// Шрифты оболочки (нужны CPU-композитору для плейсхолдеров).
    pub fn fonts(&self) -> Option<&Fonts> {
        self.fonts.as_ref()
    }

    /// Метрики оболочки.
    pub fn metrics(&self) -> Metrics {
        self.metrics
    }

    pub fn save(&mut self) {
        self.cfg.theme = match self.mode {
            Mode::Rig => "rig",
            Mode::Signalis => "signalis",
            Mode::Phosphor => "phosphor",
        }
        .into();
        self.cfg.hud = self.hud;
        self.cfg.focus_mode = self.focus_mode;
        self.cfg.vitals = self.vitals;
        self.cfg.beam = self.beam;
        let _ = self.cfg.save();
    }

    /// Ключ перерисовки: если не изменился — слой не пересобираем.
    fn redraw_key(&self, ctx: &ShellCtx) -> u64 {
        let mut h = DefaultHasher::new();
        ctx.w.hash(&mut h);
        ctx.h.hash(&mut h);
        (self.mode as u8).hash(&mut h);
        ctx.clock.hash(&mut h);
        ctx.windows.len().hash(&mut h);
        ctx.suspended.hash(&mut h);
        ctx.clusters.hash(&mut h);
        ((ctx.camera.0 / 4.0) as i32).hash(&mut h);
        ((ctx.camera.1 / 4.0) as i32).hash(&mut h);
        ((ctx.zoom * 100.0) as i32).hash(&mut h);
        ((ctx.frame_ms * 4.0) as i32).hash(&mut h);
        ctx.quality.hash(&mut h);
        for ph in ctx.placeholders {
            ph.4.hash(&mut h);
        }
        self.help.hash(&mut h);
        self.hud.hash(&mut h);
        self.focus_mode.hash(&mut h);
        self.vitals.hash(&mut h);
        self.quality.hash(&mut h);
        if let Some(m) = &self.menu {
            m.query.hash(&mut h);
            m.sel.hash(&mut h);
        } else {
            0u8.hash(&mut h);
        }
        self.toast.as_ref().map(|t| t.alive()).hash(&mut h);
        h.finish()
    }

    /// Пересобрать слой, если что-то изменилось. Возвращает true, если обновили.
    pub fn refresh(&mut self, ctx: &ShellCtx) -> bool {
        let key = self.redraw_key(ctx);
        if key == self.last_key && self.size == (ctx.w, ctx.h) {
            return false;
        }
        self.last_key = key;
        let pm = self.render(ctx);
        self.upload(&pm);
        true
    }

    pub fn render(&self, ctx: &ShellCtx) -> Pixmap {
        let (w, h) = (ctx.w, ctx.h);
        let mut pm = Pixmap::new(w, h).unwrap();
        let Some(fonts) = &self.fonts else {
            return pm;
        };
        let pal = Palette::of(self.mode);
        let m = self.metrics;

        // Панель
        let _ = draw_top_panel(
            &mut pm,
            w as f32,
            fonts,
            &PanelData {
                title: ctx.title,
                workspace: 3,
                canvas_pos: ctx.camera,
                zoom: ctx.zoom,
                windows: ctx.windows.len(),
                clock: &ctx.clock,
                date: &ctx.date,
                meters: [
                    (
                        "cpu",
                        (ctx.frame_ms / 16.6).clamp(0.0, 1.0),
                        ctx.frame_ms > 16.6,
                    ),
                    ("ram", 0.62, false),
                    ("vol", 0.80, false),
                    ("bat", 0.12, true),
                ],
            },
            &pal,
            &m,
        );

        // Плейсхолдеры усыплённых окон (восстановленная сессия): холодный контур
        // на месте живого окна + подпись, чтобы «где что было» не терялось.
        {
            let (cx, cy) = (ctx.camera.0, ctx.camera.1);
            let z = ctx.zoom.max(0.02);
            for (wx, wy, ww, wh, title) in ctx.placeholders {
                let sx = (wx - cx) * z + w as f32 * 0.5;
                let sy = (wy - cy) * z + h as f32 * 0.5;
                let sw = ww * z;
                let sh = wh * z;
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
                    draw_text(
                        &mut pm,
                        &fonts.regular,
                        "DORMANT · МЕСТО СОХРАНЕНО",
                        sx + 10.0,
                        sy + 34.0,
                        m.label_size,
                        m.tracking,
                        pal.dim,
                    );
                }
            }
        }

        // HUD (миникарта, виталы) — можно выключить в focus mode
        if self.hud && !self.focus_mode {
            hud::minimap(
                &mut pm,
                fonts,
                &pal,
                &m,
                (24.0, (h as f32) - 194.0 - 18.0, 280.0, 170.0),
                ctx.bounds,
                ctx.windows,
                ctx.viewport,
            );
            hud::viewport_frame(&mut pm, &pal, &m);
            if self.vitals {
                let mut ecg = phosphor::hud::Ecg::new(96);
                let t = self.started.elapsed().as_secs_f32();
                for i in 0..96 {
                    ecg.push_beat(t - (96 - i) as f32 * 0.02, 64.0);
                }
                hud::vitals(
                    &mut pm,
                    fonts,
                    &pal,
                    &m,
                    (w as f32) - 324.0,
                    (h as f32) - 194.0,
                    300.0,
                    68.0,
                    &ecg,
                    64.0,
                );
            }
            if self.beam {
                let phase = (self.started.elapsed().as_secs_f32() % 7.0) / 7.0;
                hud::scan_beam(&mut pm, &pal, phase);
            }
        }

        // Подсказки + тост
        let hint = if self.menu.is_some() {
            "↑↓ выбор · ENTER выполнить · ESC закрыть · набор — фильтр"
        } else if self.help {
            "ESC — вернуться"
        } else {
            "Mod+D меню · F1 справка · Mod+W overview · Mod+M fit · Mod+S suspend · ALT+TAB окна · Mod+1..4 закладки"
        };
        let right = format!("{} · {} мс", self.quality, ctx.frame_ms);
        hint_bar(&mut pm, fonts, &pal, &m, w as f32, h as f32, hint, &right);

        if let Some(t) = &self.toast {
            if t.alive() {
                let _ = hud::toast(&mut pm, fonts, &pal, &m, w as f32, 40.0, t);
            }
        }

        if self.help {
            let rows = menu::help_rows();
            menu::draw_help(&mut pm, fonts, &pal, &m, w as f32, h as f32, &rows);
        } else if let Some(st) = &self.menu {
            let _ = menu::draw(&mut pm, fonts, &pal, &m, w as f32, h as f32, st);
        }
        pm
    }

    /// Загрузить пиксели в GL-буфер (RGBA premultiplied → ARGB8888).
    pub fn upload(&mut self, pm: &Pixmap) {
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        if self.buffer.is_none() || self.size != (pm.width(), pm.height()) {
            self.buffer = Some(MemoryRenderBuffer::new(
                Fourcc::Argb8888,
                (w, h),
                1,
                Transform::Normal,
                None,
            ));
            self.size = (pm.width(), pm.height());
        }
        let Some(buf) = self.buffer.as_mut() else {
            return;
        };
        let data = pm.data();
        let _ = buf.render().draw(|mem: &mut [u8]| {
            if mem.len() >= data.len() {
                for i in (0..data.len()).step_by(4) {
                    mem[i] = data[i + 2];
                    mem[i + 1] = data[i + 1];
                    mem[i + 2] = data[i];
                    mem[i + 3] = data[i + 3];
                }
            }
            Ok::<_, std::convert::Infallible>(vec![Rectangle::from_size(
                Size::<i32, Buffer>::from((w, h)),
            )])
        });
    }

    /// Роут клавиши. `Some(action)` — выполнить в композиторе; `true` во втором
    /// поле означает «событие поглощено оболочкой» (в приложение не уходит).
    pub fn handle_key(&mut self, key: ShellKey) -> (bool, Option<Action>) {
        match key {
            ShellKey::ToggleMenu => {
                self.menu = if self.menu.is_some() {
                    None
                } else {
                    Some(MenuState::new())
                };
                (true, None)
            }
            ShellKey::ToggleHelp => {
                self.help = !self.help;
                (true, None)
            }
            ShellKey::ToggleHud => {
                self.hud = !self.hud;
                (true, None)
            }
            ShellKey::ToggleFocus => {
                self.focus_mode = !self.focus_mode;
                (true, None)
            }
            ShellKey::NextQuality => {
                self.quality = match self.quality {
                    "AUTO·RICH" => "RICH",
                    "RICH" => "LEAN",
                    _ => "AUTO·RICH",
                };
                (true, None)
            }
            _ => {
                if let Some(st) = self.menu.as_mut() {
                    match key {
                        ShellKey::Esc => {
                            self.menu = None;
                            return (true, None);
                        }
                        ShellKey::Up => st.move_sel(-1),
                        ShellKey::Down => st.move_sel(1),
                        ShellKey::Backspace => st.backspace(),
                        ShellKey::Char(c) => st.type_char(c),
                        ShellKey::Enter => {
                            let action = st.selected().map(|i| menu::entries()[i].action);
                            self.menu = None;
                            return (true, action);
                        }
                        _ => {}
                    }
                    if let Some(st) = self.menu.as_mut() {
                        let n = st.filtered().len();
                        let (start, _) = menu::visible_range(st.sel, n, menu::VISIBLE_ROWS);
                        st.scroll = start;
                    }
                    (true, None)
                } else if self.help {
                    self.help = false; // любая клавиша закрывает справку
                    (true, None)
                } else {
                    (false, None)
                }
            }
        }
    }

    /// Клик по оболочке: меню (строка) или панель (открыть меню).
    /// Возвращает `Some(Some(action))` — выполнить, `Some(None)` — поглотить,
    /// `None` — клик не наш, отдать на холст.
    pub fn handle_click(&mut self, x: f32, y: f32, screen: (u32, u32)) -> Option<Option<Action>> {
        let m = self.metrics;
        if let Some(st) = self.menu.clone() {
            let geom = menu::geometry(
                screen.0 as f32,
                screen.1 as f32,
                st.filtered().len().max(1),
                &m,
            );
            if let Some(row) = menu::hit(&geom, y, st.scroll, st.filtered().len()) {
                let mut st2 = st.clone();
                st2.sel = row;
                let action = st2.selected().map(|i| menu::entries()[i].action);
                self.menu = None;
                return Some(action);
            }
            if y < geom.rect.1
                || y > geom.rect.1 + geom.rect.3
                || x < geom.rect.0
                || x > geom.rect.0 + geom.rect.2
            {
                self.menu = None;
                return Some(None);
            }
            return Some(None);
        }
        if self.help {
            self.help = false;
            return Some(None);
        }
        // клик по панели — открыть меню (как в Noctalia: панель интерактивна)
        if y <= m.panel_h {
            self.menu = Some(MenuState::new());
            return Some(None);
        }
        None
    }

    pub fn set_theme(&mut self, t: ThemeId) {
        self.mode = t.mode();
        self.toast = Some(Toast::new("theme", t.label()));
        self.save();
    }
}

/// Подписи меню для подсказок в композиторе (используется в логах).
pub fn action_name(a: &Action) -> &'static str {
    match a {
        Action::SetTheme(ThemeId::Rig) => "theme:rig",
        Action::SetTheme(ThemeId::Signalis) => "theme:signalis",
        Action::SetTheme(ThemeId::Phosphor) => "theme:phosphor",
        Action::ToggleHud => "hud",
        Action::ToggleFocus => "focus",
        Action::ToggleVitals => "vitals",
        Action::ToggleBeam => "beam",
        Action::QualityAuto => "quality:auto",
        Action::QualityRich => "quality:rich",
        Action::QualityLean => "quality:lean",
        Action::Help => "help",
        Action::Launcher => "launcher",
        Action::Home => "home",
        Action::Overview => "overview",
        Action::FitWindow => "fit-window",
        Action::SuspendFocused => "suspend",
        Action::CycleWindows => "cycle-windows",
        Action::Quit => "quit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_swallows_keys_until_escape() {
        let mut s = ShellLayer::new();
        s.menu = Some(MenuState::new());
        let (consumed, action) = s.handle_key(ShellKey::Char('h'));
        assert!(consumed && action.is_none(), "набор идёт в фильтр");
        assert_eq!(s.menu.as_ref().unwrap().query, "h");
        let (consumed, action) = s.handle_key(ShellKey::Enter);
        assert!(consumed, "Enter внутри меню поглощён");
        assert!(action.is_some(), "Enter возвращает действие");
        assert!(s.menu.is_none(), "после Enter меню закрыто");
    }

    #[test]
    fn toggle_menu_and_help() {
        let mut s = ShellLayer::new();
        let (c, _) = s.handle_key(ShellKey::ToggleMenu);
        assert!(c && s.menu.is_some());
        let (c, _) = s.handle_key(ShellKey::ToggleMenu);
        assert!(c && s.menu.is_none());
        let (c, _) = s.handle_key(ShellKey::ToggleHelp);
        assert!(c && s.help);
        let (c, _) = s.handle_key(ShellKey::Char('x'));
        assert!(c && !s.help, "любая клавиша закрывает справку");
    }

    #[test]
    fn keys_without_menu_go_to_app() {
        let mut s = ShellLayer::new();
        let (consumed, action) = s.handle_key(ShellKey::Char('a'));
        assert!(!consumed && action.is_none(), "буквы уходят в приложение");
    }

    #[test]
    fn click_on_panel_opens_menu_and_outside_closes() {
        let mut s = ShellLayer::new();
        let screen = (1280, 800);
        // панель
        assert!(matches!(s.handle_click(100.0, 5.0, screen), Some(None)));
        assert!(s.menu.is_some(), "клик по панели открывает меню");
        // клик мимо окна меню — закрывает
        s.handle_click(10.0, 780.0, screen);
        assert!(s.menu.is_none());
        // клик по холсту — не наш
        assert!(s.handle_click(600.0, 400.0, screen).is_none());
    }

    #[test]
    fn redraw_key_depends_on_clock_and_flags() {
        let mut s = ShellLayer::new();
        let wins: [(f32, f32, f32, f32, bool); 1] = [(0.0, 0.0, 100.0, 100.0, true)];
        let mk = |clock: &str| ShellCtx {
            w: 1280,
            h: 800,
            camera: (0.0, 0.0),
            zoom: 1.0,
            windows: &wins,
            bounds: (0.0, 0.0, 100.0, 100.0),
            viewport: (0.0, 0.0, 1280.0, 800.0),
            suspended: 0,
            clusters: 0,
            frame_ms: 9.0,
            quality: "AUTO\u{b7}RICH",
            title: "zui-tad",
            clock: clock.to_string(),
            date: "29 SEP".into(),
            toast: None,
            placeholders: &[],
        };
        let k1 = s.redraw_key(&mk("21:47"));
        assert_ne!(k1, s.redraw_key(&mk("21:48")), "смена минуты → перерисовка");
        assert_eq!(
            k1,
            s.redraw_key(&mk("21:47")),
            "тот же контекст → тот же ключ"
        );
        s.hud = !s.hud;
        assert_ne!(k1, s.redraw_key(&mk("21:47")), "флаг HUD меняет ключ");
    }
}
