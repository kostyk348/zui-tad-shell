//! # tad-core
//!
//! Ядро данных ZUI-TAD Shell: формат TAD, модель Real/Virtual Objects,
//! графовая файловая система, импорт/экспорт.

pub mod graph;
pub mod import;
pub mod objects;
pub mod tad;

pub use graph::GraphStore;
pub use objects::{BlobStore, DisplayMode, ObjectKind, RealObject, RoId, VirtualObject, VoId};
pub use tad::{BlobRef, OrderedSegment, Segment, SegmentId, TadDocument, VectorKind, VectorShape};
