//! # App Launcher (Ctrl+Space)
//!
//! Поиск по .desktop файлам в /usr/share/applications/ и ~/.local/share/applications/.
//! Запускает выбранное приложение как Wayland-клиента нашего композитора.

use std::path::PathBuf;
use std::process::Command;

pub struct LauncherEntry {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub desktop_file: PathBuf,
    pub terminal: bool,
}

pub struct Launcher {
    pub entries: Vec<LauncherEntry>,
    pub filtered: Vec<usize>,
}

impl Launcher {
    pub fn load_all() -> Self {
        let mut entries = Vec::new();
        let dirs = [
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".local/share/applications")).unwrap_or_default(),
        ];
        for dir in dirs {
            if !dir.exists() { continue; }
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for entry in rd.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("desktop") { continue; }
                    if let Some(entry) = parse_desktop(&path) {
                        entries.push(entry);
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let filtered = (0..entries.len()).collect();
        Self { entries, filtered }
    }

    pub fn search(&mut self, query: &str) {
        let q = query.to_lowercase();
        self.filtered = self.entries.iter().enumerate()
            .filter(|(_, e)| {
                if q.is_empty() { return true; }
                e.name.to_lowercase().contains(&q) || e.exec.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .take(20)
            .collect();
    }

    pub fn launch(&self, idx: usize) -> std::io::Result<()> {
        let entry = &self.entries[idx];
        // Очищаем Exec от %f, %u, %U плейсхолдеров.
        let cmd = entry.exec
            .replace("%f", "")
            .replace("%F", "")
            .replace("%u", "")
            .replace("%U", "")
            .replace("%i", "")
            .replace("%c", "")
            .trim()
            .to_string();
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() { return Ok(()); }

        if entry.terminal {
            // Запускаем в терминале.
            let term = std::env::var("ZUI_TERMINAL").unwrap_or_else(|_| "xterm".into());
            Command::new(term).arg("-e").arg(&parts[0]).args(&parts[1..]).spawn()?;
        } else {
            Command::new(parts[0]).args(&parts[1..]).spawn()?;
        }
        Ok(())
    }
}

fn parse_desktop(path: &PathBuf) -> Option<LauncherEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut terminal = false;
    let mut no_display = false;
    for line in content.lines() {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry { continue; }
        if let Some(v) = line.strip_prefix("Name=") { name = Some(v.to_string()); }
        else if let Some(v) = line.strip_prefix("Exec=") { exec = Some(v.to_string()); }
        else if let Some(v) = line.strip_prefix("Icon=") { icon = Some(v.to_string()); }
        else if line == "Terminal=true" { terminal = true; }
        else if line == "NoDisplay=true" { no_display = true; }
    }
    if no_display { return None; }
    let name = name?;
    let exec = exec?;
    Some(LauncherEntry { name, exec, icon, desktop_file: path.clone(), terminal })
}
