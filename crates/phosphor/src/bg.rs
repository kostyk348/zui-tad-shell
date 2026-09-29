//! Процедурные обои — своя графика в стиле RIG/Signalis, без чужих ассетов.
//!
//! Почему не «ассеты из игр»: текстуры/логотипы Dead Space и Signalis —
//! чужая интеллектуальная собственность, их нельзя класть в публичный
//! репозиторий. Поэтому фон рисуется кодом (и его можно править), а свою
//! картинку пользователь подключает через `ZUI_WALLPAPER=/path.png`.
//!
//! Все генераторы детерминированы по seed и рисуются только хайрлайнами,
//! точками и нашими текстурами — вид один и тот же на любом разрешении.

use tiny_skia::Pixmap;

use crate::texture::{texture, TexKind};
use crate::theme::Palette;
use crate::widgets::hairline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgKind {
    /// Обшивка корабля: панельная сетка, заклёпки, маркировка.
    Hull,
    /// Глубокий космос: пыль, звёзды, туманности.
    Starfield,
    /// Кинескоп: полутон, полосы, зерно, «текст» на фоне.
    Crt,
    /// Чертёж: миллиметровка, насечки, секторные подписи, перекрестье.
    Blueprint,
}

impl BgKind {
    pub fn name(&self) -> &'static str {
        match self {
            BgKind::Hull => "hull",
            BgKind::Starfield => "starfield",
            BgKind::Crt => "crt",
            BgKind::Blueprint => "blueprint",
        }
    }
    pub fn all() -> [BgKind; 4] {
        [
            BgKind::Hull,
            BgKind::Starfield,
            BgKind::Crt,
            BgKind::Blueprint,
        ]
    }
}

fn fill(pm: &mut Pixmap, color: [u8; 4]) {
    let mut p = tiny_skia::Paint::default();
    p.set_color_rgba8(color[0], color[1], color[2], color[3]);
    if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, pm.width() as f32, pm.height() as f32) {
        pm.fill_rect(r, &p, tiny_skia::Transform::identity(), None);
    }
}

fn speck(pm: &mut Pixmap, x: f32, y: f32, color: [u8; 4]) {
    let mut p = tiny_skia::Paint::default();
    p.set_color_rgba8(color[0], color[1], color[2], color[3]);
    if let Some(r) = tiny_skia::Rect::from_xywh(x.round(), y.round(), 1.0, 1.0) {
        pm.fill_rect(r, &p, tiny_skia::Transform::identity(), None);
    }
}

/// Простой детерминированный генератор.
struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self {
        Rng(seed.wrapping_mul(2_654_435_761).wrapping_add(1))
    }
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.0 >> 16) & 0xffff) as f32 / 65535.0
    }
}

