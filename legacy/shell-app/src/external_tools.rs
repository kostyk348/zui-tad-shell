//! # Интеграция с внешними DE-инструментами
//!
//! ZUI-TAD Shell не пытается изобрести всё заново. Для задач, где уже есть
//! отличный инструмент, мы его используем:
//!
//!   - **rofi / wofi / fuzzel / walker** — app launcher (вместо встроенного)
//!   - **waybar / polybar / yambar** — верхняя панель (вместо встроенной)
//!   - **swaybg / hyprpaper / wpaperd** — обои
//!   - **swaylock / hyprlock / waylock** — lock screen
//!   - **wl-clipboard / cliphist** — буфер обмена
//!   - **mako / dunst / fnott** — уведомления
//!   - **swayidle / hypridle** — idle management
//!
//! Все эти инструменты запускаются как дочерние процессы. Если они установлены
//! и включены в конфиге — ZUI-TAD использует их; если нет — fallback на
//! встроенные реализации.

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use parking_lot::Mutex;
use ahash::AHashMap;

pub struct ExternalTools {
    pub processes: Arc<Mutex<AHashMap<String, Child>>>,
    pub config: ToolsConfig,
}

#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub launcher: LauncherBackend,
    pub panel: PanelBackend,
    pub wallpaper: WallpaperBackend,
    pub locker: LockerBackend,
    pub notifications: NotifBackend,
    pub clipboard: ClipboardBackend,
    pub idle: IdleBackend,
    pub polkit: bool,
    pub xdg_portal: bool,
}

#[derive(Debug, Clone)]
pub enum LauncherBackend {
    BuiltIn,           // встроенный launcher
    Rofi { theme: Option<String> },
    Wofi,
    Fuzzel,
    Walker,
    Tofi,
    Dmenu,
}

#[derive(Debug, Clone)]
pub enum PanelBackend {
    BuiltIn,           // встроенная панель
    Waybar { config: Option<String>, style: Option<String> },
    Polybar,
    Yambar,
    None,
}

#[derive(Debug, Clone)]
pub enum WallpaperBackend {
    SolidColor([u8; 4]),
    Swaybg { path: String, mode: String },
    Hyprpaper,
    Wpaperd,
}

#[derive(Debug, Clone)]
pub enum LockerBackend {
    None,
    Swaylock { color: Option<String>, image: Option<String> },
    Hyprlock,
    Waylock,
}

#[derive(Debug, Clone)]
pub enum NotifBackend {
    None,
    Mako { config: Option<String> },
    Dunst,
    Fnott,
}

#[derive(Debug, Clone)]
pub enum ClipboardBackend {
    None,
    WlClipboard,
    WlClipboardWithCliphist,
}

#[derive(Debug, Clone)]
pub enum IdleBackend {
    None,
    Swayidle { timeout: u32, lock_cmd: String },
    Hypridle,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            launcher: LauncherBackend::BuiltIn,
            panel: PanelBackend::BuiltIn,
            wallpaper: WallpaperBackend::SolidColor([0x1a, 0x1b, 0x20, 0xff]),
            locker: LockerBackend::None,
            notifications: NotifBackend::None,
            clipboard: ClipboardBackend::None,
            idle: IdleBackend::None,
            polkit: true,
            xdg_portal: true,
        }
    }
}

impl ExternalTools {
    pub fn new(config: ToolsConfig) -> Self {
        Self {
            processes: Arc::new(Mutex::new(AHashMap::new())),
            config,
        }
    }

    /// Запустить все включённые в конфиге инструменты.
    /// Вызывается при старте сессии.
    pub fn start_all(&self) {
        self.start_wallpaper();
        self.start_panel();
        self.start_notifications();
        self.start_clipboard();
        self.start_idle();
        if self.config.polkit { self.start_polkit(); }
        if self.config.xdg_portal { self.start_xdg_portal(); }
    }

