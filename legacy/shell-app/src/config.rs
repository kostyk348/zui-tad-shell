//! # Конфигурация ZUI-TAD Shell
//!
//! Читает ~/.config/zui-tad/config.toml (если есть). Все поля опциональны —
//! при отсутствии используется значение по умолчанию.
//!
//! Пример:
//! ```toml
//! [launcher]
//! backend = "rofi"        # builtin | rofi | wofi | fuzzel | walker | tofi | dmenu
//! rofi_theme = "gruvbox-dark"
//!
//! [panel]
//! backend = "waybar"      # builtin | waybar | polybar | yambar | none
//! waybar_config = "~/.config/waybar/config"
//! waybar_style = "~/.config/waybar/style.css"
//!
//! [wallpaper]
//! backend = "swaybg"      # solid | swaybg | hyprpaper | wpaperd
//! color = "#1a1b20"
//! path = "~/Pictures/wallpaper.jpg"
//! mode = "fill"           # stretch | fit | fill | center | tile
//!
//! [locker]
//! backend = "swaylock"    # none | swaylock | hyprlock | waylock
//! color = "#000000"
//! image = "~/Pictures/lock.jpg"
//!
//! [notifications]
//! backend = "mako"        # none | mako | dunst | fnott
//! mako_config = "~/.config/mako/config"
//!
//! [clipboard]
//! backend = "wl-clipboard" # none | wl-clipboard
//!
//! [idle]
//! backend = "swayidle"    # none | swayidle | hypridle
//! timeout = 300
//! lock_cmd = "swaylock -f"
//!
//! [session]
//! polkit = true
//! xdg_portal = true
//! autostart_script = true
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::external_tools::{ToolsConfig, LauncherBackend, PanelBackend,
    WallpaperBackend, LockerBackend, NotifBackend, ClipboardBackend, IdleBackend};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub launcher: LauncherConfig,
    pub panel: PanelConfig,
    pub wallpaper: WallpaperConfig,
    pub locker: LockerConfig,
    pub notifications: NotificationsConfig,
    pub clipboard: ClipboardConfig,
    pub idle: IdleConfig,
    pub session: SessionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub backend: String,
    pub rofi_theme: Option<String>,
}
impl Default for LauncherConfig {
    fn default() -> Self { Self { backend: "builtin".into(), rofi_theme: None } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelConfig {
    pub backend: String,
    pub waybar_config: Option<String>,
    pub waybar_style: Option<String>,
}
impl Default for PanelConfig {
    fn default() -> Self { Self { backend: "builtin".into(), waybar_config: None, waybar_style: None } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperConfig {
    pub backend: String,
    pub color: Option<String>,
    pub path: Option<String>,
    pub mode: Option<String>,
}
impl Default for WallpaperConfig {
    fn default() -> Self { Self { backend: "solid".into(), color: Some("#1a1b20".into()), path: None, mode: None } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockerConfig {
    pub backend: String,
    pub color: Option<String>,
    pub image: Option<String>,
}
impl Default for LockerConfig {
    fn default() -> Self { Self { backend: "none".into(), color: None, image: None } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsConfig {
    pub backend: String,
    pub mako_config: Option<String>,
}
impl Default for NotificationsConfig {
    fn default() -> Self { Self { backend: "none".into(), mako_config: None } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardConfig {
    pub backend: String,
}
impl Default for ClipboardConfig {
    fn default() -> Self { Self { backend: "none".into() } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleConfig {
    pub backend: String,
    pub timeout: Option<u32>,
    pub lock_cmd: Option<String>,
}
impl Default for IdleConfig {
    fn default() -> Self { Self { backend: "none".into(), timeout: Some(300), lock_cmd: Some("swaylock -f".into()) } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub polkit: bool,
    pub xdg_portal: bool,
    pub autostart_script: bool,
}
impl Default for SessionConfig {
    fn default() -> Self { Self { polkit: true, xdg_portal: true, autostart_script: true } }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            tracing::info!("Конфиг не найден: {:?}, используем значения по умолчанию", path);
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("read {:?}", path))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("parse {:?}", path))?;
        tracing::info!("Конфиг загружен: {:?}", path);
        Ok(config)
    }

    pub fn config_path() -> PathBuf {
        let base = std::env::var("ZUI_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join(".config/zui-tad")
            });
        base.join("config.toml")
    }

    pub fn to_tools_config(&self) -> ToolsConfig {
        ToolsConfig {
            launcher: match self.launcher.backend.as_str() {
                "rofi" => LauncherBackend::Rofi { theme: self.launcher.rofi_theme.clone() },
                "wofi" => LauncherBackend::Wofi,
                "fuzzel" => LauncherBackend::Fuzzel,
                "walker" => LauncherBackend::Walker,
                "tofi" => LauncherBackend::Tofi,
                "dmenu" => LauncherBackend::Dmenu,
                _ => LauncherBackend::BuiltIn,
            },
            panel: match self.panel.backend.as_str() {
                "waybar" => PanelBackend::Waybar {
                    config: self.panel.waybar_config.clone(),
                    style: self.panel.waybar_style.clone(),
                },
                "polybar" => PanelBackend::Polybar,
                "yambar" => PanelBackend::Yambar,
                "none" => PanelBackend::None,
                _ => PanelBackend::BuiltIn,
            },
            wallpaper: match self.wallpaper.backend.as_str() {
                "swaybg" => WallpaperBackend::Swaybg {
                    path: self.wallpaper.path.clone().unwrap_or_default(),
                    mode: self.wallpaper.mode.clone().unwrap_or_else(|| "fill".into()),
                },
                "hyprpaper" => WallpaperBackend::Hyprpaper,
                "wpaperd" => WallpaperBackend::Wpaperd,
                _ => WallpaperBackend::SolidColor(parse_color(
                    self.wallpaper.color.as_deref().unwrap_or("#1a1b20")
                )),
            },
            locker: match self.locker.backend.as_str() {
                "swaylock" => LockerBackend::Swaylock {
                    color: self.locker.color.clone(),
                    image: self.locker.image.clone(),
                },
                "hyprlock" => LockerBackend::Hyprlock,
                "waylock" => LockerBackend::Waylock,
                _ => LockerBackend::None,
            },
            notifications: match self.notifications.backend.as_str() {
                "mako" => NotifBackend::Mako { config: self.notifications.mako_config.clone() },
                "dunst" => NotifBackend::Dunst,
                "fnott" => NotifBackend::Fnott,
                _ => NotifBackend::None,
            },
            clipboard: match self.clipboard.backend.as_str() {
                "wl-clipboard" => ClipboardBackend::WlClipboardWithCliphist,
                _ => ClipboardBackend::None,
            },
            idle: match self.idle.backend.as_str() {
                "swayidle" => IdleBackend::Swayidle {
                    timeout: self.idle.timeout.unwrap_or(300),
                    lock_cmd: self.idle.lock_cmd.clone().unwrap_or_else(|| "swaylock -f".into()),
                },
                "hypridle" => IdleBackend::Hypridle,
                _ => IdleBackend::None,
            },
            polkit: self.session.polkit,
            xdg_portal: self.session.xdg_portal,
        }
    }
}

fn parse_color(s: &str) -> [u8; 4] {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0x1a);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0x1b);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0x20);
        [r, g, b, 0xff]
    } else { [0x1a, 0x1b, 0x20, 0xff] }
}
