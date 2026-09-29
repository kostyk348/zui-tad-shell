//! Native settings TAD document.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tad_core::{ObjectKind, RealObject, Segment, TadDocument};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingRow {
    pub label: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsState {
    pub rows: Vec<SettingRow>,
    pub selected: usize,
}

pub fn build_ro() -> RealObject {
    let st = default_state();
    let mut ro = RealObject {
        id: Uuid::new_v4(),
        title: "Settings".into(),
        kind: ObjectKind::Settings,
        document: TadDocument::new(),
        meta: Default::default(),
        doc_size: (480.0, 400.0),
        created_at: now(),
        updated_at: now(),
    };
    write_doc(&mut ro.document, &st);
    ro.meta
        .insert("settings_state".into(), serde_json::to_string(&st).unwrap());
    ro
}

fn default_state() -> SettingsState {
    SettingsState {
        rows: vec![
            SettingRow {
                label: "Semantic LOD (auto detail)".into(),
                value: "on".into(),
                enabled: true,
            },
            SettingRow {
                label: "Portal animations".into(),
                value: "on".into(),
                enabled: true,
            },
            SettingRow {
                label: "Grid background".into(),
                value: "on".into(),
                enabled: true,
            },
            SettingRow {
                label: "Native apps only (no external WM)".into(),
                value: "off".into(),
                enabled: false,
            },
            SettingRow {
                label: "Save on focus blur".into(),
                value: "on".into(),
                enabled: true,
            },
        ],
        selected: 0,
    }
}

pub fn write_doc(doc: &mut TadDocument, st: &SettingsState) {
    doc.root_segments.clear();
    doc.push(
        Segment::Heading {
            level: 1,
            text: "ZUI-TAD Settings".into(),
        },
        10.0,
        10.0,
        460.0,
        32.0,
    );
    doc.push(
        Segment::Text {
            text: "Space/Enter toggles row. Arrow keys move selection.".into(),
        },
        10.0,
        48.0,
        460.0,
        24.0,
    );
    let mut rows = vec![vec!["Setting".into(), "Value".into()]];
    for (i, r) in st.rows.iter().enumerate() {
        let mark = if i == st.selected { "▸ " } else { "  " };
        rows.push(vec![
            format!("{mark}{}", r.label),
            if r.enabled {
                r.value.clone()
            } else {
                "off".into()
            },
        ]);
    }
    doc.push(Segment::Table { rows }, 10.0, 80.0, 460.0, 300.0);
}

pub fn toggle_row(st: &mut SettingsState, row: usize) {
    if let Some(r) = st.rows.get_mut(row) {
        r.enabled = !r.enabled;
        r.value = if r.enabled { "on".into() } else { "off".into() };
    }
}

pub fn move_selection(st: &mut SettingsState, delta: i32) {
    if st.rows.is_empty() {
        return;
    }
    let n = st.rows.len() as i32;
    st.selected = (st.selected as i32 + delta).rem_euclid(n) as usize;
}

pub fn init_meta(ro: &mut RealObject) {
    if !ro.meta.contains_key("settings_state") {
        let st = default_state();
        ro.meta
            .insert("settings_state".into(), serde_json::to_string(&st).unwrap());
    }
}

pub fn persist_meta(ro: &mut RealObject) {
    if let Some(s) = ro.meta.get("settings_state") {
        if let Ok(st) = serde_json::from_str::<SettingsState>(s) {
            write_doc(&mut ro.document, &st);
        }
    }
    ro.updated_at = now();
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
