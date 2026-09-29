//! Процедурные иконки в стиле RIG/Signalis: тонкие линии и сегменты,
//! сетка 16×16, никаких растровых ассетов. Один вызов = один глиф.
//!
//! Приём из Dead Space: иконка — это НЕ пиктограмма-заливка, а чертёж
//! прибора. Поэтому везде хайрлайны, редкие точки-заклёпки и рамки.

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::widgets::hairline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Terminal,
    Browser,
    Files,
    Notes,
    Calc,
    Settings,
    Music,
    Camera,
    Lock,
    Power,
    Volume,
    Cpu,
    Ram,
    Battery,
    Cluster,
    Suspend,
    Zoom,
    Map,
    Help,
    Palette,
    Wifi,
    Window,
    Menu,
    Chevron,
}

impl Icon {
    pub fn name(&self) -> &'static str {
        match self {
            Icon::Terminal => "terminal",
            Icon::Browser => "browser",
            Icon::Files => "files",
            Icon::Notes => "notes",
            Icon::Calc => "calc",
            Icon::Settings => "settings",
            Icon::Music => "music",
            Icon::Camera => "camera",
            Icon::Lock => "lock",
            Icon::Power => "power",
            Icon::Volume => "volume",
            Icon::Cpu => "cpu",
            Icon::Ram => "ram",
            Icon::Battery => "battery",
            Icon::Cluster => "cluster",
            Icon::Suspend => "suspend",
            Icon::Zoom => "zoom",
            Icon::Map => "map",
            Icon::Help => "help",
            Icon::Palette => "palette",
            Icon::Wifi => "wifi",
            Icon::Window => "window",
            Icon::Menu => "menu",
            Icon::Chevron => "chevron",
        }
    }
}

/// Точка/заклёпка. Координаты и размер ОКРУГЛЯЕМ до целых пикселей:
/// дробный 1px-прямоугольник роняет tiny-skia (assert в scan/hairline_aa),
/// а для нашего монохрома целые пиксели и так правильнее.
fn dot(pm: &mut Pixmap, x: f32, y: f32, s: f32, c: [u8; 4]) {
    let size = s.round().max(1.0);
    let px = (x - size * 0.5).round();
    let py = (y - size * 0.5).round();
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    if let Some(r) = Rect::from_xywh(px, py, size, size) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
}

fn box_(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: [u8; 4], line: f32) {
    hairline(pm, x, y, x + w, y, c, line);
    hairline(pm, x + w, y, x + w, y + h, c, line);
    hairline(pm, x + w, y + h, x, y + h, c, line);
    hairline(pm, x, y + h, x, y, c, line);
}

/// Дуга/окружность полилинией (8..16 сегментов — для тонких линий хватает).
fn ring(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, c: [u8; 4], line: f32) {
    let segs = 14;
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=segs {
        let t = i as f32 / segs as f32;
        let a = a0 + (a1 - a0) * t;
        let x = cx + r * a.cos();
        let y = cy + r * a.sin();
        if let Some((px, py)) = prev {
            hairline(pm, px, py, x, y, c, line);
        }
        prev = Some((x, y));
    }
}

