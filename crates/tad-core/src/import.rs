//! # Импорт/Экспорт обычных файлов в TAD-сегменты
//!
//! Drag-and-drop файла с диска → создаётся новый RO, чьё содержимое
//! собирается из TAD-сегментов. Бинарные данные (пиксели изображений)
//! сразу пишутся в `BlobStore`, в сегменте `Image` хранится только `BlobRef`.

use crate::objects::{BlobStore, ObjectKind, RealObject};
use crate::tad::{Segment, TadDocument};
use anyhow::Context;
use std::path::Path;
use uuid::Uuid;

/// Создать RO из файла на диске.
/// `blobs` — открытое хранилище блобов; для текста можно `None`.
pub fn import_file(path: &Path, blobs: Option<&BlobStore>) -> anyhow::Result<RealObject> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();

    let (kind, doc) = match ext.as_str() {
        "txt" => (
            ObjectKind::Text,
            import_text(std::fs::read_to_string(path)?)?,
        ),
        "md" => (
            ObjectKind::Text,
            import_markdown(std::fs::read_to_string(path)?)?,
        ),
        "png" | "jpg" | "jpeg" => {
            let store = blobs.context("BlobStore required for image import")?;
            (ObjectKind::Image, import_image(path, store)?)
        }
        _ => anyhow::bail!("unsupported extension: .{ext}"),
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    Ok(RealObject {
        id: Uuid::new_v4(),
        title,
        kind,
        document: doc,
        meta: Default::default(),
        doc_size: (800.0, 600.0),
        created_at: now,
        updated_at: now,
    })
}

fn import_text(content: String) -> anyhow::Result<TadDocument> {
    let mut doc = TadDocument::new();
    for (i, line) in content.lines().enumerate() {
        if !line.is_empty() {
            doc.push(
                Segment::Text {
                    text: line.to_string(),
                },
                20.0,
                20.0 + (i as f32) * 24.0,
                760.0,
                24.0,
            );
        }
    }
    Ok(doc)
}

fn import_markdown(content: String) -> anyhow::Result<TadDocument> {
    let mut doc = TadDocument::new();
    let mut y = 20.0_f32;
    for line in content.lines() {
        if let Some(h) = line.strip_prefix("# ") {
            doc.push(
                Segment::Heading {
                    level: 1,
                    text: h.to_string(),
                },
                20.0,
                y,
                760.0,
                36.0,
            );
            y += 44.0;
        } else if let Some(h) = line.strip_prefix("## ") {
            doc.push(
                Segment::Heading {
                    level: 2,
                    text: h.to_string(),
                },
                20.0,
                y,
                760.0,
                28.0,
            );
            y += 34.0;
        } else if let Some(h) = line.strip_prefix("### ") {
            doc.push(
                Segment::Heading {
                    level: 3,
                    text: h.to_string(),
                },
                20.0,
                y,
                760.0,
                24.0,
            );
            y += 30.0;
        } else if !line.is_empty() {
            doc.push(
                Segment::Text {
                    text: line.to_string(),
                },
                20.0,
                y,
                760.0,
                24.0,
            );
            y += 28.0;
        }
    }
    Ok(doc)
}

fn import_image(path: &Path, store: &BlobStore) -> anyhow::Result<TadDocument> {
    let bytes = std::fs::read(path).context("read image")?;
    let img = image::load_from_memory(&bytes)?.to_rgba8();
    let (w, h) = img.dimensions();
    let blob_ref = store.put(&bytes)?;
    let mut doc = TadDocument::new();
    doc.push(
        Segment::Image {
            blob_ref,
            width: w,
            height: h,
        },
        0.0,
        0.0,
        w as f32,
        h as f32,
    );
    Ok(doc)
}

/// Экспорт RO в .md (для текстовых документов).
pub fn export_markdown(ro: &RealObject) -> anyhow::Result<String> {
    let mut out = String::new();
    for seg in &ro.document.root_segments {
        match &seg.segment {
            Segment::Heading { level, text } => {
                out.push_str(&"#".repeat(*level as usize));
                out.push(' ');
                out.push_str(text);
                out.push('\n');
            }
            Segment::Text { text } => {
                out.push_str(text);
                out.push('\n');
            }
            Segment::Link { label, target_ro } => {
                out.push_str(&format!("[{label}](tad://{target_ro})\n"));
            }
            Segment::Table { rows } => {
                if let Some(first) = rows.first() {
                    out.push_str(&first.join(" | "));
                    out.push('\n');
                    out.push_str(&first.iter().map(|_| "---").collect::<Vec<_>>().join(" | "));
                    out.push('\n');
                    for r in rows.iter().skip(1) {
                        out.push_str(&r.join(" | "));
                        out.push('\n');
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}
