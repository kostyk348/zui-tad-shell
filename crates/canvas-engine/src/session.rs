//! Сессия холста: сохранение и восстановление раскладки.
//!
//! Зачем: бесконечный холст — это место, где у пользователя живёт работа.
//! Перезапуск не должен её терять. Как в driftwm: восстанавливаем **dormant** —
//! окна возвращаются как плейсхолдеры на своих местах, ничего не запускается
//! само; камера, кластеры, закладки и якоря возвращаются как были.
//!
//! Формат — JSON с версией. Неизвестные поля не ломают загрузку, чужая версия
//! помечается и пропускается: файл сессии не должен ронять сессию.

use cgmath::{Point2, Vector2};
use serde::{Deserialize, Serialize};

use crate::scene::{Place, Scene, WindowId};

pub const SESSION_VERSION: u32 = 1;

fn default_version() -> u32 {
    SESSION_VERSION
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowRecord {
    pub app_id: String,
    pub title: String,
    pub pos: (f32, f32),
    pub size: (f32, f32),
    pub place: Place,
    #[serde(default)]
    pub suspended: bool,
    /// id кластера внутри сессии (0 = без кластера).
    #[serde(default)]
    pub cluster: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    #[serde(default = "default_version")]
    pub version: u32,
    /// (center.x, center.y, zoom)
    pub camera: (f32, f32, f32),
    pub windows: Vec<WindowRecord>,
    #[serde(default)]
    pub bookmarks: Vec<Option<(f32, f32)>>,
    #[serde(default)]
    pub anchors: Vec<(String, (f32, f32))>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: SESSION_VERSION,
            camera: (0.0, 0.0, 1.0),
            windows: Vec::new(),
            bookmarks: Vec::new(),
            anchors: Vec::new(),
        }
    }
}

/// Что вернулось из сессии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreReport {
    pub windows: usize,
    pub clusters: usize,
    pub skipped: bool,
}

impl Session {
    /// Снять снимок сцены. `suspended` ставится всем НЕживым записям: при
    /// восстановлении мы не знаем, кто из окон ещё жив.
    pub fn from_scene(scene: &Scene, camera_center: Point2<f32>, zoom: f32) -> Self {
        let mut ids: Vec<WindowId> = scene.windows().iter().map(|w| w.id).collect();
        ids.sort_unstable();
        // Кластеры переупаковываем в плотные номера 1..N (внутренние id не важны).
        let mut cluster_map = std::collections::HashMap::new();
        let mut next = 1u64;
        let windows = ids
            .iter()
            .filter_map(|id| scene.get(*id))
            .map(|w| {
                let cluster = w
                    .cluster
                    .map(|c| {
                        *cluster_map.entry(c).or_insert_with(|| {
                            let v = next;
                            next += 1;
                            v
                        })
                    })
                    .unwrap_or(0);
                WindowRecord {
                    app_id: w.app_id.clone(),
                    title: w.title.clone(),
                    pos: (w.pos.x, w.pos.y),
                    size: (w.size.x, w.size.y),
                    place: w.place,
                    suspended: w.suspended,
                    cluster,
                }
            })
            .collect();

        let mut bookmarks: Vec<Option<(f32, f32)>> = scene
            .bookmarks
            .iter()
            .map(|b| b.map(|p| (p.x, p.y)))
            .collect();
        if bookmarks.is_empty() {
            bookmarks = vec![None; 4];
        }

        Self {
            version: SESSION_VERSION,
            camera: (camera_center.x, camera_center.y, zoom),
            windows,
            bookmarks,
            anchors: scene
                .anchors
                .iter()
                .map(|(n, p)| (n.clone(), (p.x, p.y)))
                .collect(),
        }
    }

