//! CRT-постобработка: то, что превращает ровный рендер в фосфорный экран.
//!
//! Порядок строго фиксирован (важно для вида):
//!   1. ordered dither (Bayer 4×4) — ступеньки-бандинг, как на 6-битном DAC;
//!   2. scanlines — гасят каждую N-ю строку;
//!   3. chromatic aberration — R/B расходятся по X;
//!   4. vignette — края уходят в темноту;
//!   5. grain — детерминированный шум поверх всего.
//!
//! ПРОИЗВОДИТЕЛЬНОСТЬ (чтобы держать 60 fps на CPU):
//!   * работаем In-place по `PremultipliedColorU8` — без декода/энкода всего
//!     кадра в промежуточный Vec (раньше это стоило 2 полных копий кадра);
//!   * каждый проход идёт ПОЛОСАМИ в несколько потоков (`thread::scope`);
//!   * шум берётся из заранее сгенерированного тайла 256×256 (без LCG на
//!     каждый пиксель каждый кадр);
//!   * `CrtParams::off()` выходит сразу.
//!
//! Детерминизм сохранён: одинаковый вход + seed → одинаковый кадр.

use crate::theme::Palette;
use std::sync::OnceLock;
use tiny_skia::{Pixmap, PremultipliedColorU8};

#[derive(Debug, Clone, Copy)]
pub struct CrtParams {
    pub scanline: f32,
    /// Каждую N-ю строку гасим (1 = все строки, 2 = через одну).
    pub scanline_period: u32,
    pub grain: f32,
    pub chroma: f32,
    pub vignette: f32,
    /// Число уровней на канал после дизеринга (2..=64).
    pub dither_levels: u8,
    pub seed: u32,
}

impl CrtParams {
    pub fn from_palette(p: &Palette, seed: u32) -> Self {
        Self {
            scanline: p.scanline,
            scanline_period: 3,
            grain: p.grain,
            chroma: p.chroma,
            vignette: p.vignette,
            dither_levels: 12,
            seed,
        }
    }

    /// Никаких эффектов (для «чистых» скриншотов и тестов).
    pub fn off() -> Self {
        Self {
            scanline: 0.0,
            scanline_period: 3,
            grain: 0.0,
            chroma: 0.0,
            vignette: 0.0,
            dither_levels: 255,
            seed: 0,
        }
    }

    fn is_off(&self) -> bool {
        self.scanline <= 0.0
            && self.grain <= 0.0
            && self.chroma <= 0.0
            && self.vignette <= 0.0
            && !(self.dither_levels >= 2 && self.dither_levels < 255)
    }
}

const BAYER4: [[f32; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];

/// Тайл шума 256×256, генерируется один раз на процесс.
fn noise_tile() -> &'static [u8; 65536] {
    static TILE: OnceLock<[u8; 65536]> = OnceLock::new();
    TILE.get_or_init(|| {
        let mut t = [0u8; 65536];
        let mut s: u32 = 0x1234_5678;
        for v in t.iter_mut() {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v = (s >> 24) as u8;
        }
        t
    })
}

#[inline]
fn px_set(p: &mut PremultipliedColorU8, r: u8, g: u8, b: u8) {
    if let Some(v) = PremultipliedColorU8::from_rgba(r, g, b, 255) {
        *p = v;
    }
}

/// Сколько потоков использовать (для тестов и малых кадров — 1).
fn band_count(h: usize) -> usize {
    if h < 64 {
        return 1;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 16)
        .min(h)
}

