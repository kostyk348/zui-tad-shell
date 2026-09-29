//! # Модель Real / Virtual Objects (BTRON)
//!
//! **Real Object (RO)** — физическая сущность: документ TAD + метаданные.
//! Существует ровно в одном экземпляре, имеет уникальный ID, хранится в БД.
//!
//! **Virtual Object (VO)** — ссылка на RO + её пространственное размещение
//! внутри другого документа. Один и тот же RO может быть отображён через
//! миллион VO в разных местах холста (как симлинки, но с координатами).
//!
//! Это позволяет строить циклические графы: документ A содержит VO→B,
//! а B содержит VO→A. Физических копий данных не создаётся.

use crate::tad::{BlobRef, TadDocument};
use cgmath::{Point2, Vector2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type RoId = Uuid;
pub type VoId = Uuid;

/// Реальный объект. Хранится один раз.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealObject {
    pub id: RoId,
    pub title: String,
    pub kind: ObjectKind,
    /// Содержимое — дерево сегментов TAD.
    pub document: TadDocument,
    /// Метаданные (теги, даты, автор и т.д.).
    pub meta: HashMap<String, String>,
    /// Документные единицы (ширина, высота «листа»).
    /// Canvas-движок использует это как габариты по умолчанию.
    pub doc_size: (f32, f32),
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ObjectKind {
    Text,
    Mindmap,
    Table,
    Image,
    Portal,        // чисто-ссылочный объект
    WaylandWindow, // окно стороннего приложения
    Folder,
    /// Встроенный файловый менеджер (TAD-документ).
    Files,
    /// Встроенный калькулятор.
    Calculator,
    /// Встроенные настройки DE.
    Settings,
}

/// Виртуальный объект — размещение RO на холсте родительского RO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualObject {
    pub id: VoId,
    /// На какой RO указывает эта ссылка.
    pub target_ro: RoId,
    /// ID родительского RO (для графа связей).
    pub parent_ro: RoId,
    /// Мировые координаты левого-верхнего угла VO
    /// (в системе координат родительского документа).
    pub pos: Point2<f32>,
    /// Размер отображаемого VO.
    pub size: Vector2<f32>,
    /// Коэффициент масштаба относительно «нативного» размера RO.
    /// 1.0 = 1:1, 0.5 = половина, 2.0 = увеличено вдвое.
    pub scale: f32,
    /// Текущий режим отображения. Реально управляется canvas-engine
    /// в зависимости от semantic LOD, но кешируется для скорости.
    pub display_mode: DisplayMode,
    /// Дополнительный поворот (радианы). Обычно 0.
    pub rotation: f32,
    /// Z-индекс для отрисовки.
    pub z: i32,
}

impl VirtualObject {
    pub fn new(target_ro: RoId, parent_ro: RoId, pos: Point2<f32>, size: Vector2<f32>) -> Self {
        Self {
            id: Uuid::new_v4(),
            target_ro,
            parent_ro,
            pos,
            size,
            scale: 1.0,
            display_mode: DisplayMode::Icon,
            rotation: 0.0,
            z: 0,
        }
    }
}

/// Семантический режим отображения VO.
/// Движок LOD переключает эти режимы в зависимости от Scale Factor камеры.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DisplayMode {
    /// Слишком далеко — рисуем только иконку/заголовок.
    Icon,
    /// Средняя дистанция — статичное превью (rasterized).
    Preview,
    /// Близко — живой интерактивный контент.
    Live,
    /// Фокус ввода — редактор активен.
    Focused,
}

impl DisplayMode {
    pub fn from_scale(s: f32) -> Self {
        if s < 0.1 {
            DisplayMode::Icon
        } else if s < 0.5 {
            DisplayMode::Preview
        } else if s < 0.8 {
            DisplayMode::Live
        } else {
            DisplayMode::Focused
        }
    }
}

/// Хранилище бинарных блобов (картинки, шрифты).
/// Дедуплицируется по SHA-256 → Uuid.
pub struct BlobStore {
    db: sled::Tree,
}

impl BlobStore {
    pub fn open(db: &sled::Db) -> anyhow::Result<Self> {
        Ok(Self {
            db: db.open_tree("blobs")?,
        })
    }

    pub fn put(&self, bytes: &[u8]) -> anyhow::Result<BlobRef> {
        let id = Uuid::new_v4();
        self.db.insert(id.as_bytes(), bytes)?;
        Ok(BlobRef(id))
    }

    pub fn get(&self, r: &BlobRef) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.db.get(r.0.as_bytes())?.map(|v| v.to_vec()))
    }
}
