//! Native file browser TAD document.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tad_core::{ObjectKind, RealObject, Segment, TadDocument};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesState {
    pub cwd: PathBuf,
    pub selected: usize,
}

impl Default for FilesState {
    fn default() -> Self {
        Self {
            cwd: std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(".")),
            selected: 0,
        }
    }
}

pub fn build_ro() -> RealObject {
    let st = FilesState::default();
    let mut ro = RealObject {
        id: Uuid::new_v4(),
        title: "Files".into(),
        kind: ObjectKind::Files,
        document: TadDocument::new(),
        meta: Default::default(),
        doc_size: (520.0, 480.0),
        created_at: now(),
        updated_at: now(),
    };
    write_doc(&mut ro.document, &st);
    ro.meta
        .insert("files_state".into(), serde_json::to_string(&st).unwrap());
    ro
}

pub fn write_doc(doc: &mut TadDocument, st: &FilesState) {
    doc.root_segments.clear();
    doc.push(
        Segment::Heading {
            level: 1,
            text: "Files".into(),
        },
        10.0,
        10.0,
        500.0,
        32.0,
    );
    doc.push(
        Segment::Text {
            text: format!("Path: {}", st.cwd.display()),
        },
        10.0,
        48.0,
        500.0,
        24.0,
    );

    let mut rows = vec![vec!["Name".into(), "Type".into()]];
    rows.push(vec!["..".into(), "dir".into()]);
    if let Ok(rd) = std::fs::read_dir(&st.cwd) {
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            let ty = if e.path().is_dir() { "dir" } else { "file" };
            rows.push(vec![name, ty.into()]);
        }
    }
    doc.push(Segment::Table { rows }, 10.0, 80.0, 500.0, 380.0);
}

pub fn go_up(st: &mut FilesState) {
    if st.cwd.parent().is_some() {
        st.cwd.pop();
        st.selected = 0;
    }
}

pub fn open_selected(st: &mut FilesState) -> Option<PathBuf> {
    let entries = list_entries(&st.cwd);
    let idx = st.selected;
    if idx == 0 {
        go_up(st);
        return None;
    }
    let path = entries.get(idx - 1)?.clone();
    if path.is_dir() {
        st.cwd = path;
        st.selected = 0;
        None
    } else {
        Some(path)
    }
}

pub fn move_selection(st: &mut FilesState, delta: i32) {
    let count = list_entries(&st.cwd).len() + 1; // + ..
    if count == 0 {
        return;
    }
    let next = (st.selected as i32 + delta).rem_euclid(count as i32) as usize;
    st.selected = next;
}

fn list_entries(cwd: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(cwd)
        .map(|rd| {
            let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            v.sort();
            v
        })
        .unwrap_or_default()
}

pub fn init_meta(ro: &mut RealObject) {
    if !ro.meta.contains_key("files_state") {
        let st = FilesState::default();
        ro.meta
            .insert("files_state".into(), serde_json::to_string(&st).unwrap());
    }
}

pub fn persist_meta(ro: &mut RealObject) {
    if let Some(s) = ro.meta.get("files_state") {
        if let Ok(st) = serde_json::from_str::<FilesState>(s) {
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