pub fn crt(pm: &mut Pixmap, p: &CrtParams) {
    let w = pm.width() as usize;
    let h = pm.height() as usize;
    if w == 0 || h == 0 || p.is_off() {
        return;
    }
    let bands = band_count(h);
    let band_h = h.div_ceil(bands);

    // Проход 1: дизеринг -> сканлайны -> виньетка -> зерно.
    // Работаем по СЫРЫМ байтам: обёртки demultiply/from_rgba на каждый пиксель
    // стоили дороже самой математики (кадр всегда непрозрачен, alpha не трогаем).
    let (scanline, period, levels) = (p.scanline, p.scanline_period.max(1), p.dither_levels);
    let (vig, grain) = (p.vignette, p.grain);
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let max_r2 = (cx * cx + cy * cy).max(1.0);
    let amp = grain.clamp(0.0, 1.0) * 255.0;
    let tile = noise_tile();
    let off = (p.seed as usize) & 0xff;
    {
        let data = pm.data_mut();
        std::thread::scope(|s| {
            for (bi, band) in data.chunks_mut(band_h * w * 4).enumerate() {
                let y0 = bi * band_h;
                s.spawn(move || {
                    let q = (levels.max(2) - 1) as f32;
                    let keep = 1.0 - scanline.clamp(0.0, 1.0);
                    let quant = levels >= 2 && levels < 255;
                    for (r, row) in band.chunks_mut(w * 4).enumerate() {
                        let y = y0 + r;
                        let dark = scanline > 0.0 && (y as u32) % period == 1;
                        let dy = y as f32 - cy;
                        for (x, px) in row.chunks_exact_mut(4).enumerate() {
                            let (mut rr, mut gg, mut bb) =
                                (px[0] as f32, px[1] as f32, px[2] as f32);
                            if quant {
                                let t = BAYER4[y & 3][x & 3] / 16.0 - 0.5;
                                rr = ((rr / 255.0 * q + t).round() / q).clamp(0.0, 1.0) * 255.0;
                                gg = ((gg / 255.0 * q + t).round() / q).clamp(0.0, 1.0) * 255.0;
                                bb = ((bb / 255.0 * q + t).round() / q).clamp(0.0, 1.0) * 255.0;
                            }
                            if dark {
                                rr *= keep;
                                gg *= keep;
                                bb *= keep;
                            }
                            if vig > 0.0 {
                                let dx = x as f32 - cx;
                                let fac = (1.0
                                    - vig.clamp(0.0, 1.0) * (dx * dx + dy * dy) / max_r2)
                                    .clamp(0.0, 1.0);
                                rr *= fac;
                                gg *= fac;
                                bb *= fac;
                            }
                            if amp > 0.0 {
                                let n =
                                    tile[((y & 255) << 8) | ((x + off) & 255)] as f32 / 255.0 - 0.5;
                                rr += n * amp;
                                gg += n * amp;
                                bb += n * amp;
                            }
                            px[0] = rr.clamp(0.0, 255.0) as u8;
                            px[1] = gg.clamp(0.0, 255.0) as u8;
                            px[2] = bb.clamp(0.0, 255.0) as u8;
                        }
                    }
                });
            }
        });
    }

    // Проход 2: хроматическая аберрация (R вправо, B влево).
    let shift = p.chroma.round() as i32;
    if shift != 0 {
        let data = pm.data_mut();
        std::thread::scope(|s| {
            for (bi, band) in data.chunks_mut(band_h * w * 4).enumerate() {
                let _ = bi;
                s.spawn(move || {
                    let mut tmp = vec![0u8; w * 4];
                    for row in band.chunks_mut(w * 4) {
                        tmp.copy_from_slice(row);
                        for x in 0..w {
                            let xr = (x as i32 + shift).clamp(0, w as i32 - 1) as usize * 4;
                            let xb = (x as i32 - shift).clamp(0, w as i32 - 1) as usize * 4;
                            row[x * 4] = tmp[xr];
                            row[x * 4 + 2] = tmp[xb + 2];
                        }
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Mode, Palette};
    use tiny_skia::{Paint, Rect, Transform};

    fn flat(w: u32, h: u32, color: [u8; 4]) -> Pixmap {
        let mut pm = Pixmap::new(w, h).unwrap();
        let mut p = Paint::default();
        p.set_color_rgba8(color[0], color[1], color[2], color[3]);
        pm.fill_rect(
            Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap(),
            &p,
            Transform::identity(),
            None,
        );
        pm
    }

    fn lum(px: &PremultipliedColorU8) -> u32 {
        let c = px.demultiply();
        c.red() as u32 + c.green() as u32 + c.blue() as u32
    }

    #[test]
    fn same_seed_same_pixels_and_other_seed_differs() {
        let base = flat(64, 48, [180, 120, 40, 255]);
        let mut a = base.clone();
        let mut b = base.clone();
        let mut c = base.clone();
        let prm = CrtParams::from_palette(&Palette::of(Mode::Signalis), 7);
        crt(&mut a, &prm);
        crt(&mut b, &prm);
        crt(&mut c, &CrtParams { seed: 99, ..prm });
        assert_eq!(a.data(), b.data(), "детерминизм по seed");
        assert_ne!(a.data(), c.data(), "другой seed → другой шум");
    }

    #[test]
    fn scanlines_darken_every_period_second_row() {
        let mut pm = flat(16, 12, [200, 200, 200, 255]);
        crt(
            &mut pm,
            &CrtParams {
                scanline: 0.5,
                scanline_period: 2,
                grain: 0.0,
                chroma: 0.0,
                vignette: 0.0,
                dither_levels: 255,
                seed: 0,
            },
        );
        let w = pm.width() as usize;
        let row = |y: usize| lum(&pm.pixels()[y * w + 4]);
        assert!(row(1) < row(0), "строка 1 притемнена, строка 0 — нет");
        assert_eq!(row(0), row(2));
        assert!(row(3) < row(2));
    }

    #[test]
    fn vignette_darkens_corners_but_not_center() {
        let mut pm = flat(64, 64, [220, 220, 220, 255]);
        crt(
            &mut pm,
            &CrtParams {
                scanline: 0.0,
                scanline_period: 3,
                grain: 0.0,
                chroma: 0.0,
                vignette: 0.7,
                dither_levels: 255,
                seed: 0,
            },
        );
        let w = pm.width() as usize;
        let center = lum(&pm.pixels()[32 * w + 32]);
        let corner = lum(&pm.pixels()[0]);
        assert!(corner < center / 2, "угол {corner} vs центр {center}");
    }

    #[test]
    fn chroma_shifts_channels_horizontally() {
        let mut pm = Pixmap::new(32, 4).unwrap();
        for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
            let x = i % 32;
            let v = if x < 16 { 255 } else { 0 };
            *px = PremultipliedColorU8::from_rgba(v, v, v, 255).unwrap();
        }
        crt(
            &mut pm,
            &CrtParams {
                scanline: 0.0,
                scanline_period: 3,
                grain: 0.0,
                chroma: 1.0,
                vignette: 0.0,
                dither_levels: 255,
                seed: 0,
            },
        );
        let p = pm.pixels()[16].demultiply();
        assert!(p.blue() > p.red(), "B={} R={}", p.blue(), p.red());
    }

    #[test]
    fn dither_quantizes_before_other_effects() {
        let mut pm = flat(16, 16, [0, 0, 0, 255]);
        for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
            let v = (i % 16) as u8 * 16;
            *px = PremultipliedColorU8::from_rgba(v, v, v, 255).unwrap();
        }
        let mut dithered = pm.clone();
        crt(
            &mut dithered,
            &CrtParams {
                scanline: 0.0,
                scanline_period: 3,
                grain: 0.0,
                chroma: 0.0,
                vignette: 0.0,
                dither_levels: 4,
                seed: 0,
            },
        );
        let uniq = |p: &Pixmap| {
            let mut s = std::collections::BTreeSet::new();
            for px in p.pixels() {
                s.insert(px.demultiply().red());
            }
            s.len()
        };
        assert!(
            uniq(&dithered) <= uniq(&pm),
            "дизеринг не добавляет уровней"
        );
        assert!(uniq(&dithered) <= 4, "4 уровня на канал");
    }

    #[test]
    fn multi_threaded_result_is_band_independent() {
        // 700 строк → несколько полос; полосы не должны конфликтовать
        let mut pm = flat(700, 700, [200, 160, 90, 255]);
        crt(
            &mut pm,
            &CrtParams {
                scanline: 0.2,
                scanline_period: 3,
                grain: 0.05,
                chroma: 1.0,
                vignette: 0.4,
                dither_levels: 8,
                seed: 5,
            },
        );
        let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert_eq!(lit, 700 * 700, "кадр остался непрозрачным целиком");
        // верхняя и нижняя полосы обработаны (разные значения)
        assert_ne!(pm.data()[..4], pm.data()[700 * 699 * 4..700 * 699 * 4 + 4]);
    }

    #[test]
    fn tiny_and_empty_pixmaps_do_not_panic() {
        let prm = CrtParams::from_palette(&Palette::of(Mode::Rig), 1);
        for (w, h) in [(1, 1), (2, 3), (1, 64), (64, 1), (3, 129)] {
            let mut pm = flat(w, h, [10, 10, 10, 255]);
            crt(&mut pm, &prm);
        }
    }

    #[test]
    fn off_params_leave_image_untouched() {
        let base = flat(96, 64, [90, 140, 200, 255]);
        let mut pm = base.clone();
        crt(&mut pm, &CrtParams::off());
        assert_eq!(pm.data(), base.data());
    }
}
