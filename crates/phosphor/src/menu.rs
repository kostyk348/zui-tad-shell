//! Меню оболочки: командная палитра + справка по горячим клавишам.
//!
//! Подача — плотная и «афишная» (Persona-подобная: наклонный заголовок,
//! инверсная подсветка строки, группы подписей), но краски наши — фосфорный
//! монохром. Всё кликабельно И управляется с клавиатуры: шелл должен быть
//! удобен для долгой работы, а не только красив.

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::icons::{icon, Icon};
use crate::texture::{texture, TexKind};
use crate::theme::{Metrics, Mode, Palette};
use crate::widgets::{draw_text, frame, hairline, text_width, Fonts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    Rig,
    Signalis,
    Phosphor,
}

impl ThemeId {
    pub fn mode(self) -> Mode {
        match self {
            ThemeId::Rig => Mode::Rig,
            ThemeId::Signalis => Mode::Signalis,
            ThemeId::Phosphor => Mode::Phosphor,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Rig => "RIG · ORANGE SIGNAL",
            ThemeId::Signalis => "SIGNALIS · AMBER CRT",
            ThemeId::Phosphor => "PHOSPHOR · GREEN TERMINAL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SetTheme(ThemeId),
    ToggleHud,
    ToggleFocus,
    ToggleVitals,
    ToggleBeam,
    QualityAuto,
    QualityRich,
    QualityLean,
    Help,
    Launcher,
    Home,
    Overview,
    FitWindow,
    SuspendFocused,
    CycleWindows,
    Quit,
}

#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub icon: Icon,
    pub group: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub action: Action,
}

/// Команды палитры. Порядок = порядок вывода (группы идут подряд).
pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            icon: Icon::Palette,
            group: "SHELL",
            label: "Theme: RIG",
            hint: "1",
            action: Action::SetTheme(ThemeId::Rig),
        },
        Entry {
            icon: Icon::Palette,
            group: "SHELL",
            label: "Theme: SIGNALIS",
            hint: "2",
            action: Action::SetTheme(ThemeId::Signalis),
        },
        Entry {
            icon: Icon::Palette,
            group: "SHELL",
            label: "Theme: PHOSPHOR",
            hint: "3",
            action: Action::SetTheme(ThemeId::Phosphor),
        },
        Entry {
            icon: Icon::Window,
            group: "SHELL",
            label: "HUD: on/off",
            hint: "F2",
            action: Action::ToggleHud,
        },
        Entry {
            icon: Icon::Camera,
            group: "SHELL",
            label: "Focus mode (тише HUD)",
            hint: "F3",
            action: Action::ToggleFocus,
        },
        Entry {
            icon: Icon::Music,
            group: "SHELL",
            label: "Vitals: on/off",
            hint: "F4",
            action: Action::ToggleVitals,
        },
        Entry {
            icon: Icon::Zoom,
            group: "SHELL",
            label: "Scan beam: on/off",
            hint: "F5",
            action: Action::ToggleBeam,
        },
        Entry {
            icon: Icon::Cpu,
            group: "SHELL",
            label: "Quality: AUTO / RICH / LEAN",
            hint: "F6",
            action: Action::QualityAuto,
        },
        Entry {
            icon: Icon::Terminal,
            group: "CANVAS",
            label: "Launcher",
            hint: "L",
            action: Action::Launcher,
        },
        Entry {
            icon: Icon::Map,
            group: "CANVAS",
            label: "Home (origin, zoom 1:1)",
            hint: "SPACE",
            action: Action::Home,
        },
        Entry {
            icon: Icon::Zoom,
            group: "CANVAS",
            label: "Overview — все окна",
            hint: "W",
            action: Action::Overview,
        },
        Entry {
            icon: Icon::Window,
            group: "CANVAS",
            label: "Fit focused window",
            hint: "M",
            action: Action::FitWindow,
        },
        Entry {
            icon: Icon::Suspend,
            group: "CANVAS",
            label: "Suspend focused (плейсхолдер)",
            hint: "S",
            action: Action::SuspendFocused,
        },
        Entry {
            icon: Icon::Cluster,
            group: "CANVAS",
            label: "Cycle windows (MRU)",
            hint: "TAB",
            action: Action::CycleWindows,
        },
        Entry {
            icon: Icon::Help,
            group: "INFO",
            label: "Help / FAQ по клавишам",
            hint: "?",
            action: Action::Help,
        },
        Entry {
            icon: Icon::Power,
            group: "INFO",
            label: "Quit preview",
            hint: "ESC",
            action: Action::Quit,
        },
    ]
}

