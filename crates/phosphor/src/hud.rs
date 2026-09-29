//! HUD оболочки: миникарта, виталы (ЭКГ), луч развёртки, рамка видоискателя,
//! тосты. Всё — тонкие линии и сегменты, ничего «стеклянного».
//!
//! Чистая математика (подгонка миникарты, кольцевой буфер ЭКГ, жизнь тоста)
//! вынесена в функции без побочных эффектов — чтобы её можно было тестировать.

use std::time::{Duration, Instant};

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::theme::{Metrics, Palette};
use crate::widgets::{draw_text, frame, hairline, text_width, Fonts};

// ---------------------------------------------------------------- миникарта

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapFit {
    pub scale: f32,
    pub ox: f32,
    pub oy: f32,
}

/// Подогнать мировой прямоугольник (x, y, w, h) в экранный бокс (w, h) с полем.
/// Возвращает масштаб и смещение; масштаб никогда не превышает 1:1.
pub fn minimap_fit(bounds: (f32, f32, f32, f32), box_size: (f32, f32), pad: f32) -> MinimapFit {
    let (_, _, bw, bh) = bounds;
    let usable_w = (box_size.0 - pad * 2.0).max(1.0);
    let usable_h = (box_size.1 - pad * 2.0).max(1.0);
    let sx = usable_w / bw.max(1.0);
    let sy = usable_h / bh.max(1.0);
    let scale = sx.min(sy).min(1.0);
    // центр содержимого → центр бокса
    let ox = pad + (usable_w - bw * scale) * 0.5;
    let oy = pad + (usable_h - bh * scale) * 0.5;
    MinimapFit { scale, ox, oy }
}

/// Миникарта холста: рамки окон + текущий вьюпорт.
#[allow(clippy::too_many_arguments)]
pub fn minimap(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    rect: (f32, f32, f32, f32),
    bounds: (f32, f32, f32, f32),
    windows: &[(f32, f32, f32, f32, bool)],
    viewport: (f32, f32, f32, f32),
) {
    let (x, y, w, h) = rect;
    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.bg[0], pal.bg[1], pal.bg[2], 0xcc);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    frame(
        pm,
        rect,
        crate::demo::with_alpha(pal.dim, 0xff),
        m.line,
        5.0,
    );
    draw_text(
        pm,
        &fonts.regular,
        "MAP · CANVAS",
        x + 8.0,
        y + 13.0,
        m.label_size,
        m.tracking,
        pal.dim,
    );

    let fit = minimap_fit(bounds, (w, h), 16.0);
    let to_local = |wx: f32, wy: f32| -> (f32, f32) {
        (
            x + fit.ox + (wx - bounds.0) * fit.scale,
            y + fit.oy + (wy - bounds.1) * fit.scale,
        )
    };

    // окна
    for (wx, wy, ww, wh, focused) in windows {
        let (lx, ly) = to_local(*wx, *wy);
        let color = if *focused {
            pal.primary
        } else {
            crate::demo::with_alpha(pal.dim, 0xdd)
        };
        let rw = (ww * fit.scale).max(2.0);
        let rh = (wh * fit.scale).max(2.0);
        let mut p = Paint::default();
        p.set_color_rgba8(color[0], color[1], color[2], color[3]);
        if let Some(r) = Rect::from_xywh(lx, ly, rw, rh) {
            pm.fill_rect(r, &p, Transform::identity(), None);
        }
    }

    // вьюпорт камеры
    let (vx, vy) = to_local(viewport.0, viewport.1);
    let vr = (
        vx,
        vy,
        (viewport.2 * fit.scale).max(3.0),
        (viewport.3 * fit.scale).max(3.0),
    );
    frame(pm, vr, pal.cold, m.line, 0.0);
}

// ---------------------------------------------------------------- виталы

/// Кольцевой буфер ЭКГ: последние `cap` сэмплов, значение нормировано в 0..1.
#[derive(Debug, Clone)]
pub struct Ecg {
    pub samples: Vec<f32>,
    head: usize,
    cap: usize,
}