    /// Запустить app launcher.
    /// Возвращает выбранную пользователем команду (если внешний launcher).
    pub fn launch_app(&self) -> Option<String> {
        match &self.config.launcher {
            LauncherBackend::BuiltIn => None, // обрабатывается GUI
            LauncherBackend::Rofi { theme } => self.run_rofi(theme),
            LauncherBackend::Wofi => self.run_wofi(),
            LauncherBackend::Fuzzel => self.run_fuzzel(),
            LauncherBackend::Walker => self.run_walker(),
            LauncherBackend::Tofi => self.run_tofi(),
            LauncherBackend::Dmenu => self.run_dmenu(),
        }
    }

    fn start_wallpaper(&self) {
        match &self.config.wallpaper {
            WallpaperBackend::SolidColor(_) => {} // рисуем сами
            WallpaperBackend::Swaybg { path, mode } => {
                self.spawn("swaybg", vec![
                    "-i".into(), path.clone(),
                    "-m".into(), mode.clone(),
                ]);
            }
            WallpaperBackend::Hyprpaper => { self.spawn("hyprpaper", vec![]); }
            WallpaperBackend::Wpaperd => { self.spawn("wpaperd", vec![]); }
        }
    }

    fn start_panel(&self) {
        match &self.config.panel {
            PanelBackend::BuiltIn => {} // рисуем сами
            PanelBackend::Waybar { config, style } => {
                let mut args: Vec<String> = vec![];
                if let Some(c) = config { args.push("-c".into()); args.push(c.clone()); }
                if let Some(s) = style { args.push("-s".into()); args.push(s.clone()); }
                self.spawn("waybar", args);
            }
            PanelBackend::Polybar => { self.spawn("polybar", vec![]); }
            PanelBackend::Yambar => { self.spawn("yambar", vec![]); }
            PanelBackend::None => {}
        }
    }

    fn start_notifications(&self) {
        match &self.config.notifications {
            NotifBackend::None => {}
            NotifBackend::Mako { config } => {
                let mut args: Vec<String> = vec![];
                if let Some(c) = config { args.push("-c".into()); args.push(c.clone()); }
                self.spawn("mako", args);
            }
            NotifBackend::Dunst => { self.spawn("dunst", vec![]); }
            NotifBackend::Fnott => { self.spawn("fnott", vec![]); }
        }
    }

    fn start_clipboard(&self) {
        match &self.config.clipboard {
            ClipboardBackend::None => {}
            ClipboardBackend::WlClipboard => {
                self.spawn("wl-paste", vec!["--watch".into(), "cliphist".into(), "store".into()]);
            }
            ClipboardBackend::WlClipboardWithCliphist => {
                self.spawn("wl-paste", vec!["--watch".into(), "cliphist".into(), "store".into()]);
            }
        }
    }

    fn start_idle(&self) {
        match &self.config.idle {
            IdleBackend::None => {}
            IdleBackend::Swayidle { timeout, lock_cmd } => {
                let t = timeout.to_string();
                self.spawn("swayidle", vec![
                    "timeout".into(), t,
                    "lock".into(), lock_cmd.clone(),
                    "before-sleep".into(), lock_cmd.clone(),
                ]);
            }
            IdleBackend::Hypridle => { self.spawn("hypridle", vec![]); }
        }
    }

    fn start_polkit(&self) {
        let candidates = [
            "/usr/libexec/polkit-gnome-authentication-agent-1",
            "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1",
            "/usr/lib/x86_64-linux-gnu/polkit-mate/polkit-mate-authentication-agent-1",
            "lxpolkit",
            "polkit-kde-authentication-agent-1",
        ];
        for c in candidates {
            if std::path::Path::new(c).exists() || which(c).is_some() {
                self.spawn(c, vec![]);
                return;
            }
        }
    }

    fn start_xdg_portal(&self) {
        self.spawn("xdg-desktop-portal", vec![]);
        for c in &["xdg-desktop-portal-wlr", "xdg-desktop-portal-hyprland", "xdg-desktop-portal-gnome"] {
            if which(c).is_some() {
                self.spawn(c, vec![]);
                return;
            }
        }
    }

