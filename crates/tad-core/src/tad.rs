//! # TAD (Tree-structured Arranged Data) — лёгкий формат документа
//!
//! TAD заменяет понятие «файл приложения». Документ — это упорядоченное дерево
//! сегментов разных типов (заголовок, текст, растр, вектор, ссылка, таблица).
//! Любой сегмент может сам быть документом (рекурсия), что позволяет встраивать
//! одни объекты внутрь других.
//!
//! Формат сериализации: MessagePack (`rmp-serde`). Это компактнее JSON и
//! быстрее парсится, что критично при отрисовке тысяч VO на холсте.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Уникальный идентификатор сегмента внутри документа.
pub type SegmentId = Uuid;

/// Тип сегмента TAD. Определяет, какой редактор подключится при фокусе.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "data")]
pub enum Segment {
    /// Заголовок уровня N.
    Heading { level: u8, text: String },
    /// Абзац обычного текста (plain UTF-8).
    Text { text: String },
    /// Растровое изображение. `bytes` хранятся отдельно в BlobStore.
    Image {
        blob_ref: BlobRef,
        width: u32,
        height: u32,
    },
    /// Векторная 2D-графика (SVG-подобное описание).
    Vector { shapes: Vec<VectorShape> },
    /// Таблица N×M ячеек.
    Table { rows: Vec<Vec<String>> },
    /// Гиперссылка-портал на другой Real Object.
    Link { target_ro: Uuid, label: String },
    /// Произвольные метаданные (для плагинов).
    Custom {
        kind: String,
        payload: serde_json::Value,
    },
}

/// Описание одной векторной фигуры.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VectorShape {
    pub kind: VectorKind,
    pub stroke: Option<[f32; 4]>, // RGBA
    pub fill: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum VectorKind {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
    Path {
        points: Vec<(f32, f32)>,
        width: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
    },
}

/// Ссылка на бинарный блоб (растровые пиксели, шрифты и т.д.).
/// Хранится отдельно от сегментов для дедупликации.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct BlobRef(pub Uuid);

/// Упорядоченный сегмент: сам сегмент + его позиция в дереве.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedSegment {
    pub id: SegmentId,
    pub segment: Segment,
    /// Локальные координаты относительно родительского документа.
    /// (0,0) — верхний левый угол, размер — в «документных единицах»
    /// (не пикселях!).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Документ TAD — упорядоченное дерево сегментов.
/// Это и есть «содержимое» Real Object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TadDocument {
    pub schema: u16, // версия формата, текущая = 1
    pub root_segments: Vec<OrderedSegment>,
}

impl TadDocument {
    pub fn new() -> Self {
        Self {
            schema: 1,
            root_segments: Vec::new(),
        }
    }

    /// Сериализация в бинарный MessagePack.
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        Ok(rmp_serde::to_vec_named(self)?)
    }

    /// Десериализация.
    pub fn from_bytes(b: &[u8]) -> anyhow::Result<Self> {
        Ok(rmp_serde::from_slice(b)?)
    }

    /// Поиск сегмента по ID (BFS по дереву).
    pub fn find(&self, id: SegmentId) -> Option<&OrderedSegment> {
        self.root_segments.iter().find(|s| s.id == id)
    }

    /// Мутабельный поиск.
    pub fn find_mut(&mut self, id: SegmentId) -> Option<&mut OrderedSegment> {
        self.root_segments.iter_mut().find(|s| s.id == id)
    }

    /// Добавить сегмент в конец.
    pub fn push(&mut self, seg: Segment, x: f32, y: f32, w: f32, h: f32) -> SegmentId {
        let id = Uuid::new_v4();
        self.root_segments.push(OrderedSegment {
            id,
            segment: seg,
            x,
            y,
            w,
            h,
        });
        id
    }
}

impl Default for TadDocument {
    fn default() -> Self {
        Self::new()
    }
}