impl Ecg {
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(2);
        Self {
            samples: vec![0.5; cap],
            head: 0,
            cap,
        }
    }

    pub fn push(&mut self, v: f32) {
        self.head = (self.head + 1) % self.cap;
        self.samples[self.head] = v.clamp(0.0, 1.0);
    }

    /// Генератор правдоподобного пульса: спайк + плавный тренд.
    pub fn push_beat(&mut self, t: f32, bpm: f32) {
        let period = 60.0 / bpm.max(20.0);
        let phase = (t / period).fract();
        // QRS-комплекс: короткий пик на ~0.1 фазы, дальше покой с дыханием
        let spike = if phase < 0.08 {
            1.0
        } else if phase < 0.14 {
            0.35
        } else {
            0.0
        };
        let baseline = 0.35 + 0.05 * (t * 1.7).sin();
        self.push(baseline + spike * 0.6);
    }

    /// Значения от старых к новым.
    pub fn ordered(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.cap);
        for i in 1..=self.cap {
            out.push(self.samples[(self.head + i) % self.cap]);
        }
        out
    }

    pub fn last(&self) -> f32 {
        self.samples[self.head]
    }
}

/// Полоса ЭКГ (RIG «vital signs»).
#[allow(clippy::too_many_arguments)]
pub fn vitals(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    ecg: &Ecg,
    bpm: f32,
) {
    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.bg[0], pal.bg[1], pal.bg[2], 0xaa);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    frame(
        pm,
        (x, y, w, h),
        crate::demo::with_alpha(pal.dim, 0xcc),
        m.line,
        4.0,
    );
    draw_text(
        pm,
        &fonts.regular,
        &format!("VITAL {:.0}", bpm),
        x + 8.0,
        y + 12.0,
        m.label_size,
        m.tracking,
        pal.dim,
    );

    let samples = ecg.ordered();
    let n = samples.len().max(2);
    let inner_h = h - 20.0;
    let top = y + 16.0;
    for i in 1..n {
        let t0 = (i - 1) as f32 / (n - 1) as f32;
        let t1 = i as f32 / (n - 1) as f32;
        let x0 = x + 6.0 + t0 * (w - 12.0);
        let x1 = x + 6.0 + t1 * (w - 12.0);
        let y0 = top + (1.0 - samples[i - 1]) * inner_h;
        let y1 = top + (1.0 - samples[i]) * inner_h;
        // пики ярче, фон тусклее — читается как настоящая ЭКГ
        let v = samples[i].max(samples[i - 1]);
        let color = if v > 0.8 { pal.primary } else { pal.dim };
        hairline(pm, x0, y0, x1, y1, color, m.line);
    }
}

// ---------------------------------------------------------------- развёртка

/// Луч CRT-развёртки: яркая линия, ползущая вниз (фаза 0..1).
pub fn scan_beam(pm: &mut Pixmap, pal: &Palette, phase: f32) {
    let h = pm.height() as f32;
    let y = (phase.clamp(0.0, 1.0) * h).round();
    if y < 1.0 || y > h - 2.0 {
        return;
    }
    let mut p = Paint::default();
    p.set_color_rgba8(pal.primary[0], pal.primary[1], pal.primary[2], 0x22);
    if let Some(r) = Rect::from_xywh(0.0, y - 2.0, pm.width() as f32, 4.0) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
    p.set_color_rgba8(pal.primary[0], pal.primary[1], pal.primary[2], 0x48);
    if let Some(r) = Rect::from_xywh(0.0, y, pm.width() as f32, 1.0) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
}

// ---------------------------------------------------------------- видоискатель

/// Рамка видоискателя: угловые скобки по краям экрана + прицел в центре.
pub fn viewport_frame(pm: &mut Pixmap, pal: &Palette, m: &Metrics) {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let c = crate::demo::with_alpha(pal.dim, 0xaa);
    let d = 26.0;
    let inset = 6.0;
    for (cx, cy, sx, sy) in [
        (inset, inset, 1.0, 1.0),
        (w - inset, inset, -1.0, 1.0),
        (inset, h - inset, 1.0, -1.0),
        (w - inset, h - inset, -1.0, -1.0),
    ] {
        hairline(pm, cx, cy, cx + d * sx, cy, c, m.line);
        hairline(pm, cx, cy, cx, cy + d * sy, c, m.line);
    }
    // прицел камеры
    let (mx, my) = (w * 0.5, h * 0.5);
    hairline(pm, mx - 9.0, my, mx + 9.0, my, pal.cold, m.line);
    hairline(pm, mx, my - 9.0, mx, my + 9.0, pal.cold, m.line);
    // шкала зума слева
    let mut ty = h * 0.5 - 40.0;
    for _ in 0..9 {
        hairline(pm, inset + 2.0, ty, inset + 10.0, ty, c, m.line);
        ty += 10.0;
    }
}