/// Нарисовать иконку в квадрате `size` с левым-верхним углом (x, y).
pub fn icon(pm: &mut Pixmap, kind: Icon, x: f32, y: f32, size: f32, c: [u8; 4]) {
    let u = size / 16.0; // шаг сетки 16×16
    let g = |v: f32| x + v * u;
    let v = |v: f32| y + v * u;
    let l = (u * 0.16).clamp(0.7, 1.6); // толщина линии
    let s = (u * 0.5).max(1.0); // «точка»

    // Многие иконки: рамка + внутренний рисунок.
    match kind {
        Icon::Terminal => {
            box_(pm, g(1.5), v(2.5), 13.0 * u, 11.0 * u, c, l);
            hairline(pm, g(4.0), v(6.0), g(6.5), v(8.0), c, l);
            hairline(pm, g(6.5), v(8.0), g(4.0), v(10.0), c, l);
            hairline(pm, g(8.5), v(10.8), g(12.0), v(10.8), c, l);
        }
        Icon::Browser => {
            ring(
                pm,
                g(8.0),
                v(8.0),
                5.4 * u,
                0.0,
                std::f32::consts::TAU,
                c,
                l,
            );
            hairline(pm, g(2.6), v(8.0), g(13.4), v(8.0), c, l);
            hairline(pm, g(8.0), v(2.6), g(8.0), v(13.4), c, l);
            ring(pm, g(8.0), v(8.0), 5.4 * u, 2.2, 4.1, c, l);
        }
        Icon::Files => {
            hairline(pm, g(2.0), v(4.0), g(6.0), v(4.0), c, l);
            hairline(pm, g(6.0), v(4.0), g(7.5), v(6.0), c, l);
            box_(pm, g(2.0), v(6.0), 12.0 * u, 8.0 * u, c, l);
        }
        Icon::Notes => {
            box_(pm, g(3.0), v(2.0), 10.0 * u, 12.0 * u, c, l);
            for i in 0..3 {
                let yy = g(0.0) + v(5.0) - g(0.0) + (i as f32) * 2.6 * u;
                hairline(pm, g(5.0), yy, g(11.0), yy, c, l);
            }
        }
        Icon::Calc => {
            box_(pm, g(3.0), v(2.0), 10.0 * u, 12.0 * u, c, l);
            hairline(pm, g(3.0), v(6.0), g(13.0), v(6.0), c, l);
            for r in 0..2 {
                for col in 0..3 {
                    dot(pm, g(5.0 + col as f32 * 3.0), v(9.0 + r as f32 * 3.0), s, c);
                }
            }
        }
        Icon::Settings => {
            ring(
                pm,
                g(8.0),
                v(8.0),
                4.6 * u,
                0.0,
                std::f32::consts::TAU,
                c,
                l,
            );
            ring(
                pm,
                g(8.0),
                v(8.0),
                1.8 * u,
                0.0,
                std::f32::consts::TAU,
                c,
                l,
            );
            for i in 0..4 {
                let a = std::f32::consts::FRAC_PI_4 + i as f32 * std::f32::consts::FRAC_PI_2;
                hairline(
                    pm,
                    g(8.0) + 4.6 * u * a.cos(),
                    v(8.0) + 4.6 * u * a.sin(),
                    g(8.0) + 7.2 * u * a.cos(),
                    v(8.0) + 7.2 * u * a.sin(),
                    c,
                    l,
                );
            }
        }
        Icon::Music => {
            hairline(pm, g(6.0), v(12.0), g(6.0), v(4.5), c, l);
            hairline(pm, g(6.0), v(4.5), g(12.0), v(3.0), c, l);
            hairline(pm, g(12.0), v(3.0), g(12.0), v(10.5), c, l);
            dot(pm, g(4.8), v(12.2), s * 2.0, c);
            dot(pm, g(10.8), v(10.7), s * 2.0, c);
        }
        Icon::Camera => {
            box_(pm, g(2.0), v(5.0), 12.0 * u, 8.0 * u, c, l);
            hairline(pm, g(5.5), v(5.0), g(7.0), v(3.0), c, l);
            hairline(pm, g(7.0), v(3.0), g(10.0), v(3.0), c, l);
            hairline(pm, g(10.0), v(3.0), g(11.0), v(5.0), c, l);
            ring(
                pm,
                g(8.0),
                v(9.0),
                2.4 * u,
                0.0,
                std::f32::consts::TAU,
                c,
                l,
            );
        }
        Icon::Lock => {
            box_(pm, g(4.0), v(8.0), 8.0 * u, 6.0 * u, c, l);
            ring(
                pm,
                g(8.0),
                v(8.0),
                3.0 * u,
                std::f32::consts::PI,
                std::f32::consts::TAU,
                c,
                l,
            );
            dot(pm, g(8.0), v(11.0), s, c);
        }
        Icon::Power => {
            ring(
                pm,
                g(8.0),
                v(9.0),
                4.6 * u,
                -std::f32::consts::FRAC_PI_3 - 0.35,
                std::f32::consts::FRAC_PI_3 + 3.6,
                c,
                l,
            );
            hairline(pm, g(8.0), v(2.0), g(8.0), v(8.0), c, l);
        }
        Icon::Volume => {
            hairline(pm, g(3.0), v(6.0), g(6.0), v(6.0), c, l);
            hairline(pm, g(6.0), v(6.0), g(9.5), v(3.0), c, l);
            hairline(pm, g(9.5), v(3.0), g(9.5), v(13.0), c, l);
            hairline(pm, g(9.5), v(13.0), g(6.0), v(10.0), c, l);
            hairline(pm, g(6.0), v(10.0), g(3.0), v(10.0), c, l);
            hairline(pm, g(3.0), v(10.0), g(3.0), v(6.0), c, l);
            ring(pm, g(9.5), v(8.0), 3.4 * u, -0.9, 0.9, c, l);
            ring(pm, g(9.5), v(8.0), 5.6 * u, -0.8, 0.8, c, l);
        }
        Icon::Cpu => {
            box_(pm, g(4.0), v(4.0), 8.0 * u, 8.0 * u, c, l);
            box_(pm, g(6.5), v(6.5), 3.0 * u, 3.0 * u, c, l);
            for i in 0..3 {
                let o = 6.0 + i as f32 * 2.0;
                hairline(pm, g(o), v(1.5), g(o), v(4.0), c, l);
                hairline(pm, g(o), v(12.0), g(o), v(14.5), c, l);
                hairline(pm, g(1.5), v(o), g(4.0), v(o), c, l);
                hairline(pm, g(12.0), v(o), g(14.5), v(o), c, l);
            }
        }
        Icon::Ram => {
            box_(pm, g(1.5), v(6.0), 13.0 * u, 5.0 * u, c, l);
            for i in 0..4 {
                let xx = 3.5 + i as f32 * 3.0;
                hairline(pm, g(xx), v(6.0), g(xx), v(11.0), c, l);
            }
            hairline(pm, g(6.0), v(11.0), g(6.0), v(13.0), c, l);
            hairline(pm, g(10.0), v(11.0), g(10.0), v(13.0), c, l);
        }
        Icon::Battery => {
            box_(pm, g(2.0), v(5.5), 11.0 * u, 5.0 * u, c, l);
            hairline(pm, g(13.0), v(7.0), g(14.5), v(7.0), c, l);
            hairline(pm, g(14.5), v(7.0), g(14.5), v(9.0), c, l);
            hairline(pm, g(14.5), v(9.0), g(13.0), v(9.0), c, l);
            let mut p = Paint::default();
            p.set_color_rgba8(c[0], c[1], c[2], c[3]);
            if let Some(r) = Rect::from_xywh(g(3.2), v(6.7), 4.4 * u, 2.6 * u) {
                pm.fill_rect(r, &p, Transform::identity(), None);
            }
        }
        Icon::Cluster => {
            box_(pm, g(2.0), v(4.0), 5.0 * u, 8.0 * u, c, l);
            box_(pm, g(8.0), v(4.0), 6.0 * u, 8.0 * u, c, l);
            hairline(pm, g(1.0), v(2.0), g(7.0), v(2.0), c, l);
            hairline(pm, g(1.0), v(2.0), g(1.0), v(4.0), c, l);
            hairline(pm, g(15.0), v(14.0), g(9.0), v(14.0), c, l);
            hairline(pm, g(15.0), v(14.0), g(15.0), v(12.0), c, l);
        }
        Icon::Suspend => {
            box_(pm, g(2.0), v(3.0), 12.0 * u, 10.0 * u, c, l);
            hairline(pm, g(6.0), v(6.0), g(6.0), v(10.0), c, l);
            hairline(pm, g(10.0), v(6.0), g(10.0), v(10.0), c, l);
        }
        Icon::Zoom => {
            ring(
                pm,
                g(7.0),
                v(7.0),
                4.2 * u,
                0.0,
                std::f32::consts::TAU,
                c,
                l,
            );
            hairline(pm, g(10.2), v(10.2), g(14.0), v(14.0), c, l);
            hairline(pm, g(5.0), v(7.0), g(9.0), v(7.0), c, l);
            hairline(pm, g(7.0), v(5.0), g(7.0), v(9.0), c, l);
        }
        Icon::Map => {
            box_(pm, g(2.0), v(3.0), 12.0 * u, 10.0 * u, c, l);
            dot(pm, g(5.5), v(7.0), s, c);
            dot(pm, g(9.5), v(5.5), s, c);
            dot(pm, g(11.0), v(10.0), s, c);
            box_(pm, g(7.0), v(7.0), 5.0 * u, 3.0 * u, c, l);
        }
        Icon::Help => {
            ring(
                pm,
                g(8.0),
                v(8.0),
                6.0 * u,
                0.0,
                std::f32::consts::TAU,
                c,
                l,
            );
            ring(pm, g(8.0), v(6.8), 2.0 * u, 2.9, 6.6, c, l);
            hairline(pm, g(8.0), v(8.6), g(8.0), v(10.0), c, l);
            dot(pm, g(8.0), v(11.8), s, c);
        }
        Icon::Palette => {
            // три наклонных штриха — «метка» Persona, но в нашем монохроме
            for i in 0..3 {
                let o = i as f32 * 4.0;
                hairline(pm, g(3.0 + o), v(12.0), g(6.5 + o), v(4.0), c, l * 1.6);
            }
        }
        Icon::Wifi => {
            ring(pm, g(8.0), v(11.0), 7.0 * u, 3.6, 5.9, c, l);
            ring(pm, g(8.0), v(11.0), 4.6 * u, 3.7, 5.8, c, l);
            ring(pm, g(8.0), v(11.0), 2.2 * u, 3.8, 5.7, c, l);
            dot(pm, g(8.0), v(12.4), s, c);
        }
        Icon::Window => {
            box_(pm, g(2.0), v(3.0), 12.0 * u, 10.0 * u, c, l);
            hairline(pm, g(2.0), v(6.0), g(14.0), v(6.0), c, l);
            dot(pm, g(4.0), v(4.5), s * 0.8, c);
            dot(pm, g(6.0), v(4.5), s * 0.8, c);
        }
        Icon::Menu => {
            for i in 0..3 {
                let yy = v(5.0 + i as f32 * 3.0);
                let wgt = 11.0 - i as f32 * 2.5;
                hairline(pm, g(2.5), yy, g(2.5 + wgt), yy, c, l);
            }
        }
        Icon::Chevron => {
            hairline(pm, g(6.0), v(4.0), g(10.5), v(8.0), c, l);
            hairline(pm, g(10.5), v(8.0), g(6.0), v(12.0), c, l);
        }
    }
}

