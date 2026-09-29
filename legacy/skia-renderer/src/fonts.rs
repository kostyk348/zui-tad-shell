//! Загрузка шрифтов для рендерера.
//! По умолчанию берёт DejaVuSans (поддержка Latin + Cyrillic).

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use anyhow::{anyhow, Result};
use std::sync::Arc;

pub struct FontCache {
    pub regular: Arc<FontVec>,
    pub bold: Arc<FontVec>,
    pub mono: Arc<FontVec>,
}

impl FontCache {
    /// Загрузка шрифтов по умолчанию.
    /// Ищет в нескольких местах: CWD/assets/fonts, рядом с бинарником, системные пути.
    pub fn load_default() -> Result<Self> {
        // Список кандидатов: (regular, bold). Перебираем по очереди.
        let local_candidates = [
            "assets/fonts/DejaVuSans.ttf",
            "../assets/fonts/DejaVuSans.ttf",
            "../../assets/fonts/DejaVuSans.ttf",
        ];
        let mut regular_path: Option<std::path::PathBuf> = None;
        for p in local_candidates {
            if std::path::Path::new(p).exists() {
                regular_path = Some(std::path::PathBuf::from(p));
                break;
            }
        }
        // Пробуем найти рядом с исполняемым бинарником.
        if regular_path.is_none() {
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    let p = dir.join("assets/fonts/DejaVuSans.ttf");
                    if p.exists() {
                        regular_path = Some(p);
                    } else {
                        let p = dir.join("../assets/fonts/DejaVuSans.ttf");
                        if p.exists() {
                            regular_path = Some(p);
                        }
                    }
                }
            }
        }
        // Системные пути.
        let regular_path = regular_path.unwrap_or_else(|| {
            std::path::PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
        });
        let bold_path = regular_path.with_file_name("DejaVuSans-Bold.ttf");

        let regular_bytes = std::fs::read(&regular_path)
            .map_err(|e| anyhow!("regular font {:?}: {e}", regular_path))?;
        let bold_bytes =
            std::fs::read(&bold_path).map_err(|e| anyhow!("bold font {:?}: {e}", bold_path))?;
        let regular =
            FontVec::try_from_vec(regular_bytes).map_err(|e| anyhow!("regular parse: {e}"))?;
        let bold = FontVec::try_from_vec(bold_bytes).map_err(|e| anyhow!("bold parse: {e}"))?;
        // mono = regular (упрощение) — перечитываем, т.к. FontVec не Clone.
        let mono_bytes = std::fs::read(&regular_path)?;
        let mono = FontVec::try_from_vec(mono_bytes).map_err(|e| anyhow!("mono parse: {e}"))?;

        Ok(Self {
            regular: Arc::new(regular),
            bold: Arc::new(bold),
            mono: Arc::new(mono),
        })
    }

    /// Измерить ширину текста данным шрифтом.
    pub fn text_width(text: &str, font: &FontVec, size: f32) -> f32 {
        let scale = PxScale::from(size);
        let scaled = font.as_scaled(scale);
        let mut w = 0.0_f32;
        let mut last: Option<ab_glyph::GlyphId> = None;
        for ch in text.chars() {
            let gid = scaled.scaled_glyph(ch).id;
            w += scaled.h_advance(gid);
            if let Some(prev) = last {
                w += scaled.kern(prev, gid);
            }
            last = Some(gid);
        }
        w
    }
}