// ---------------------------------------------------------------- тосты

#[derive(Debug, Clone)]
pub struct Toast {
    pub title: String,
    pub body: String,
    pub at: Instant,
    pub ttl: Duration,
}

impl Toast {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            at: Instant::now(),
            ttl: Duration::from_millis(4200),
        }
    }

    pub fn alive(&self) -> bool {
        self.at.elapsed() < self.ttl
    }

    /// Прозрачность по времени жизни: держится, потом гаснет последние 900 мс.
    pub fn alpha(&self) -> f32 {
        let left = self.ttl.saturating_sub(self.at.elapsed()).as_millis() as f32;
        (left / 900.0).clamp(0.0, 1.0)
    }
}

/// Тост уведомления (правый верх, под панелью).
pub fn toast(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    screen_w: f32,
    y: f32,
    t: &Toast,
) -> (f32, f32) {
    let w = 380.0;
    let h = 58.0;
    let x = screen_w - w - 24.0;
    let a = (t.alpha() * 235.0) as u8;

    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.panel_bg[0], pal.panel_bg[1], pal.panel_bg[2], a);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    let mut accent = pal.primary;
    accent[3] = a;
    frame(pm, (x, y, w, h), accent, m.line, 6.0);
    // маркер слева
    let mut mk = Paint::default();
    mk.set_color_rgba8(accent[0], accent[1], accent[2], a);
    if let Some(r) = Rect::from_xywh(x, y, 3.0, h) {
        pm.fill_rect(r, &mk, Transform::identity(), None);
    }

    let mut dim = pal.dim;
    dim[3] = a;
    let mut text = pal.text;
    text[3] = a;

    draw_text(
        pm,
        &fonts.bold,
        &t.title.to_uppercase(),
        x + 14.0,
        y + 22.0,
        m.value_size,
        m.tracking,
        accent,
    );
    draw_text(
        pm,
        &fonts.regular,
        &t.body,
        x + 14.0,
        y + 42.0,
        m.label_size,
        m.tracking,
        dim,
    );
    let mark = "●";
    let mw = text_width(&fonts.regular, mark, m.label_size, 0.0);
    draw_text(
        pm,
        &fonts.regular,
        mark,
        x + w - mw - 12.0,
        y + 22.0,
        m.label_size,
        0.0,
        text,
    );
    (x, y)
}