/// Все иконки — для теста покрытия и меню.
pub const ALL: [Icon; 24] = [
    Icon::Terminal,
    Icon::Browser,
    Icon::Files,
    Icon::Notes,
    Icon::Calc,
    Icon::Settings,
    Icon::Music,
    Icon::Camera,
    Icon::Lock,
    Icon::Power,
    Icon::Volume,
    Icon::Cpu,
    Icon::Ram,
    Icon::Battery,
    Icon::Cluster,
    Icon::Suspend,
    Icon::Zoom,
    Icon::Map,
    Icon::Help,
    Icon::Palette,
    Icon::Wifi,
    Icon::Window,
    Icon::Menu,
    Icon::Chevron,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Mode, Palette};

    #[test]
    fn every_icon_draws_a_glyph_without_filling_the_box() {
        let pal = Palette::of(Mode::Rig);
        for kind in ALL {
            let mut pm = Pixmap::new(64, 64).unwrap();
            icon(&mut pm, kind, 8.0, 8.0, 48.0, pal.primary);
            let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
            assert!(lit > 25, "{:?}: иконка почти пустая ({lit})", kind);
            assert!(
                lit < (48 * 48) / 2,
                "{:?}: иконка залита ({lit}) — это не чертёж",
                kind
            );
        }
    }

    #[test]
    fn icons_are_deterministic_and_scale_safely() {
        let pal = Palette::of(Mode::Signalis);
        let mut a = Pixmap::new(32, 32).unwrap();
        let mut b = Pixmap::new(32, 32).unwrap();
        icon(&mut a, Icon::Cpu, 0.0, 0.0, 32.0, pal.primary);
        icon(&mut b, Icon::Cpu, 0.0, 0.0, 32.0, pal.primary);
        assert_eq!(a.data(), b.data());
        for size in [6.0, 12.0, 24.0, 96.0] {
            let mut pm = Pixmap::new(110, 110).unwrap();
            icon(&mut pm, Icon::Settings, 4.0, 4.0, size, pal.text);
        }
    }

    #[test]
    fn names_are_unique_and_non_empty() {
        let mut names: Vec<&str> = ALL.iter().map(|i| i.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "имена иконок должны быть уникальны");
        assert!(names.iter().all(|n| !n.is_empty()));
    }
}