    /// Восстановить сцену: сцена очищается, окна приходят как плейсхолдеры
    /// (suspended), кластеры и камера возвращаются.
    pub fn apply(&self, scene: &mut Scene) -> RestoreReport {
        scene.clear();
        let mut cluster_groups: std::collections::HashMap<u64, Vec<WindowId>> =
            std::collections::HashMap::new();

        for rec in &self.windows {
            let id = scene.insert(
                rec.app_id.clone(),
                rec.title.clone(),
                Point2::new(rec.pos.0, rec.pos.1),
                Vector2::new(rec.size.0.max(1.0), rec.size.1.max(1.0)),
                rec.place,
            );
            // Восстановленное окно всегда dormant: приложений мы не запускаем.
            scene.suspend(id);
            if rec.cluster != 0 {
                cluster_groups.entry(rec.cluster).or_default().push(id);
            }
        }

        let mut clusters = 0;
        for (_, ids) in cluster_groups {
            if ids.len() >= 2 && scene.group(&ids).is_some() {
                clusters += 1;
            }
        }

        for (slot, b) in self.bookmarks.iter().enumerate() {
            if let Some((x, y)) = b {
                scene.set_bookmark(slot, Point2::new(*x, *y));
            }
        }
        for (name, (x, y)) in &self.anchors {
            scene.add_anchor(name.clone(), Point2::new(*x, *y));
        }

        RestoreReport {
            windows: self.windows.len(),
            clusters,
            skipped: false,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Разбор JSON: чужая версия/мусор — `None` (сессию просто не грузим).
    pub fn from_json(s: &str) -> Option<Self> {
        let s: Session = serde_json::from_str(s).ok()?;
        if s.version > SESSION_VERSION {
            return None;
        }
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_with_windows() -> (Scene, WindowId, WindowId, WindowId) {
        let mut s = Scene::new();
        let a = s.insert(
            "alacritty",
            "term",
            Point2::new(10.0, 20.0),
            Vector2::new(600.0, 400.0),
            Place::Normal,
        );
        let b = s.insert(
            "firefox",
            "web",
            Point2::new(700.0, 20.0),
            Vector2::new(500.0, 400.0),
            Place::Normal,
        );
        let c = s.insert(
            "mpv",
            "pip",
            Point2::new(40.0, 40.0),
            Vector2::new(320.0, 180.0),
            Place::PinnedToScreen,
        );
        s.suspend(b);
        s.group(&[a, b]);
        (s, a, b, c)
    }

    #[test]
    fn roundtrip_preserves_layout_and_grouping() {
        let (mut scene, a, b, c) = scene_with_windows();
        scene.set_bookmark(1, Point2::new(100.0, 200.0));
        scene.add_anchor("home", Point2::new(5.0, 6.0));
        let snap = Session::from_scene(&scene, Point2::new(50.0, 60.0), 1.5);

        let mut restored = Scene::new();
        let rep = snap.apply(&mut restored);
        assert_eq!(rep.windows, 3);
        assert_eq!(rep.clusters, 1, "кластер из двух окон восстановлен");
        assert_eq!(restored.len(), 3);

        // позиции/размеры/тип
        let wa = restored.get(a).unwrap();
        assert!((wa.pos.x - 10.0).abs() < 0.01 && (wa.size.x - 600.0).abs() < 0.01);
        assert_eq!(restored.get(c).unwrap().place, Place::PinnedToScreen);
        // восстановленные окна dormant
        assert!(restored.windows().iter().all(|w| w.suspended));
        assert_eq!(restored.clusters().len(), 1);
        // окна кластера получили один id
        assert_eq!(
            restored.get(a).unwrap().cluster,
            restored.get(b).unwrap().cluster
        );
        // закладки и якоря
        assert_eq!(restored.bookmark(1), Some(Point2::new(100.0, 200.0)));
        assert_eq!(restored.anchor("home"), Some(Point2::new(5.0, 6.0)));
    }

    #[test]
    fn json_roundtrip_is_stable() {
        let (scene, ..) = scene_with_windows();
        let snap = Session::from_scene(&scene, Point2::new(0.0, 0.0), 1.0);
        let json = snap.to_json();
        let back = Session::from_json(&json).expect("свой же JSON должен читаться");
        assert_eq!(back, snap);
    }

    #[test]
    fn future_version_is_refused_and_garbage_ignored() {
        let mut snap = Session::default();
        snap.version = SESSION_VERSION + 1;
        assert!(
            Session::from_json(&snap.to_json()).is_none(),
            "чужая версия — не грузим"
        );
        assert!(Session::from_json("{").is_none());
        assert!(Session::from_json("").is_none());
    }

    #[test]
    fn unknown_fields_do_not_break_loading() {
        let json = r#"{
            "version": 1,
            "camera": [0.0, 0.0, 1.0],
            "windows": [
                {"app_id":"x","title":"y","pos":[1.0,2.0],"size":[100.0,50.0],
                 "place":"Normal","cluster":0,"future_flag":true}
            ],
            "something_new": {"a": 1}
        }"#;
        let snap = Session::from_json(json).expect("неизвестные поля игнорируются");
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].place, Place::Normal);
    }

    #[test]
    fn empty_session_applies_cleanly() {
        let mut scene = Scene::new();
        scene.insert(
            "a",
            "A",
            Point2::new(0.0, 0.0),
            Vector2::new(10.0, 10.0),
            Place::Normal,
        );
        let rep = Session::default().apply(&mut scene);
        assert_eq!(rep.windows, 0);
        assert!(scene.is_empty(), "пустая сессия очищает холст");
    }

    #[test]
    fn session_of_empty_scene_has_no_windows() {
        let s = Session::from_scene(&Scene::new(), Point2::new(0.0, 0.0), 1.0);
        assert!(s.windows.is_empty());
        assert_eq!(s.bookmarks.len(), 4, "закладки всегда четыре слота");
    }
}
