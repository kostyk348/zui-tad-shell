//! Процедурные текстуры оболочки: штриховка, полутон, «потёртый металл»,
//! полосы развёртки, заклёпки. Ноль ассетов — всё считается из seed,
//! поэтому одинаковый seed даёт одинаковую текстуру (тесты это фиксируют).
//!
//! Зачем: ровные заливки читаются как «веб-страница». У Dead Space RIG и
//! Signalis поверхность всегда чем-то неоднородна — точками, штрихом, грязью.

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::widgets::hairline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexKind {
    /// Диагональный штрих под 45°.
    Hatch,
    /// Полутоновая сетка точек.
    Halftone,
    /// Потёртость: редкие зёрна + тёмные пятна (детерминированные).
    Worn,
    /// Горизонтальные полосы (как строки на кинескопе).
    Bands,
    /// Заклёпки по углам (техпанель RIG).
    Rivets,
}

/// Прямоугольник текстуры. Округляем к целым пикселям: дробные 1px-прямоугольники
/// падают в tiny-skia (assert в scan/hairline_aa).
fn fill(pm: &mut Pixmap, rect: (f32, f32, f32, f32), color: [u8; 4]) {
    let mut p = Paint::default();
    p.set_color_rgba8(color[0], color[1], color[2], color[3]);
    let (x, y) = (rect.0.round(), rect.1.round());
    let (w, h) = (rect.2.round().max(1.0), rect.3.round().max(1.0));
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
}

/// Пиксель (целый) — для полутона и зёрен.
/// Квадратная заклёпка (целые пиксели).
fn dot_sq(pm: &mut Pixmap, x: f32, y: f32, size: f32, color: [u8; 4]) {
    let s = size.round().max(1.0);
    let mut p = Paint::default();
    p.set_color_rgba8(color[0], color[1], color[2], color[3]);
    if let Some(r) = Rect::from_xywh((x - s * 0.5).round(), (y - s * 0.5).round(), s, s) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
}

fn speck(pm: &mut Pixmap, x: f32, y: f32, color: [u8; 4]) {
    let mut p = Paint::default();
    p.set_color_rgba8(color[0], color[1], color[2], color[3]);
    if let Some(r) = Rect::from_xywh(x.round(), y.round(), 1.0, 1.0) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
}

