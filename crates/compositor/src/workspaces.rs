//! # Workspace Manager
//!
//! 9 рабочих столов (как в i3/sway). Каждый workspace — отдельный экран
//! бесконечного холста с собственной позицией камеры.

use cgmath::Point2;

pub struct WorkspaceManager {
    pub count: u8,
    pub workspaces: Vec<Workspace>,
    pub current: u8,
}

pub struct Workspace {
    pub id: u8,
    pub name: String,
    pub camera_center: Point2<f32>,
    pub camera_zoom: f32,
}

impl WorkspaceManager {
    pub fn new(count: u8) -> Self {
        let workspaces: Vec<Workspace> = (0..count)
            .map(|i| Workspace {
                id: i,
                name: (i + 1).to_string(),
                camera_center: Point2::new(800.0 + (i as f32) * 5000.0, 500.0),
                camera_zoom: 0.5,
            })
            .collect();
        Self { count, workspaces, current: 0 }
    }

    pub fn switch_to(&mut self, id: u8) -> Option<&Workspace> {
        if id >= self.count { return None; }
        self.current = id;
        self.workspaces.get(id as usize)
    }

    pub fn current(&self) -> &Workspace {
        &self.workspaces[self.current as usize]
    }

    pub fn next(&mut self) -> &Workspace {
        let next = (self.current + 1) % self.count;
        self.switch_to(next).unwrap()
    }

    pub fn prev(&mut self) -> &Workspace {
        let prev = if self.current == 0 { self.count - 1 } else { self.current - 1 };
        self.switch_to(prev).unwrap()
    }

    pub fn save_camera(&mut self, center: Point2<f32>, zoom: f32) {
        let ws = &mut self.workspaces[self.current as usize];
        ws.camera_center = center;
        ws.camera_zoom = zoom;
    }
}