/// Сгенерировать фон. Размеры лучше брать кратными 120 (сетка генераторов).
pub fn render(kind: BgKind, w: u32, h: u32, pal: &Palette, seed: u32) -> Pixmap {
    let mut pm = Pixmap::new(w.max(16), h.max(16)).unwrap();
    let (fw, fh) = (pm.width() as f32, pm.height() as f32);
    // Обои должны быть видны: база чуть светлее фона шелла, линии — плотнее.
    let mix = |a: [u8; 4], b: [u8; 4], k: f32| -> [u8; 4] {
        [
            (a[0] as f32 + (b[0] as f32 - a[0] as f32) * k) as u8,
            (a[1] as f32 + (b[1] as f32 - a[1] as f32) * k) as u8,
            (a[2] as f32 + (b[2] as f32 - a[2] as f32) * k) as u8,
            0xff,
        ]
    };
    fill(&mut pm, mix(pal.bg, pal.panel_bg, 0.85));
    let mut rng = Rng::new(seed);
    let dim = pal.dim;
    let faint = [dim[0], dim[1], dim[2], 0x8a];
    let mid = [pal.primary[0], pal.primary[1], pal.primary[2], 0xa8];

    match kind {
        BgKind::Hull => {
            // Панельная сетка: главный рисунок фона, поэтому читаемая, а не «намёк».
            let step = 240.0;
            let mut x = 0.0;
            while x <= fw {
                hairline(&mut pm, x, 0.0, x, fh, [dim[0], dim[1], dim[2], 0xb0], 1.0);
                x += step;
            }
            let mut y = 0.0;
            while y <= fh {
                hairline(&mut pm, 0.0, y, fw, y, [dim[0], dim[1], dim[2], 0xb0], 1.0);
                y += step;
            }
            // Внутри каждой панели: скобки по углам, иногда штриховка/заклёпки,
            // снизу — «маркировка» (штрихи, издалека похожие на текст).
            let mut py = 0.0;
            while py < fh {
                let mut px = 0.0;
                while px < fw {
                    let d = 10.0;
                    let br = [dim[0], dim[1], dim[2], 0x99];
                    // угловые скобки
                    hairline(&mut pm, px + 4.0, py + 4.0, px + 4.0 + d, py + 4.0, br, 1.0);
                    hairline(&mut pm, px + 4.0, py + 4.0, px + 4.0, py + 4.0 + d, br, 1.0);
                    hairline(
                        &mut pm,
                        px + step - 4.0,
                        py + step - 4.0,
                        px + step - 4.0 - d,
                        py + step - 4.0,
                        br,
                        1.0,
                    );
                    hairline(
                        &mut pm,
                        px + step - 4.0,
                        py + step - 4.0,
                        px + step - 4.0,
                        py + step - 4.0 - d,
                        br,
                        1.0,
                    );

                    let roll = rng.next();
                    if roll > 0.72 {
                        texture(
                            &mut pm,
                            (px + 12.0, py + 12.0, step - 24.0, step - 24.0),
                            TexKind::Hatch,
                            [mid[0], mid[1], mid[2], 0x40],
                            14.0,
                            seed,
                        );
                    } else if roll > 0.55 {
                        let mut bx = px + 24.0;
                        let mut by = py + 24.0;
                        while by < py + step - 20.0 {
                            texture(&mut pm, (bx, by, 8.0, 8.0), TexKind::Rivets, br, 4.0, seed);
                            bx += 64.0;
                            if bx > px + step - 30.0 {
                                bx = px + 24.0;
                                by += 64.0;
                            }
                        }
                    }
                    // маркировка
                    if rng.next() > 0.5 {
                        let bx = px + 20.0;
                        let by = py + step - 18.0;
                        let mut dx = 0.0;
                        let n = 3 + (rng.next() * 5.0) as i32;
                        for _ in 0..n {
                            let w = 8.0 + rng.next() * 14.0;
                            hairline(
                                &mut pm,
                                bx + dx,
                                by,
                                bx + dx + w,
                                by,
                                [mid[0], mid[1], mid[2], 0xc0],
                                1.0,
                            );
                            dx += w + 5.0;
                        }
                    }
                    px += step;
                }
                py += step;
            }
            // Центральная «структура» + перекрестье: взгляд должен цепляться.
            let cxm = fw * 0.5;
            let cym = fh * 0.5;
            for k in 0..7 {
                let r = 110.0 + k as f32 * 52.0;
                let a = (0xd0 - k * 18).max(0x40) as u8;
                let c = [pal.primary[0], pal.primary[1], pal.primary[2], a];
                hairline(
                    &mut pm,
                    cxm - r,
                    cym - r * 0.62,
                    cxm + r,
                    cym - r * 0.62,
                    c,
                    1.0,
                );
                hairline(
                    &mut pm,
                    cxm - r,
                    cym + r * 0.62,
                    cxm + r,
                    cym + r * 0.62,
                    c,
                    1.0,
                );
                hairline(
                    &mut pm,
                    cxm - r,
                    cym - r * 0.62,
                    cxm - r,
                    cym + r * 0.62,
                    c,
                    1.0,
                );
                hairline(
                    &mut pm,
                    cxm + r,
                    cym - r * 0.62,
                    cxm + r,
                    cym + r * 0.62,
                    c,
                    1.0,
                );
            }
            hairline(
                &mut pm,
                cxm - 420.0,
                cym,
                cxm + 420.0,
                cym,
                [pal.primary[0], pal.primary[1], pal.primary[2], 0x30],
                1.0,
            );
            hairline(
                &mut pm,
                cxm,
                cym - 320.0,
                cxm,
                cym + 320.0,
                [pal.primary[0], pal.primary[1], pal.primary[2], 0x30],
                1.0,
            );
            // Полосы развёртки — деликатно, а не «вся картинка в линиях».
            texture(
                &mut pm,
                (0.0, 0.0, fw, fh),
                TexKind::Bands,
                [dim[0], dim[1], dim[2], 0x20],
                5.0,
                seed ^ 3,
            );
        }
        BgKind::Starfield => {
            // туманности
            for _ in 0..7 {
                let nx = rng.next() * fw;
                let ny = rng.next() * fh;
                let nw = 260.0 + rng.next() * 520.0;
                let nh = 160.0 + rng.next() * 360.0;
                let use_cold = rng.next() > 0.6;
                let c = if use_cold { pal.cold } else { pal.primary };
                let a = (26.0 + rng.next() * 30.0) as u8;
                texture(
                    &mut pm,
                    (nx, ny, nw, nh),
                    TexKind::Halftone,
                    [c[0], c[1], c[2], a],
                    11.0,
                    seed,
                );
            }
            // звёзды (с заворотом на краях, чтобы фон смотрелся бесшовно)
            for _ in 0..900 {
                let sx = rng.next() * fw;
                let sy = rng.next() * fh;
                let bright = rng.next();
                let a = (90.0 + bright * 165.0) as u8;
                let c = if bright > 0.94 {
                    [255, 255, 255, a]
                } else if bright > 0.8 {
                    [pal.cold[0], pal.cold[1], pal.cold[2], a]
                } else {
                    [pal.text[0], pal.text[1], pal.text[2], a]
                };
                speck(&mut pm, sx, sy, c);
                // заворот
                if sx < 2.0 {
                    speck(&mut pm, sx + fw, sy, c);
                }
                if sy < 2.0 {
                    speck(&mut pm, sx, sy + fh, c);
                }
                if sx > fw - 2.0 {
                    speck(&mut pm, sx - fw, sy, c);
                }
                if sy > fh - 2.0 {
                    speck(&mut pm, sx, sy - fh, c);
                }
            }
            texture(
                &mut pm,
                (0.0, 0.0, fw, fh),
                TexKind::Bands,
                [dim[0], dim[1], dim[2], 0x22],
                7.0,
                seed ^ 5,
            );
        }
        BgKind::Crt => {
            texture(
                &mut pm,
                (0.0, 0.0, fw, fh),
                TexKind::Halftone,
                [dim[0], dim[1], dim[2], 0x90],
                4.0,
                seed,
            );
            texture(
                &mut pm,
                (0.0, 0.0, fw, fh),
                TexKind::Bands,
                [dim[0], dim[1], dim[2], 0x70],
                3.0,
                seed ^ 1,
            );
            texture(
                &mut pm,
                (0.0, 0.0, fw, fh),
                TexKind::Worn,
                [dim[0], dim[1], dim[2], 0xc0],
                3.0,
                seed ^ 2,
            );
            // «терминал на фоне»: блоки поддельного текста
            let mut y = 60.0;
            let mut rows = 0;
            while y < fh - 40.0 && rows < 26 {
                let mut x = 40.0;
                let n = 2 + (rng.next() * 6.0) as i32;
                for _ in 0..n {
                    let w = 30.0 + rng.next() * 180.0;
                    let a = (70.0 + rng.next() * 90.0) as u8;
                    hairline(&mut pm, x, y, x + w, y, [dim[0], dim[1], dim[2], a], 1.0);
                    x += w + 18.0;
                    if x > fw - 60.0 {
                        break;
                    }
                }
                y += 26.0;
                rows += 1;
            }
            // большая «рамка кадра»
            hairline(&mut pm, 24.0, 24.0, fw - 24.0, 24.0, mid, 1.0);
            hairline(&mut pm, 24.0, fh - 24.0, fw - 24.0, fh - 24.0, mid, 1.0);
            hairline(&mut pm, 24.0, 24.0, 24.0, fh - 24.0, mid, 1.0);
            hairline(&mut pm, fw - 24.0, 24.0, fw - 24.0, fh - 24.0, mid, 1.0);
        }
        BgKind::Blueprint => {
            // миллиметровка: минор каждые 20, мажор каждые 100
            let mut x = 0.0;
            while x <= fw {
                let major = ((x / 20.0) as i32) % 5 == 0;
                hairline(
                    &mut pm,
                    x,
                    0.0,
                    x,
                    fh,
                    if major {
                        faint
                    } else {
                        [dim[0], dim[1], dim[2], 0x4a]
                    },
                    1.0,
                );
                x += 20.0;
            }
            let mut y = 0.0;
            while y <= fh {
                let major = ((y / 20.0) as i32) % 5 == 0;
                hairline(
                    &mut pm,
                    0.0,
                    y,
                    fw,
                    y,
                    if major {
                        faint
                    } else {
                        [dim[0], dim[1], dim[2], 0x4a]
                    },
                    1.0,
                );
                y += 20.0;
            }
            // насечки по краям
            let mut t = 0.0;
            while t <= fw {
                let long = ((t / 100.0) as i32) % 2 == 0;
                let len = if long { 12.0 } else { 6.0 };
                hairline(&mut pm, t, 0.0, t, 0.0 + len, mid, 1.0);
                hairline(&mut pm, t, fh, t, fh - len, mid, 1.0);
                t += 100.0;
            }
            // перекрестье и окружность допуска
            let cxm = fw * 0.5;
            let cym = fh * 0.5;
            hairline(&mut pm, cxm - 120.0, cym, cxm + 120.0, cym, mid, 1.0);
            hairline(&mut pm, cxm, cym - 120.0, cxm, cym + 120.0, mid, 1.0);
            let mut a = 0.0;
            let mut prev: Option<(f32, f32)> = None;
            while a <= std::f32::consts::TAU + 0.01 {
                let (sx, sy) = (cxm + 200.0 * a.cos(), cym + 200.0 * a.sin());
                if let Some((px, py)) = prev {
                    hairline(&mut pm, px, py, sx, sy, faint, 1.0);
                }
                prev = Some((sx, sy));
                a += 0.12;
            }
            // «секторные подписи»
            for i in 0..5 {
                let bx = 60.0 + i as f32 * 260.0;
                let by = fh - 46.0;
                hairline(&mut pm, bx, by, bx + 90.0, by, mid, 1.0);
                hairline(&mut pm, bx, by - 12.0, bx + 40.0, by - 12.0, faint, 1.0);
            }
            texture(
                &mut pm,
                (0.0, 0.0, fw, fh),
                TexKind::Worn,
                [dim[0], dim[1], dim[2], 0x80],
                3.0,
                seed ^ 7,
            );
        }
    }

    pm
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Mode, Palette};

    fn lit(pm: &Pixmap) -> usize {
        pm.pixels().iter().filter(|p| p.alpha() > 0).count()
    }

    #[test]
    fn every_kind_renders_a_full_opaque_backdrop() {
        let pal = Palette::of(Mode::Signalis);
        for k in BgKind::all() {
            let pm = render(k, 480, 240, &pal, 7);
            assert_eq!(pm.width(), 480);
            assert_eq!(lit(&pm), 480 * 240, "{:?}: фон должен быть непрозрачным", k);
        }
    }

    #[test]
    fn kinds_differ_from_each_other() {
        let pal = Palette::of(Mode::Rig);
        let a = render(BgKind::Hull, 240, 240, &pal, 1);
        let b = render(BgKind::Starfield, 240, 240, &pal, 1);
        let c = render(BgKind::Blueprint, 240, 240, &pal, 1);
        assert_ne!(a.data(), b.data());
        assert_ne!(b.data(), c.data());
        assert_ne!(a.data(), c.data());
    }

    #[test]
    fn deterministic_by_seed() {
        let pal = Palette::of(Mode::Phosphor);
        let a = render(BgKind::Starfield, 320, 200, &pal, 42);
        let b = render(BgKind::Starfield, 320, 200, &pal, 42);
        let c = render(BgKind::Starfield, 320, 200, &pal, 43);
        assert_eq!(a.data(), b.data());
        assert_ne!(a.data(), c.data());
    }

    #[test]
    fn tiny_and_odd_sizes_do_not_panic() {
        let pal = Palette::of(Mode::Rig);
        for (w, h) in [(16, 16), (17, 19), (120, 120), (37, 400)] {
            for k in BgKind::all() {
                let _ = render(k, w, h, &pal, 3);
            }
        }
    }

    #[test]
    fn backdrop_is_not_flat_noise() {
        // фон должен содержать структуру: заметная доля «непустых» пикселей,
        // но не быть кашей из белого шума
        let pal = Palette::of(Mode::Signalis);
        for k in BgKind::all() {
            let pm = render(k, 400, 300, &pal, 11);
            let distinct = {
                let mut set = std::collections::BTreeSet::new();
                for p in pm.pixels() {
                    let c = p.demultiply();
                    set.insert((c.red() / 8, c.green() / 8, c.blue() / 8));
                }
                set.len()
            };
            assert!(
                distinct >= 4,
                "{:?}: фон слишком плоский ({distinct} тонов)",
                k
            );
        }
    }
}