/// Нанести текстуру внутрь прямоугольника.
pub fn texture(
    pm: &mut Pixmap,
    rect: (f32, f32, f32, f32),
    kind: TexKind,
    color: [u8; 4],
    step: f32,
    seed: u32,
) {
    let (x, y, w, h) = rect;
    if w <= 1.0 || h <= 1.0 {
        return;
    }
    let step = step.max(2.0);
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut rnd = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state >> 16) as f32 / 65535.0
    };

    match kind {
        TexKind::Hatch => {
            let mut d = -h;
            while d < w {
                hairline(pm, x + d, y + h, x + d + h, y, color, 1.0);
                d += step;
            }
        }
        TexKind::Halftone => {
            let mut yy = y + step * 0.5;
            while yy < y + h {
                let mut xx = x + step * 0.5;
                while xx < x + w {
                    speck(pm, xx, yy, [color[0], color[1], color[2], 0x50]);
                    xx += step;
                }
                yy += step;
            }
        }
        TexKind::Worn => {
            // пятна
            let patches = ((w * h) / 4000.0).clamp(3.0, 24.0) as usize;
            for _ in 0..patches {
                let px = x + rnd() * w;
                let py = y + rnd() * h;
                let pw = 6.0 + rnd() * 26.0;
                let ph = 3.0 + rnd() * 10.0;
                let a = (color[3] as f32 * (0.10 + rnd() * 0.18)) as u8;
                fill(pm, (px, py, pw, ph), [color[0], color[1], color[2], a]);
            }
            // зёрна
            let specks = ((w * h) / 220.0).clamp(20.0, 900.0) as usize;
            for _ in 0..specks {
                let px = x + rnd() * w;
                let py = y + rnd() * h;
                let a = (color[3] as f32 * (0.25 + rnd() * 0.5)) as u8;
                speck(pm, px, py, [color[0], color[1], color[2], a]);
            }
        }
        TexKind::Bands => {
            let mut yy = y;
            let mut i = 0u32;
            while yy < y + h {
                if i % 2 == 1 {
                    let a = (color[3] as f32 * 0.16) as u8;
                    fill(
                        pm,
                        (x, yy, w, (step * 0.5).max(1.0)),
                        [color[0], color[1], color[2], a],
                    );
                }
                yy += step.max(3.0);
                i += 1;
            }
        }
        TexKind::Rivets => {
            let d = 4.0;
            for (rx, ry) in [
                (x + d, y + d),
                (x + w - d, y + d),
                (x + d, y + h - d),
                (x + w - d, y + h - d),
            ] {
                dot_sq(pm, rx, ry, 3.0, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Mode, Palette};

    fn pm() -> Pixmap {
        Pixmap::new(120, 80).unwrap()
    }

    #[test]
    fn every_kind_draws_something_but_not_everything() {
        let pal = Palette::of(Mode::Signalis);
        for kind in [
            TexKind::Hatch,
            TexKind::Halftone,
            TexKind::Worn,
            TexKind::Bands,
            TexKind::Rivets,
        ] {
            let mut p = pm();
            texture(&mut p, (0.0, 0.0, 120.0, 80.0), kind, pal.primary, 6.0, 42);
            let lit = p.pixels().iter().filter(|q| q.alpha() > 0).count();
            assert!(lit > 20, "{kind:?}: почти ничего не нарисовано ({lit})");
            assert!(
                lit < 120 * 80,
                "{kind:?}: залило всё подряд ({lit}) — это уже не текстура"
            );
        }
    }

    #[test]
    fn textures_are_deterministic_by_seed() {
        let pal = Palette::of(Mode::Rig);
        let mut a = pm();
        let mut b = pm();
        let mut c = pm();
        texture(
            &mut a,
            (0.0, 0.0, 120.0, 80.0),
            TexKind::Worn,
            pal.dim,
            5.0,
            7,
        );
        texture(
            &mut b,
            (0.0, 0.0, 120.0, 80.0),
            TexKind::Worn,
            pal.dim,
            5.0,
            7,
        );
        texture(
            &mut c,
            (0.0, 0.0, 120.0, 80.0),
            TexKind::Worn,
            pal.dim,
            5.0,
            8,
        );
        assert_eq!(a.data(), b.data(), "тот же seed → та же текстура");
        assert_ne!(a.data(), c.data(), "другой seed → другая грязь");
    }

    #[test]
    fn degenerate_rects_and_steps_do_not_panic() {
        let pal = Palette::of(Mode::Phosphor);
        for rect in [
            (0.0, 0.0, 1.0, 1.0),
            (0.0, 0.0, 0.0, 40.0),
            (5.0, 5.0, 2.0, 2.0),
        ] {
            for kind in [
                TexKind::Hatch,
                TexKind::Halftone,
                TexKind::Worn,
                TexKind::Bands,
                TexKind::Rivets,
            ] {
                let mut p = pm();
                texture(&mut p, rect, kind, pal.primary, 0.0, 1);
                texture(&mut p, rect, kind, pal.primary, 1.0, 1);
            }
        }
    }

    #[test]
    fn rivets_land_in_corners() {
        let pal = Palette::of(Mode::Rig);
        let mut p = Pixmap::new(40, 30).unwrap();
        texture(
            &mut p,
            (0.0, 0.0, 40.0, 30.0),
            TexKind::Rivets,
            pal.primary,
            4.0,
            0,
        );
        let corner = p.pixels()[(4 * 40 + 4) as usize].alpha() > 0;
        let center = p.pixels()[(15 * 40 + 20) as usize].alpha() > 0;
        assert!(corner, "заклёпка есть в углу");
        assert!(!center, "в центре заклёпок нет");
    }
}
