//! Быстрые операции над пиксмапой: где источник НЕПРОЗРАЧЕН, SourceOver не нужен.
//!
//! Замер 1600×900: `fill_rect` + `draw_pixmap` для фона и карточек окон стоили
//! ~9 мс из 15 (tiny-skia честно смешивает каждый пиксель), а memcpy тех же
//! байтов — доли миллисекунды.

use tiny_skia::{Pixmap, PremultipliedColorU8};

/// Залить весь пиксмап одним цветом (без AA-растеризатора).
pub fn fill(pm: &mut Pixmap, color: [u8; 4]) {
    let px = PremultipliedColorU8::from_rgba(color[0], color[1], color[2], color[3])
        .unwrap_or(PremultipliedColorU8::TRANSPARENT);
    for slot in pm.pixels_mut().iter_mut() {
        *slot = px;
    }
}

/// Скопировать `src` в `dst` со смещением (x, y), обрезая по границам.
/// Смешивания нет: байты переносятся как есть (только для непрозрачных src).
pub fn blit(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32) {
    let (dw, dh) = (dst.width() as i32, dst.height() as i32);
    let (sw, sh) = (src.width() as i32, src.height() as i32);
    if sw <= 0 || sh <= 0 {
        return;
    }
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + sw).min(dw);
    let y1 = (y + sh).min(dh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let rows = (y1 - y0) as usize;
    let cols = (x1 - x0) as usize;
    let (sx0, sy0) = ((x0 - x) as usize, (y0 - y) as usize);
    let (dstride, sstride) = (dst.width() as usize, src.width() as usize);

    let dst_px = dst.pixels_mut();
    let src_px = src.pixels();
    for r in 0..rows {
        let d0 = (y0 as usize + r) * dstride + x0 as usize;
        let s0 = (sy0 + r) * sstride + sx0;
        dst_px[d0..d0 + cols].copy_from_slice(&src_px[s0..s0 + cols]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pm(w: u32, h: u32, c: [u8; 4]) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        fill(&mut p, c);
        p
    }

    #[test]
    fn fill_sets_every_pixel() {
        let p = pm(8, 4, [10, 20, 30, 255]);
        for px in p.pixels() {
            let c = px.demultiply();
            assert_eq!([c.red(), c.green(), c.blue(), c.alpha()], [10, 20, 30, 255]);
        }
    }

    #[test]
    fn blit_copies_exact_rect_and_leaves_rest() {
        let mut dst = pm(16, 16, [0, 0, 0, 255]);
        let src = pm(4, 4, [200, 100, 50, 255]);
        blit(&mut dst, &src, 3, 5);
        let at = |x: usize, y: usize| {
            let c = dst.pixels()[y * 16 + x].demultiply();
            [c.red(), c.green(), c.blue()]
        };
        assert_eq!(at(3, 5), [200, 100, 50]);
        assert_eq!(at(6, 8), [200, 100, 50]);
        assert_eq!(at(2, 5), [0, 0, 0]);
        assert_eq!(at(7, 5), [0, 0, 0]);
    }

    #[test]
    fn blit_clips_at_edges_and_never_panics() {
        let mut dst = pm(10, 10, [0, 0, 0, 255]);
        let src = pm(6, 6, [255, 255, 255, 255]);
        for (x, y) in [(-3, -3), (7, 7), (-3, 4), (4, -3), (-99, 5), (99, 5)] {
            blit(&mut dst, &src, x, y);
        }
        assert_eq!(dst.pixels()[0].demultiply().red(), 255);
        assert_eq!(dst.pixels()[9 * 10 + 9].demultiply().red(), 255);
    }

    #[test]
    fn blit_outside_or_empty_is_noop() {
        let mut dst = pm(10, 10, [1, 2, 3, 255]);
        let before = dst.data().to_vec();
        let src = pm(4, 4, [9, 9, 9, 255]);
        blit(&mut dst, &src, 20, 0);
        blit(&mut dst, &src, 0, 20);
        blit(&mut dst, &src, -6, 0);
        assert_eq!(dst.data(), before.as_slice());
    }
}
