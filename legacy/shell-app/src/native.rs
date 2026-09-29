//! Native TAD apps integration (Settings, Files, Calculator).

use cgmath::Point2;
use native_apps::{create_native_ro, create_native_vo, NativeAppKind};
use tad_core::{GraphStore, ObjectKind, RealObject, RoId, VirtualObject};

pub fn spawn_native(
    kind: NativeAppKind,
    pos: Point2<f32>,
    store: &GraphStore,
    desktop_ro: RoId,
) -> anyhow::Result<(RealObject, VirtualObject)> {
    let mut ro = create_native_ro(kind);
    native_apps::load_state_into_ro(&mut ro);
    store.put_ro(&ro)?;
    let vo = create_native_vo(&ro, desktop_ro, pos);
    store.put_vo(&vo)?;
    Ok((ro, vo))
}

pub fn desktop_ro(ro_index: &std::collections::HashMap<RoId, RealObject>) -> RoId {
    ro_index
        .values()
        .find(|r| r.kind == ObjectKind::Folder)
        .map(|r| r.id)
        .unwrap_or_else(uuid::Uuid::nil)
}

pub fn active_native_kind(ro: &RealObject) -> Option<ObjectKind> {
    match ro.kind {
        ObjectKind::Settings | ObjectKind::Files | ObjectKind::Calculator => Some(ro.kind),
        _ => None,
    }
}

pub fn handle_native_char(ro: &mut RealObject, ch: char) -> bool {
    match ro.kind {
        ObjectKind::Calculator => {
            let mut st: native_apps::calculator::CalcState = ro
                .meta
                .get("calc_state")
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            if native_apps::calculator_key(ch, &mut st) {
                ro.meta.insert("calc_state".into(), serde_json::to_string(&st).unwrap());
                native_apps::calculator::write_doc(&mut ro.document, &st);
                return true;
            }
        }
        _ => {}
    }
    false
}

pub fn handle_native_key(ro: &mut RealObject, action: crate::hotkeys::HotkeyAction) -> bool {
    match ro.kind {
        ObjectKind::Files => {
            let mut st: native_apps::files::FilesState = ro
                .meta
                .get("files_state")
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            match action {
                crate::hotkeys::HotkeyAction::ArrowUp => native_apps::files::move_selection(&mut st, -1),
                crate::hotkeys::HotkeyAction::ArrowDown => native_apps::files::move_selection(&mut st, 1),
                crate::hotkeys::HotkeyAction::Enter => {
                    let _ = native_apps::files_open_selected(&mut st);
                }
                crate::hotkeys::HotkeyAction::Backspace => {
                    native_apps::files_go_up(&mut st);
                }
                _ => return false,
            }
            ro.meta.insert("files_state".into(), serde_json::to_string(&st).unwrap());
            native_apps::files::write_doc(&mut ro.document, &st);
            true
        }
        ObjectKind::Settings => {
            let mut st: native_apps::settings::SettingsState = ro
                .meta
                .get("settings_state")
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            match action {
                crate::hotkeys::HotkeyAction::ArrowUp => native_apps::settings::move_selection(&mut st, -1),
                crate::hotkeys::HotkeyAction::ArrowDown => native_apps::settings::move_selection(&mut st, 1),
                crate::hotkeys::HotkeyAction::Enter | crate::hotkeys::HotkeyAction::ResetView => {
                    native_apps::settings_toggle(&mut st, st.selected);
                }
                _ => return false,
            }
            ro.meta
                .insert("settings_state".into(), serde_json::to_string(&st).unwrap());
            native_apps::settings::write_doc(&mut ro.document, &st);
            true
        }
        _ => false,
    }
}

pub fn persist_native(ro: &RealObject) -> RealObject {
    native_apps::persist_state(ro)
}
