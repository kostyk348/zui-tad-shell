//! Палитры и метрики оболочки.
//!
//! Все режимы — МОНОХРОМ: один акцентный «фосфор» + производные яркости.
//! Второй цвет если и появляется, то как аварийный (тревога) или «холодный
//! сигнал» — и никогда как равноправный акцент.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Dead Space RIG: оранжевый сигнал по почти-чёрному.
    Rig,
    /// Signalis: тёплый амбер CRT, сильнее зерно и сканлайны.
    Signalis,
    /// Зелёный фосфорный терминал.
    Phosphor,
}

fn c(rgb: u32) -> [u8; 4] {
    [
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
        0xff,
    ]
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Почти-чёрный фон холста.
    pub bg: [u8; 4],
    /// Фон панели/OSD (чуть светлее холста, всё ещё тёмный).
    pub panel_bg: [u8; 4],
    /// Акцент: линии, значения, фокус.
    pub primary: [u8; 4],
    /// Приглушённый акцент: подписи, неактивные ячейки.
    pub dim: [u8; 4],
    /// Основной текст.
    pub text: [u8; 4],
    /// Аварийный (тревога, перегрев, красная зона).
    pub alert: [u8; 4],
    /// Холодный сигнал (редкий второй цвет).
    pub cold: [u8; 4],
    /// Сила сканлайнов 0..1.
    pub scanline: f32,
    /// Сила зерна 0..1.
    pub grain: f32,
    /// Сила хроматической аберрации в пикселях (0..2).
    pub chroma: f32,
    /// Виньетка 0..1.
    pub vignette: f32,
}

impl Palette {
    pub fn of(mode: Mode) -> Self {
        match mode {
            Mode::Rig => Self {
                bg: c(0x060709),
                panel_bg: c(0x0b0d11),
                primary: c(0xff7a18),
                dim: c(0x7a3a12),
                text: c(0xe8d9c8),
                alert: c(0xff2f2f),
                cold: c(0x7fe9ff),
                scanline: 0.10,
                grain: 0.03,
                chroma: 1.0,
                vignette: 0.35,
            },
            Mode::Signalis => Self {
                bg: c(0x0a0906),
                panel_bg: c(0x120f08),
                primary: c(0xffb64a),
                dim: c(0x6e4a18),
                text: c(0xf2e3bf),
                alert: c(0xff4d3d),
                cold: c(0x9fd8ff),
                scanline: 0.16,
                grain: 0.06,
                chroma: 0.7,
                vignette: 0.45,
            },
            Mode::Phosphor => Self {
                bg: c(0x040704),
                panel_bg: c(0x081008),
                primary: c(0x7cf2a0),
                dim: c(0x2f6b43),
                text: c(0xcfe9d6),
                alert: c(0xff5f5f),
                cold: c(0x8fd4ff),
                scanline: 0.12,
                grain: 0.04,
                chroma: 0.6,
                vignette: 0.35,
            },
        }
    }

    /// Производная яркость акцента: k = 0 → фон, k = 1 → primary.
    pub fn accent_lerp(&self, k: f32) -> [u8; 4] {
        let k = k.clamp(0.0, 1.0);
        [
            (self.bg[0] as f32 + (self.primary[0] as f32 - self.bg[0] as f32) * k) as u8,
            (self.bg[1] as f32 + (self.primary[1] as f32 - self.bg[1] as f32) * k) as u8,
            (self.bg[2] as f32 + (self.primary[2] as f32 - self.bg[2] as f32) * k) as u8,
            0xff,
        ]
    }

    /// Ячейка сегментного индикатора: on → акцент, off → тусклая рамка.
    pub fn cell(&self, on: bool) -> [u8; 4] {
        if on {
            self.primary
        } else {
            self.dim
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    /// Высота верхней панели.
    pub panel_h: f32,
    /// Толщина всех линий (хайрлайн).
    pub line: f32,
    /// Разрядка букв в подписях (px).
    pub tracking: f32,
    /// Базовый размер подписи.
    pub label_size: f32,
    /// Размер значения.
    pub value_size: f32,
    /// Высота блока сегментного индикатора.
    pub cell_h: f32,
    /// Ширина блока сегментного индикатора.
    pub cell_w: f32,
    /// Отступ от краёв.
    pub pad: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            panel_h: 26.0,
            line: 1.0,
            tracking: 1.8,
            label_size: 10.0,
            value_size: 13.0,
            cell_h: 9.0,
            cell_w: 3.0,
            pad: 12.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_is_monochrome_by_construction() {
        // акцент должен быть заметно ярче тусклого по всем каналам суммы
        for m in [Mode::Rig, Mode::Signalis, Mode::Phosphor] {
            let p = Palette::of(m);
            let sum = |c: [u8; 4]| c[0] as u32 + c[1] as u32 + c[2] as u32;
            assert!(
                sum(p.primary) > sum(p.dim),
                "{m:?}: accent must outshine dim"
            );
            assert!(sum(p.bg) < sum(p.dim), "{m:?}: bg must be darkest");
            assert_eq!(p.bg[3], 0xff);
        }
    }

    #[test]
    fn accent_lerp_endpoints_and_midpoint() {
        let p = Palette::of(Mode::Rig);
        assert_eq!(p.accent_lerp(0.0), p.bg);
        assert_eq!(p.accent_lerp(1.0), p.primary);
        let mid = p.accent_lerp(0.5);
        assert!(mid[0] > p.bg[0] && mid[0] < p.primary[0]);
    }

    #[test]
    fn accent_lerp_clamps_out_of_range() {
        let p = Palette::of(Mode::Signalis);
        assert_eq!(p.accent_lerp(-5.0), p.bg);
        assert_eq!(p.accent_lerp(9.0), p.primary);
    }

    #[test]
    fn cell_helper_switches_color() {
        let p = Palette::of(Mode::Phosphor);
        assert_eq!(p.cell(true), p.primary);
        assert_eq!(p.cell(false), p.dim);
    }
}