#[derive(Debug, Clone)]
pub struct MenuState {
    pub query: String,
    pub sel: usize,
    /// Прокрутка: индекс первой видимой строки отфильтрованного списка.
    pub scroll: usize,
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            sel: 0,
            scroll: 0,
        }
    }

    /// Индексы подходящих записей (регистронезависимая подстрока по label+group).
    pub fn filtered(&self) -> Vec<usize> {
        let q = self.query.trim().to_lowercase();
        let all = entries();
        if q.is_empty() {
            return (0..all.len()).collect();
        }
        all.iter()
            .enumerate()
            .filter(|(_, e)| {
                e.label.to_lowercase().contains(&q) || e.group.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Индекс выбранной записи в общем списке.
    pub fn selected(&self) -> Option<usize> {
        let f = self.filtered();
        if f.is_empty() {
            None
        } else {
            Some(f[self.sel.min(f.len() - 1)])
        }
    }

    pub fn move_sel(&mut self, delta: i32) {
        let n = self.filtered().len();
        if n == 0 {
            self.sel = 0;
            return;
        }
        let cur = self.sel.min(n - 1) as i32;
        self.sel = (((cur + delta) % n as i32) + n as i32) as usize % n;
    }

    pub fn type_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.query.push(c);
        self.sel = 0;
        self.scroll = 0;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.sel = 0;
    }
}

/// Сколько строк влезает в меню.
pub const VISIBLE_ROWS: usize = 8;

/// Начало/конец видимого окна списка (прокрутка держит выделение в кадре).
pub fn visible_range(sel: usize, total: usize, visible: usize) -> (usize, usize) {
    if total == 0 || visible == 0 {
        return (0, 0);
    }
    let visible = visible.min(total);
    let sel = sel.min(total - 1);
    let start = if sel < visible { 0 } else { sel + 1 - visible };
    (start, (start + visible).min(total))
}

/// Горячие клавиши для справки (единственный источник правды для shell_shot/preview).
pub fn help_rows() -> Vec<(Icon, &'static str, &'static str)> {
    vec![
        (Icon::Menu, ": /", "командное меню"),
        (Icon::Help, "? F1", "эта справка"),
        (Icon::Palette, "1 2 3", "Rig / Signalis / Phosphor"),
        (Icon::Window, "F2", "HUD вкл/выкл"),
        (Icon::Camera, "F3", "focus mode (тихий)"),
        (Icon::Music, "F4", "виталы ЭКГ"),
        (Icon::Zoom, "F5", "луч развёртки"),
        (Icon::Cpu, "F6", "качество AUTO/RICH/LEAN"),
        (Icon::Terminal, "L", "лаунчер"),
        (Icon::Map, "Space", "home (origin, 1:1)"),
        (Icon::Zoom, "W", "overview всех окон"),
        (Icon::Window, "M", "fit окна под экран"),
        (Icon::Suspend, "S", "suspend окна"),
        (Icon::Cluster, "Tab", "окна по MRU"),
        (Icon::Zoom, "+ - 0", "зум / сброс"),
        (Icon::Camera, "Mod+wheel", "зум к курсору"),
        (Icon::Window, "ЛКМ", "фокус и перенос окна"),
        (Icon::Cpu, "ЛКМ+край", "ресайз за 8 краёв"),
        (Icon::Map, "ЛКМ пусто", "панорама холста"),
        (Icon::Power, "Esc", "выход (конфиг сохранится)"),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuGeom {
    pub rect: (f32, f32, f32, f32),
    pub header: (f32, f32, f32, f32),
    pub rows_top: f32,
    pub row_h: f32,
    pub rows: usize,
    pub footer: (f32, f32, f32, f32),
}

pub fn geometry(screen_w: f32, screen_h: f32, rows: usize, m: &Metrics) -> MenuGeom {
    let rows_shown = rows.min(VISIBLE_ROWS);
    let row_h = 30.0;
    let header_h = 44.0;
    let footer_h = 26.0;
    let w = 620.0f32.min(screen_w - 40.0);
    let h = header_h + rows_shown as f32 * row_h + footer_h + 10.0;
    let x = ((screen_w - w) * 0.5).round();
    let y = ((screen_h - h) * 0.42).round().max(m.panel_h + 8.0);
    MenuGeom {
        rect: (x, y, w, h),
        header: (x, y, w, header_h),
        rows_top: y + header_h + 4.0,
        row_h,
        rows: rows_shown,
        footer: (x, y + h - footer_h, w, footer_h),
    }
}

/// Какая строка под курсором (учитывает прокрутку).
pub fn hit(geom: &MenuGeom, my: f32, scroll: usize, total: usize) -> Option<usize> {
    if my < geom.rows_top {
        return None;
    }
    let idx = ((my - geom.rows_top) / geom.row_h).floor();
    if idx < 0.0 || idx >= geom.rows as f32 {
        return None;
    }
    let i = scroll + idx as usize;
    if i < total {
        Some(i)
    } else {
        None
    }
}

/// Нарисовать палитру. Возвращает геометрию (для хит-теста мышью).
pub fn draw(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    screen_w: f32,
    screen_h: f32,
    st: &MenuState,
) -> MenuGeom {
    let filtered = st.filtered();
    let geom = geometry(screen_w, screen_h, filtered.len().max(1), m);
    let (x, y, w, h) = geom.rect;

    // затемнение фона — чтобы холст не спорил с меню
    let mut scrim = Paint::default();
    scrim.set_color_rgba8(0, 0, 0, 0x66);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, screen_w, screen_h) {
        pm.fill_rect(r, &scrim, Transform::identity(), None);
    }

    // корпус + текстуры
    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.panel_bg[0], pal.panel_bg[1], pal.panel_bg[2], 0xf7);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    texture(
        pm,
        (x, y + geom.header.3, w, h - geom.header.3),
        TexKind::Halftone,
        pal.dim,
        7.0,
        11,
    );
    frame(pm, (x, y, w, h), pal.primary, m.line, 8.0);

    // Заголовок: наклонная плашка (Persona-приём), текст инверсно.
    let hh = geom.header.3;
    let slant = 12.0;
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w - slant, y + hh);
    pb.line_to(x, y + hh);
    pb.close();
    if let Some(path) = pb.finish() {
        let mut head = Paint::default();
        head.set_color_rgba8(pal.primary[0], pal.primary[1], pal.primary[2], 0xff);
        pm.fill_path(
            &path,
            &head,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    texture(pm, (x, y, w, hh), TexKind::Bands, pal.bg, 5.0, 3);
    let ink = [pal.bg[0], pal.bg[1], pal.bg[2], 0xff];
    draw_text(
        pm,
        &fonts.bold,
        "COMMAND",
        x + 16.0,
        y + 26.0,
        m.value_size * 1.25,
        m.tracking * 1.6,
        ink,
    );
    let glyph = if st.query.is_empty() { ">" } else { "" };
    let qtext = if st.query.is_empty() {
        String::from("type to filter…")
    } else {
        st.query.clone()
    };
    draw_text(
        pm,
        &fonts.regular,
        &qtext,
        x + 168.0,
        y + 26.0,
        m.value_size,
        m.tracking,
        ink,
    );
    if !glyph.is_empty() {
        draw_text(
            pm,
            &fonts.bold,
            glyph,
            x + 150.0,
            y + 26.0,
            m.value_size,
            0.0,
            ink,
        );
    }

    // Строки
    let (start, end) = visible_range(st.sel, filtered.len(), VISIBLE_ROWS);
    let mut last_group = "";
    for (row, fi) in (start..end).enumerate() {
        let e = entries()[filtered[fi]];
        let ry = geom.rows_top + row as f32 * geom.row_h;
        let selected = fi == st.sel.min(filtered.len().saturating_sub(1));

        if selected {
            // инверсная подсветка со скосом — «афиша», но без потери читаемости
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(x + 8.0, ry);
            pb.line_to(x + w - 8.0, ry);
            pb.line_to(x + w - 16.0, ry + geom.row_h - 4.0);
            pb.line_to(x + 8.0, ry + geom.row_h - 4.0);
            pb.close();
            if let Some(path) = pb.finish() {
                let mut sel = Paint::default();
                sel.set_color_rgba8(pal.primary[0], pal.primary[1], pal.primary[2], 0xdd);
                pm.fill_path(
                    &path,
                    &sel,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        let fg = if selected { ink } else { pal.text };
        let dim = if selected { ink } else { pal.dim };

        // группа — только когда меняется (комфортнее читать списком)
        if e.group != last_group && !selected {
            draw_text(
                pm,
                &fonts.regular,
                e.group,
                x + 44.0,
                ry + 12.0,
                m.label_size * 0.9,
                m.tracking,
                dim,
            );
            last_group = e.group;
        }
        icon(pm, e.icon, x + 16.0, ry + 6.0, 16.0, fg);
        draw_text(
            pm,
            &fonts.regular,
            e.label,
            x + 44.0,
            ry + 21.0,
            m.value_size,
            0.5,
            fg,
        );
        let hw = text_width(&fonts.regular, e.hint, m.label_size, m.tracking);
        draw_text(
            pm,
            &fonts.regular,
            e.hint,
            x + w - hw - 26.0,
            ry + 21.0,
            m.label_size,
            m.tracking,
            dim,
        );
    }
    if filtered.is_empty() {
        draw_text(
            pm,
            &fonts.regular,
            "НЕТ СОВПАДЕНИЙ",
            x + 44.0,
            geom.rows_top + 20.0,
            m.value_size,
            m.tracking,
            pal.alert,
        );
    }

    // Подвал: подсказки управления
    let (fx, fy, fw, fh) = geom.footer;
    hairline(pm, fx, fy, fx + fw, fy, pal.dim, m.line);
    draw_text(
        pm,
        &fonts.regular,
        "↑↓ ВЫБОР · ENTER ВЫПОЛНИТЬ · ESC ЗАКРЫТЬ · ? СПРАВКА",
        fx + 16.0,
        fy + fh * 0.72,
        m.label_size,
        m.tracking,
        pal.dim,
    );
    geom
}

/// Справка/FAQ по горячим клавишам — двухколоночный список с иконками.
pub fn draw_help(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    screen_w: f32,
    screen_h: f32,
    hotkeys: &[(Icon, &'static str, &'static str)],
) {
    let w = 780.0f32.min(screen_w - 40.0);
    let row_h = 22.0;
    let per_col = hotkeys.len().div_ceil(2);
    let h = 70.0 + per_col as f32 * row_h + 46.0;
    let x = ((screen_w - w) * 0.5).round();
    let y = ((screen_h - h) * 0.5).round();

    let mut scrim = Paint::default();
    scrim.set_color_rgba8(0, 0, 0, 0x80);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, screen_w, screen_h) {
        pm.fill_rect(r, &scrim, Transform::identity(), None);
    }
    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.panel_bg[0], pal.panel_bg[1], pal.panel_bg[2], 0xf8);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    texture(
        pm,
        (x, y, w, 40.0),
        TexKind::Hatch,
        crate::demo::with_alpha(pal.dim, 0x38),
        11.0,
        5,
    );
    texture(
        pm,
        (x, y + h - 40.0, w, 40.0),
        TexKind::Bands,
        pal.dim,
        4.0,
        7,
    );
    frame(pm, (x, y, w, h), pal.primary, m.line, 10.0);

    draw_text(
        pm,
        &fonts.bold,
        "HELP · HOTKEYS",
        x + 18.0,
        y + 28.0,
        m.value_size * 1.2,
        m.tracking * 1.4,
        pal.primary,
    );
    let sub = "всё работает и мышью: панель, меню, окна";
    let sw = text_width(&fonts.regular, sub, m.label_size, m.tracking);
    draw_text(
        pm,
        &fonts.regular,
        sub,
        x + w - sw - 18.0,
        y + 28.0,
        m.label_size,
        m.tracking,
        pal.dim,
    );

    // Фиксированная колонка клавиш — строки выравниваются, ничего не наезжает.
    const KEY_COL: f32 = 86.0;
    let col_w = (w - 36.0) * 0.5;
    for (i, (ic, keys, what)) in hotkeys.iter().enumerate() {
        let col = i / per_col;
        let row = i % per_col;
        let cx = x + 18.0 + col as f32 * (col_w + 12.0);
        let cy = y + 56.0 + row as f32 * row_h;
        icon(pm, *ic, cx, cy - 10.0, 14.0, pal.primary);
        draw_text(
            pm,
            &fonts.bold,
            keys,
            cx + 22.0,
            cy,
            m.label_size,
            m.tracking,
            pal.text,
        );
        // описание начинается от фиксированной позиции и обрезается по колонке
        let dx = cx + 22.0 + KEY_COL;
        let max_w = (cx + col_w) - dx - 6.0;
        let mut txt = what.to_string();
        while !txt.is_empty() && text_width(&fonts.regular, &txt, m.label_size, m.tracking) > max_w
        {
            txt.pop();
        }
        if txt.len() < what.len() {
            txt.push('…');
        }
        draw_text(
            pm,
            &fonts.regular,
            &txt,
            dx,
            cy,
            m.label_size,
            m.tracking,
            pal.dim,
        );
    }

    let foot = "ЛЮБОЙ ВВОД В ОКНЕ УХОДИТ В ПРИЛОЖЕНИЕ · F2 HUD · F3 FOCUS · F1/? ЭТА СПРАВКА · ESC ЗАКРЫТЬ";
    draw_text(
        pm,
        &fonts.regular,
        foot,
        x + 18.0,
        y + h - 14.0,
        m.label_size,
        m.tracking,
        pal.dim,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    #[test]
    fn filter_matches_label_and_group_case_insensitively() {
        let mut st = MenuState::new();
        assert_eq!(st.filtered().len(), entries().len());
        st.query = "PHOS".into();
        let f = st.filtered();
        assert_eq!(f.len(), 1);
        assert!(entries()[f[0]].label.contains("PHOSPHOR"));
        st.query = "canvas".into();
        assert!(st.filtered().len() >= 4, "группа CANVAS должна находиться");
        st.query = "qwerty".into();
        assert!(st.filtered().is_empty());
    }

    #[test]
    fn selection_wraps_and_tracks_filter() {
        let mut st = MenuState::new();
        assert_eq!(st.selected(), Some(0));
        st.move_sel(-1);
        assert_eq!(
            st.selected(),
            Some(entries().len() - 1),
            "назад с 0 → в конец"
        );
        st.move_sel(1);
        assert_eq!(st.selected(), Some(0));
        st.query = "theme".into();
        let n = st.filtered().len();
        assert_eq!(n, 3);
        st.move_sel(1);
        assert_eq!(st.selected(), Some(st.filtered()[1.min(n - 1)]));
    }

    #[test]
    fn typing_and_backspace_reset_selection() {
        let mut st = MenuState::new();
        st.move_sel(5);
        st.type_char('h');
        assert_eq!(st.sel, 0);
        assert_eq!(st.query, "h");
        st.backspace();
        assert!(st.query.is_empty());
        st.type_char('\n'); // control-символы игнорируются
        assert!(st.query.is_empty());
    }

    #[test]
    fn visible_range_keeps_selection_in_frame() {
        assert_eq!(visible_range(0, 16, 8), (0, 8));
        assert_eq!(visible_range(7, 16, 8), (0, 8));
        assert_eq!(visible_range(8, 16, 8), (1, 9));
        assert_eq!(visible_range(15, 16, 8), (8, 16));
        assert_eq!(visible_range(0, 3, 8), (0, 3), "меньше окна — берём всё");
        assert_eq!(visible_range(0, 0, 8), (0, 0));
    }

    #[test]
    fn geometry_stays_on_screen_and_rows_fit() {
        let m = Metrics::default();
        let g = geometry(1600.0, 900.0, 16, &m);
        assert!(g.rect.0 >= 0.0 && g.rect.0 + g.rect.2 <= 1600.0);
        assert!(g.rect.1 >= m.panel_h, "меню не залезает на панель");
        assert!(g.rect.1 + g.rect.3 <= 900.0);
        assert_eq!(g.rows, VISIBLE_ROWS, "показываем не больше VISIBLE_ROWS");
        let g2 = geometry(520.0, 400.0, 3, &m);
        assert!(g2.rect.2 <= 520.0 - 40.0 + 0.1, "узкий экран — меню уже");
        assert_eq!(g2.rows, 3);
    }

    #[test]
    fn hit_maps_row_with_scroll() {
        let m = Metrics::default();
        let g = geometry(1600.0, 900.0, 16, &m);
        assert_eq!(hit(&g, g.rows_top + 3.0, 0, 16), Some(0));
        assert_eq!(hit(&g, g.rows_top + g.row_h + 3.0, 0, 16), Some(1));
        assert_eq!(hit(&g, g.rows_top + 3.0, 5, 16), Some(5), "с прокруткой");
        assert_eq!(hit(&g, g.rect.1, 0, 16), None, "заголовок — не строка");
        assert_eq!(
            hit(&g, g.rows_top + g.row_h * 9.0, 0, 16),
            None,
            "ниже окна"
        );
    }

    #[test]
    fn drawing_smoke_and_pixels() {
        let fonts = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Rig);
        let m = Metrics::default();
        let mut pm = Pixmap::new(800, 500).unwrap();
        let mut st = MenuState::new();
        st.query = "theme".into();
        let g = draw(&mut pm, &fonts, &pal, &m, 800.0, 500.0, &st);
        assert!(g.rows >= 1);
        let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(lit > 5000, "меню должно быть плотно нарисовано: {lit}");

        let mut pm2 = Pixmap::new(900, 600).unwrap();
        draw_help(
            &mut pm2,
            &fonts,
            &pal,
            &m,
            900.0,
            600.0,
            &[
                (Icon::Zoom, "W", "overview"),
                (Icon::Window, "M", "fit window"),
                (Icon::Help, "?", "help"),
                (Icon::Power, "ESC", "quit"),
            ],
        );
        let lit2 = pm2.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(lit2 > 5000, "справка должна рисоваться: {lit2}");
    }
}
