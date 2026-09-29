//! Конфиг оболочки: `~/.config/zui-tad/shell.toml`.
//!
//! Удобство для долгой работы = твой выбор переживает перезапуск: тема,
//! режим качества, включённые HUD/виталы/развёртка, focus mode.
//! Неизвестные поля и отсутствие файла не ломают запуск (serde default).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_THEME: &str = "rig";
pub const DEFAULT_QUALITY: &str = "auto";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    /// rig | signalis | phosphor
    pub theme: String,
    /// auto | rich | lean
    pub quality: String,
    /// Тише HUD, больше холста (для долгих сессий).
    pub focus_mode: bool,
    pub hud: bool,
    pub vitals: bool,
    pub beam: bool,
    /// hull | starfield | crt | blueprint | off | путь к своей картинке
    pub bg: String,
    /// Показывать справку при старте (для новичков; по умолчанию выкл).
    pub help_on_start: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.into(),
            quality: DEFAULT_QUALITY.into(),
            focus_mode: false,
            hud: true,
            vitals: true,
            beam: true,
            bg: "hull".into(),
            help_on_start: false,
        }
    }
}

impl ShellConfig {
    /// `$XDG_CONFIG_HOME/zui-tad/shell.toml` (или `~/.config/...`).
    pub fn path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"));
        base.join("zui-tad").join("shell.toml")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    pub fn load_from(p: &Path) -> Self {
        match std::fs::read_to_string(p) {
            Ok(s) => toml::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, p: &Path) -> std::io::Result<()> {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = toml::to_string_pretty(self).unwrap_or_default();
        std::fs::write(p, s)
    }

    /// Нормализовать тему из конфига (незнакомое значение → дефолт).
    pub fn theme_or_default(&self) -> &str {
        match self.theme.as_str() {
            "rig" | "signalis" | "phosphor" => self.theme.as_str(),
            _ => DEFAULT_THEME,
        }
    }

    /// Нормализовать фон (kind/off/путь).
    pub fn bg_or_default(&self) -> String {
        let v = self.bg.trim();
        if v.is_empty() {
            return "hull".into();
        }
        match v {
            "hull" | "starfield" | "crt" | "blueprint" | "off" => v.to_string(),
            path => path.to_string(), // свой путь к картинке
        }
    }

    pub fn quality_or_default(&self) -> &str {
        match self.quality.as_str() {
            "auto" | "rich" | "lean" => self.quality.as_str(),
            _ => DEFAULT_QUALITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("zui-tad-test-{}-{}.toml", name, std::process::id()));
        p
    }

    #[test]
    fn roundtrip_preserves_every_field() {
        let p = tmp("roundtrip");
        let cfg = ShellConfig {
            theme: "signalis".into(),
            quality: "lean".into(),
            focus_mode: true,
            hud: false,
            vitals: true,
            beam: false,
            bg: "starfield".into(),
            help_on_start: true,
        };
        cfg.save_to(&p).unwrap();
        let back = ShellConfig::load_from(&p);
        assert_eq!(cfg, back);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let p = tmp("missing");
        let _ = std::fs::remove_file(&p);
        assert_eq!(ShellConfig::load_from(&p), ShellConfig::default());
    }

    #[test]
    fn partial_and_unknown_fields_do_not_break() {
        let p = tmp("partial");
        std::fs::write(&p, "theme = \"phosphor\"\nfuture_thing = 42\n").unwrap();
        let c = ShellConfig::load_from(&p);
        assert_eq!(c.theme, "phosphor");
        assert_eq!(c.quality, DEFAULT_QUALITY, "остальное — дефолты");
        assert!(c.hud);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let p = tmp("corrupt");
        std::fs::write(&p, "theme = [ this is not toml").unwrap();
        assert_eq!(ShellConfig::load_from(&p), ShellConfig::default());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn unknown_theme_value_normalises() {
        let c = ShellConfig {
            theme: "neon".into(),
            ..Default::default()
        };
        assert_eq!(c.theme_or_default(), DEFAULT_THEME);
        let q = ShellConfig {
            quality: "ultra".into(),
            ..Default::default()
        };
        assert_eq!(q.quality_or_default(), DEFAULT_QUALITY);
    }

    #[test]
    fn bg_value_normalises() {
        let c = ShellConfig {
            bg: String::new(),
            ..Default::default()
        };
        assert_eq!(c.bg_or_default(), "hull");
        let c2 = ShellConfig {
            bg: "/home/x/wp.png".into(),
            ..Default::default()
        };
        assert_eq!(c2.bg_or_default(), "/home/x/wp.png");
        let c3 = ShellConfig {
            bg: "off".into(),
            ..Default::default()
        };
        assert_eq!(c3.bg_or_default(), "off");
    }

    #[test]
    fn path_is_inside_xdg_config_home() {
        let p = ShellConfig::path();
        let s = p.to_string_lossy();
        assert!(s.ends_with("zui-tad/shell.toml"), "путь: {s}");
    }
}
