//! # Window Tracker
//!
//! Отслеживает соответствие xdg_toplevel ↔ VO на холсте.

use ahash::AHashMap;
use tad_core::{RoId, VoId};

pub struct WindowTracker {
    pub windows: AHashMap<u64, WindowEntry>,
    pub next_id: u64,
}

pub struct WindowEntry {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub ro_id: RoId,
    pub vo_id: VoId,
    pub workspace: u8,
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub size: (u32, u32),
}

impl WindowTracker {
    pub fn new() -> Self {
        Self {
            windows: AHashMap::new(),
            next_id: 1,
        }
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn add(
        &mut self,
        title: String,
        app_id: String,
        size: (u32, u32),
        ro_id: RoId,
        vo_id: VoId,
        workspace: u8,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.windows.insert(
            id,
            WindowEntry {
                id,
                title,
                app_id,
                ro_id,
                vo_id,
                workspace,
                minimized: false,
                maximized: false,
                fullscreen: false,
                size,
            },
        );
        id
    }

    pub fn remove(&mut self, id: u64) -> Option<WindowEntry> {
        self.windows.remove(&id)
    }

    pub fn get(&self, id: u64) -> Option<&WindowEntry> {
        self.windows.get(&id)
    }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut WindowEntry> {
        self.windows.get_mut(&id)
    }

    pub fn list_on_workspace(&self, ws: u8) -> Vec<u64> {
        self.windows
            .iter()
            .filter(|(_, e)| e.workspace == ws && !e.minimized)
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn titles_on_workspace(&self, ws: u8) -> Vec<(u64, String)> {
        self.windows
            .iter()
            .filter(|(_, e)| e.workspace == ws)
            .map(|(id, e)| (*id, e.title.clone()))
            .collect()
    }
}