/// HUD над окном: app_id + размер в мировых единицах + зум.
pub fn window_hud(
    pm: &mut Pixmap,
    fonts: &Fonts,
    pal: &Palette,
    m: &Metrics,
    screen_rect: (f32, f32, f32, f32),
    title: &str,
    size: (f32, f32),
    zoom: f32,
) {
    let (x, y, w, _) = screen_rect;
    let label = format!(
        "{}  ·  {:.0}×{:.0}  ·  {:.2}×",
        title.to_uppercase(),
        size.0,
        size.1,
        zoom
    );
    let tw = text_width(&fonts.regular, &label, m.label_size, m.tracking);
    let bx = x.min(pm.width() as f32 - tw - 20.0);
    let by = (y - 18.0).max(0.0);
    let mut bg = Paint::default();
    bg.set_color_rgba8(pal.bg[0], pal.bg[1], pal.bg[2], 0xd0);
    if let Some(r) = Rect::from_xywh(bx - 6.0, by, tw + 12.0, 15.0) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    draw_text(
        pm,
        &fonts.regular,
        &label,
        bx,
        by + 11.0,
        m.label_size,
        m.tracking,
        pal.cold,
    );
    let _ = w;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Mode;

    #[test]
    fn minimap_fit_keeps_content_inside_box_and_never_zooms_in() {
        let fit = minimap_fit((0.0, 0.0, 4000.0, 2000.0), (280.0, 180.0), 16.0);
        assert!(fit.scale <= 1.0, "миникарта только уменьшает");
        let w = 4000.0 * fit.scale;
        assert!(w <= 280.0 - 32.0 + 0.5, "ширина влезла: {w}");
        assert!(fit.ox >= 0.0 && fit.oy >= 0.0);
    }

    #[test]
    fn minimap_fit_handles_degenerate_bounds() {
        let fit = minimap_fit((10.0, 10.0, 0.0, 0.0), (100.0, 60.0), 8.0);
        assert!(fit.scale > 0.0 && fit.scale <= 1.0);
        assert!(fit.scale.is_finite());
    }

    #[test]
    fn minimap_fit_centers_small_content() {
        let fit = minimap_fit((0.0, 0.0, 100.0, 100.0), (200.0, 100.0), 10.0);
        // по высоте 1:1 (80/100 → 0.8), значит поля по ширине
        assert!((fit.scale - 0.8).abs() < 1e-3, "scale={}", fit.scale);
        assert!(fit.ox > 10.0, "малое содержимое центруется по X");
    }

    #[test]
    fn ecg_ring_buffer_wraps_and_orders() {
        let mut e = Ecg::new(4);
        for v in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6] {
            e.push(v);
        }
        let o = e.ordered();
        assert_eq!(o.len(), 4);
        assert!((o[3] - 0.6).abs() < 1e-6, "последний — свежий: {o:?}");
        assert!((o[0] - 0.3).abs() < 1e-6, "старейший вытеснен: {o:?}");
        assert!((e.last() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn ecg_clamps_values() {
        let mut e = Ecg::new(2);
        e.push(-5.0);
        e.push(9.0);
        assert!(e.ordered().iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn ecg_beat_is_periodic_and_spiky() {
        let mut e = Ecg::new(64);
        let mut beats = 0;
        let mut prev = 0.0_f32;
        for i in 0..600 {
            let t = i as f32 * 0.01; // 6 секунд
            e.push_beat(t, 60.0); // период 1 c → ~6 ударов
            let cur = e.last();
            if cur > 0.9 && prev <= 0.9 {
                beats += 1; // фронт, а не каждый сэмпл пика
            }
            prev = cur;
        }
        assert!(
            (5..=7).contains(&beats),
            "за 6 c при 60 bpm ждём ~6 ударов, got {beats}"
        );
    }

    #[test]
    fn toast_expires_and_fades() {
        let mut t = Toast::new("CANVAS", "restored");
        assert!(t.alive());
        assert!((t.alpha() - 1.0).abs() < 1e-6);
        t.at = Instant::now() - Duration::from_millis(4000); // 200 мс до конца
        assert!(t.alive());
        assert!(t.alpha() < 1.0 && t.alpha() > 0.0, "гаснет: {}", t.alpha());
        t.at = Instant::now() - Duration::from_millis(5000);
        assert!(!t.alive());
        assert_eq!(t.alpha(), 0.0);
    }

    #[test]
    fn hud_drawing_smoke_test() {
        let fonts = Fonts::load_default().unwrap();
        let pal = Palette::of(Mode::Rig);
        let m = Metrics::default();
        let mut pm = Pixmap::new(400, 300).unwrap();
        let mut ecg = Ecg::new(64);
        for i in 0..64 {
            ecg.push_beat(i as f32 * 0.02, 72.0);
        }
        minimap(
            &mut pm,
            &fonts,
            &pal,
            &m,
            (10.0, 10.0, 200.0, 120.0),
            (0.0, 0.0, 2000.0, 1000.0),
            &[(0.0, 0.0, 900.0, 600.0, true)],
            (0.0, 0.0, 1280.0, 800.0),
        );
        vitals(
            &mut pm, &fonts, &pal, &m, 10.0, 150.0, 200.0, 60.0, &ecg, 72.0,
        );
        scan_beam(&mut pm, &pal, 0.4);
        viewport_frame(&mut pm, &pal, &m);
        window_hud(
            &mut pm,
            &fonts,
            &pal,
            &m,
            (20.0, 40.0, 900.0, 600.0),
            "alacritty",
            (900.0, 600.0),
            1.25,
        );
        let t = Toast::new("NEW WINDOW", "alacritty · 900×600");
        let _ = toast(&mut pm, &fonts, &pal, &m, 400.0, 30.0, &t);
        let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(lit > 1000, "HUD должен нарисоваться: {lit}");
    }
}