    /// Заблокировать экран.
    pub fn lock_screen(&self) {
        match &self.config.locker {
            LockerBackend::None => {}
            LockerBackend::Swaylock { color, image } => {
                let mut args = vec![];
                if let Some(c) = color { args.push("-c"); args.push(c); }
                if let Some(i) = image { args.push("-i"); args.push(i); }
                let _ = Command::new("swaylock").args(&args).spawn();
            }
            LockerBackend::Hyprlock => { let _ = Command::new("hyprlock").spawn(); }
            LockerBackend::Waylock => { let _ = Command::new("waylock").spawn(); }
        }
    }

    // ─── Внешние launcher'ы ──────────────────────────────────────────────

    fn run_rofi(&self, theme: &Option<String>) -> Option<String> {
        let mut cmd = Command::new("rofi");
        cmd.args(["-dmenu", "-i", "-p", "Run"]);
        if let Some(t) = theme { cmd.args(["-theme", t]); }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = cmd.spawn().ok()?;
        // Подаём список приложений.
        let apps = list_desktop_names();
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(apps.join("\n").as_bytes());
        }
        let output = child.wait_with_output().ok()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    fn run_wofi(&self) -> Option<String> {
        let mut cmd = Command::new("wofi");
        cmd.args(["--dmenu", "--prompt=Run"]);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = cmd.spawn().ok()?;
        let apps = list_desktop_names();
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(apps.join("\n").as_bytes());
        }
        let output = child.wait_with_output().ok()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    fn run_fuzzel(&self) -> Option<String> {
        let output = Command::new("fuzzel")
            .args(["--dmenu", "--prompt=Run"])
            .stdin(Stdio::piped()).stdout(Stdio::piped())
            .spawn().ok()?
            .wait_with_output().ok()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    fn run_walker(&self) -> Option<String> {
        let output = Command::new("walker").stdin(Stdio::piped()).stdout(Stdio::piped())
            .spawn().ok()?.wait_with_output().ok()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    fn run_tofi(&self) -> Option<String> {
        let output = Command::new("tofi")
            .stdin(Stdio::piped()).stdout(Stdio::piped())
            .spawn().ok()?.wait_with_output().ok()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    fn run_dmenu(&self) -> Option<String> {
        let output = Command::new("dmenu_run")
            .stdin(Stdio::piped()).stdout(Stdio::piped())
            .spawn().ok()?.wait_with_output().ok()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    fn spawn(&self, name: &str, args: Vec<String>) {
        if which(name).is_none() && !std::path::Path::new(name).exists() {
            tracing::warn!("Внешний инструмент не найден: {name}");
            return;
        }
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        match Command::new(name).args(&args_refs).spawn() {
            Ok(child) => {
                self.processes.lock().insert(name.to_string(), child);
                tracing::info!("Запущен внешний инструмент: {name}");
            }
            Err(e) => tracing::error!("Не удалось запустить {name}: {e}"),
        }
    }

    /// Завершить все дочерние процессы.
    pub fn kill_all(&self) {
        let mut guard = self.processes.lock();
        for (_, mut child) in guard.drain() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(cmd);
        if full.is_file() { return Some(full); }
    }
    None
}

fn list_desktop_names() -> Vec<String> {
    let mut out = Vec::new();
    for dir in &["/usr/share/applications", "/usr/local/share/applications"] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for line in content.lines() {
                        if let Some(name) = line.strip_prefix("Name=") {
                            out.push(name.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// Найти .desktop файл по имени приложения и вернуть его Exec.
pub fn find_exec_by_name(name: &str) -> Option<String> {
    for dir in &["/usr/share/applications", "/usr/local/share/applications"] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let mut file_name = None;
                    let mut file_exec = None;
                    for line in content.lines() {
                        if let Some(n) = line.strip_prefix("Name=") { file_name = Some(n.to_string()); }
                        else if let Some(e) = line.strip_prefix("Exec=") { file_exec = Some(e.to_string()); }
                    }
                    if file_name.as_deref() == Some(name) {
                        return file_exec;
                    }
                }
            }
        }
    }
    None
}
