//! # editor-core
//!
//! Контекстные редакторы и фокус-система.

pub mod editors;
pub mod focus;
pub mod portals;

pub use editors::{EditorRegistry, TableEditor, TextEditor, ToolCommand, VectorEditor};
pub use focus::{editor_kind_for_segment, EditorKind, FocusManager, FocusTarget, InputEvent};
pub use portals::{find_portal_target, FlyAnimation, PortalSystem};
