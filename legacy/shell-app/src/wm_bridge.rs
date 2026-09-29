//! # Интеграция с оконным менеджером
//!
//! Запускает сторонние приложения (терминал, браузер, редактор) как отдельные
//! процессы и регистрирует их как VO типа `WaylandWindow` на холсте.
//!
//! В прототипе мы не перехватываем окно приложения как текстуру (для этого
//! нужен полноценный compositor через smithay/wlroots). Вместо этого:
//!   - Запускаем процесс.
//!   - Создаём VO-плейсхолдер с цветом, заголовком, app_id.
//!   - При двойном клике на такой VO — поднимаем окно процесса на передний план.
//!
//! Это даёт пользователю «запуск приложения с холста» без полного композитинга.

use ahash::AHashMap;
use cgmath::Point2;
use parking_lot::Mutex;
use std::process::{Child, Command};
use std::sync::Arc;
use tad_core::{ObjectKind, RealObject, RoId, VirtualObject};
use uuid::Uuid;

pub struct WmBridge {
    /// Запущенные процессы: VoId → (child, app_id).
    pub processes: Arc<Mutex<AHashMap<Uuid, RunningApp>>>,
    pub apps: crate::hotkeys::WmApps,
}

pub struct RunningApp {
    pub child: Child,
    pub app_id: String,
    pub title: String,
    pub ro_id: RoId,
}

impl WmBridge {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(AHashMap::new())),
            apps: crate::hotkeys::WmApps::default(),
        }
    }

    /// Запустить приложение и создать соответствующий RO + VO.
    /// Возвращает (RO, VO) для добавления в граф.
    pub fn launch(
        &self,
        kind: AppKind,
        pos: Point2<f32>,
        size: (f32, f32),
        store: &tad_core::GraphStore,
        blob_store: Option<&tad_core::BlobStore>,
    ) -> anyhow::Result<(RealObject, VirtualObject)> {
        let (binary, title, app_id) = match kind {
            AppKind::Terminal => (self.apps.terminal.clone(), "Terminal", "terminal"),
            AppKind::Browser  => (self.apps.browser.clone(),  "Browser",  "browser"),
            AppKind::Editor   => (self.apps.editor.clone(),   "Editor",   "editor"),
            AppKind::Files    => (self.apps.files.clone(),    "Files",    "files"),
            AppKind::Calc     => (self.apps.calc.clone(),     "Calc",     "calc"),
        };

        // 1. Запускаем процесс.
        let child = Command::new(&binary).spawn()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?.as_secs();

        // 2. Создаём RO типа WaylandWindow.
        let ro_id = Uuid::new_v4();
        let mut doc = tad_core::TadDocument::new();
        doc.push(tad_core::Segment::Text {
            text: format!("Запущен процесс: {} (PID {})", binary, child.id()),
        }, 20.0, 20.0, size.0 - 40.0, 30.0);
        doc.push(tad_core::Segment::Text {
            text: format!("app_id = {}", app_id),
        }, 20.0, 60.0, size.0 - 40.0, 24.0);
        doc.push(tad_core::Segment::Link {
            target_ro: ro_id,  // ссылка на самого себя — для визуального значка
            label: format!("{} — {}", app_id, title),
        }, 20.0, 100.0, size.0 - 40.0, 30.0);

        let ro = RealObject {
            id: ro_id,
            title: format!("{} — {}", title, binary),
            kind: ObjectKind::WaylandWindow,
            document: doc,
            meta: {
                let mut m = std::collections::HashMap::new();
                m.insert("app_id".to_string(), app_id.to_string());
                m.insert("binary".to_string(), binary.clone());
                m.insert("pid".to_string(), child.id().to_string());
                m
            },
            doc_size: size,
            created_at: now,
            updated_at: now,
        };
        store.put_ro(&ro)?;

        // 3. Создаём VO на холсте (родитель — Desktop, ищем первый Folder).
        let desktop_ro = store.all_ro()?.into_iter()
            .find(|r| r.kind == ObjectKind::Folder)
            .map(|r| r.id)
            .ok_or_else(|| anyhow::anyhow!("Desktop folder not found"))?;

        let vo = VirtualObject::new(ro_id, desktop_ro, pos,
            cgmath::Vector2::new(size.0, size.1));
        store.put_vo(&vo)?;

        // 4. Регистрируем процесс.
        self.processes.lock().insert(vo.id, RunningApp {
            child,
            app_id: app_id.to_string(),
            title: title.to_string(),
            ro_id,
        });

        let _ = blob_store;
        Ok((ro, vo))
    }

    /// Список всех запущенных приложений.
    pub fn list(&self) -> Vec<(Uuid, String, String)> {
        self.processes.lock().iter()
            .map(|(id, app)| (*id, app.app_id.clone(), app.title.clone()))
            .collect()
    }

    /// Поднять окно приложения на передний план (через wmctrl, если доступен).
    pub fn focus_app(&self, vo_id: Uuid) -> anyhow::Result<()> {
        let guard = self.processes.lock();
        let Some(app) = guard.get(&vo_id) else { return Ok(()); };
        // Пробуем wmctrl -a <title>, если установлен.
        if std::process::Command::new("wmctrl").arg("-a").arg(&app.title).output().is_ok() {
            return Ok(());
        }
        // Альтернатива: xdotool search --name <title> windowactivate.
        let _ = std::process::Command::new("xdotool")
            .args(["search", "--name", &app.title, "windowactivate", "--sync"])
            .output();
        Ok(())
    }

    /// Завершить все процессы при выходе.
    pub fn kill_all(&self) {
        let mut guard = self.processes.lock();
        for (_, mut app) in guard.drain() {
            let _ = app.child.kill();
            let _ = app.child.wait();
        }
    }

    /// Проверить, какие процессы ещё живы; убрать мёртвые из реестра.
    pub fn reap_dead(&self) {
        let mut guard = self.processes.lock();
        let mut dead = Vec::new();
        for (id, app) in guard.iter_mut() {
            match app.child.try_wait() {
                Ok(Some(_)) => dead.push(*id),
                _ => {}
            }
        }
        for id in dead {
            guard.remove(&id);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AppKind {
    Terminal,
    Browser,
    Editor,
    Files,
    Calc,
}
