//! # XDG Apps scanner
//!
//! Парсит /usr/share/applications/*.desktop для launcher.

pub fn list_applications() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let dirs = [
        "/usr/share/applications",
        "/usr/local/share/applications",
    ];
    for dir in dirs {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let mut name = None;
                    let mut exec = None;
                    let mut no_display = false;
                    for line in content.lines() {
                        if line.starts_with("Name=") { name = line.strip_prefix("Name=").map(String::from); }
                        else if line.starts_with("Exec=") { exec = line.strip_prefix("Exec=").map(String::from); }
                        else if line == "NoDisplay=true" { no_display = true; }
                    }
                    if no_display { continue; }
                    if let (Some(n), Some(e)) = (name, exec) {
                        out.push((n, e));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}
