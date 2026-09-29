//! Built-in TAD applications: Settings, Files, Calculator.

pub mod calculator;
pub mod files;
pub mod pins;
pub mod settings;

use cgmath::{Point2, Vector2};
use tad_core::{ObjectKind, RealObject, VirtualObject};

pub enum NativeAppKind {
    Settings,
    Files,
    Calculator,
}

pub fn create_native_ro(kind: NativeAppKind) -> RealObject {
    match kind {
        NativeAppKind::Settings => settings::build_ro(),
        NativeAppKind::Files => files::build_ro(),
        NativeAppKind::Calculator => calculator::build_ro(),
    }
}

pub fn create_native_vo(ro: &RealObject, parent_ro: uuid::Uuid, pos: Point2<f32>) -> VirtualObject {
    VirtualObject::new(
        ro.id,
        parent_ro,
        pos,
        Vector2::new(ro.doc_size.0, ro.doc_size.1),
    )
}

/// Parse calculator key input (digits and operators).
pub fn calculator_key(ch: char, state: &mut calculator::CalcState) -> bool {
    calculator::handle_key(ch, state)
}

/// Navigate files app one directory up.
pub fn files_go_up(state: &mut files::FilesState) {
    files::go_up(state);
}

/// Open selected path in files app.
pub fn files_open_selected(state: &mut files::FilesState) -> Option<std::path::PathBuf> {
    files::open_selected(state)
}

/// Toggle a settings row by index.
pub fn settings_toggle(state: &mut settings::SettingsState, row: usize) {
    settings::toggle_row(state, row);
}

/// Sync RO document from app state (after edits).
pub fn sync_ro_from_state(ro: &mut RealObject) {
    match ro.kind {
        ObjectKind::Calculator => {
            if let Ok(st) = serde_json::from_str::<calculator::CalcState>(
                ro.meta
                    .get("calc_state")
                    .map(String::as_str)
                    .unwrap_or("{}"),
            ) {
                calculator::write_doc(&mut ro.document, &st);
            }
        }
        ObjectKind::Files => {
            if let Ok(st) = serde_json::from_str::<files::FilesState>(
                ro.meta
                    .get("files_state")
                    .map(String::as_str)
                    .unwrap_or("{}"),
            ) {
                files::write_doc(&mut ro.document, &st);
            }
        }
        ObjectKind::Settings => {
            if let Ok(st) = serde_json::from_str::<settings::SettingsState>(
                ro.meta
                    .get("settings_state")
                    .map(String::as_str)
                    .unwrap_or("{}"),
            ) {
                settings::write_doc(&mut ro.document, &st);
            }
        }
        _ => {}
    }
}

pub fn load_state_into_ro(ro: &mut RealObject) {
    match ro.kind {
        ObjectKind::Calculator => calculator::init_meta(ro),
        ObjectKind::Files => files::init_meta(ro),
        ObjectKind::Settings => settings::init_meta(ro),
        _ => {}
    }
}

pub fn persist_state(ro: &RealObject) -> RealObject {
    let mut ro = ro.clone();
    match ro.kind {
        ObjectKind::Calculator => calculator::persist_meta(&mut ro),
        ObjectKind::Files => files::persist_meta(&mut ro),
        ObjectKind::Settings => settings::persist_meta(&mut ro),
        _ => {}
    }
    ro
}
