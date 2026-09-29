//! # Горячие клавиши
//!
//! Все горячие клавиши ZUI-TAD Shell. Делятся на 4 группы:
//!
//!   1. Навигация по холсту (Space, F, +/-, 0).
//!   2. Управление VO (Del, Ctrl+D, Ctrl+C/V).
//!   3. Редактирование (Ctrl+Z/Y/S, Tab, Esc, стрелки).
//!   4. Запуск внешних приложений (Ctrl+T = терминал, Ctrl+B = браузер,
//!      Ctrl+E = редактор, Ctrl+F = файловый менеджер).

use winit::event::KeyEvent;
use winit::keyboard::{KeyCode, PhysicalKey, ModifiersState};

/// Действие, которое выполняет shell в ответ на клавишу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyAction {
    // Навигация
    ResetView,              // Space
    ZoomIn,                 // + / =
    ZoomOut,                // -
    ZoomReset,              // 0
    FocusNext,              // Tab
    FocusPrev,              // Shift+Tab
    Blur,                   // Esc
    FlyToFocused,           // F

    // VO
    DeleteVo,               // Delete
    DuplicateVo,            // Ctrl+D
    NewTextDoc,             // Ctrl+N
    NewMindmap,             // Ctrl+Shift+N
    NewTable,               // Ctrl+Alt+N

    // Редактирование
    Undo,                   // Ctrl+Z
    Redo,                   // Ctrl+Y or Ctrl+Shift+Z
    Save,                   // Ctrl+S
    ExportMarkdown,         // Ctrl+E
    Backspace,              // Backspace
    Delete,                 // Delete (внутри редактора)
    Enter,                  // Enter
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,

    // Внешние приложения (WM integration)
    LaunchTerminal,         // Ctrl+T
    LaunchBrowser,          // Ctrl+B
    LaunchEditor,           // Ctrl+Shift+E
    LaunchFiles,            // Ctrl+F
    LaunchCalc,             // Ctrl+Shift+C
    LaunchSettings,         // Ctrl+,

    // Системные
    Quit,                   // Ctrl+Q
    ToggleHelp,             // ?
    Screenshot,             // F12

    // DE
    SwitchWorkspace(u8),       // Ctrl+1..9
    MoveWindowToWorkspace(u8), // Ctrl+Shift+1..9
    ToggleLauncher,            // Ctrl+Space
    LauncherUp,                // Up (когда launcher открыт)
    LauncherDown,              // Down (когда launcher открыт)
    NextWorkspace,             // Ctrl+Tab
    PrevWorkspace,             // Ctrl+Shift+Tab
    ShowDesktop,               // Super+D
    Lock,                      // Ctrl+Alt+L

    // Обычный текстовый ввод
    InsertChar(char),
}

/// Разбор KeyEvent → HotkeyAction.
/// Учитывает модификаторы (Ctrl, Shift, Alt).
pub fn dispatch(key: &KeyEvent, mods: ModifiersState) -> Option<HotkeyAction> {
    if key.state != winit::event::ElementState::Pressed {
        return None;
    }
    let PhysicalKey::Code(code) = key.physical_key else { return None };
    let ctrl = mods.control_key();
    let shift = mods.shift_key();
    let alt = mods.alt_key();

    // Сначала проверяем комбинации с Ctrl.
    if ctrl {
        match code {
            KeyCode::KeyZ => return Some(if shift { HotkeyAction::Redo } else { HotkeyAction::Undo }),
            KeyCode::KeyY => return Some(HotkeyAction::Redo),
            KeyCode::KeyS => return Some(HotkeyAction::Save),
            KeyCode::KeyD => return Some(HotkeyAction::DuplicateVo),
            KeyCode::KeyN => return Some(if shift { HotkeyAction::NewMindmap } else if alt { HotkeyAction::NewTable } else { HotkeyAction::NewTextDoc }),
            KeyCode::KeyE => return Some(if shift { HotkeyAction::LaunchEditor } else { HotkeyAction::ExportMarkdown }),
            KeyCode::KeyT => return Some(HotkeyAction::LaunchTerminal),
            KeyCode::KeyB => return Some(HotkeyAction::LaunchBrowser),
            KeyCode::KeyF => return Some(HotkeyAction::LaunchFiles),
            KeyCode::Comma => return Some(HotkeyAction::LaunchSettings),
            KeyCode::KeyC => return Some(if shift { HotkeyAction::LaunchCalc } else { HotkeyAction::DuplicateVo }),
            KeyCode::KeyQ => return Some(HotkeyAction::Quit),
            KeyCode::KeyL if alt => return Some(HotkeyAction::Lock),
            KeyCode::Space => return Some(HotkeyAction::ToggleLauncher),
            KeyCode::Tab => return Some(if shift { HotkeyAction::PrevWorkspace } else { HotkeyAction::NextWorkspace }),
            // Ctrl+1..9 → workspace switch
            KeyCode::Digit1 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(0) } else { HotkeyAction::SwitchWorkspace(0) }),
            KeyCode::Digit2 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(1) } else { HotkeyAction::SwitchWorkspace(1) }),
            KeyCode::Digit3 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(2) } else { HotkeyAction::SwitchWorkspace(2) }),
            KeyCode::Digit4 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(3) } else { HotkeyAction::SwitchWorkspace(3) }),
            KeyCode::Digit5 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(4) } else { HotkeyAction::SwitchWorkspace(4) }),
            KeyCode::Digit6 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(5) } else { HotkeyAction::SwitchWorkspace(5) }),
            KeyCode::Digit7 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(6) } else { HotkeyAction::SwitchWorkspace(6) }),
            KeyCode::Digit8 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(7) } else { HotkeyAction::SwitchWorkspace(7) }),
            KeyCode::Digit9 => return Some(if shift { HotkeyAction::MoveWindowToWorkspace(8) } else { HotkeyAction::SwitchWorkspace(8) }),
            _ => {}
        }
    }

    // Без модификаторов.
    match code {
        KeyCode::Space => Some(HotkeyAction::ResetView),
        KeyCode::Escape => Some(HotkeyAction::Blur),
        KeyCode::Tab => Some(if shift { HotkeyAction::FocusPrev } else { HotkeyAction::FocusNext }),
        KeyCode::Enter => Some(HotkeyAction::Enter),
        KeyCode::ArrowUp => Some(HotkeyAction::LauncherUp),
        KeyCode::ArrowDown => Some(HotkeyAction::LauncherDown),
        KeyCode::ArrowLeft => Some(HotkeyAction::ArrowLeft),
        KeyCode::ArrowRight => Some(HotkeyAction::ArrowRight),
        KeyCode::Home => Some(HotkeyAction::Home),
        KeyCode::End => Some(HotkeyAction::End),
        KeyCode::Delete => Some(HotkeyAction::Delete),
        KeyCode::Backspace => Some(HotkeyAction::Backspace),
        KeyCode::Equal | KeyCode::NumpadAdd => Some(HotkeyAction::ZoomIn),
        KeyCode::Minus | KeyCode::NumpadSubtract => Some(HotkeyAction::ZoomOut),
        KeyCode::Digit0 | KeyCode::Numpad0 => Some(HotkeyAction::ZoomReset),
        KeyCode::KeyF => Some(HotkeyAction::FlyToFocused),
        KeyCode::F1 => Some(HotkeyAction::ToggleHelp),
        KeyCode::F12 => Some(HotkeyAction::Screenshot),
        KeyCode::Slash if shift => Some(HotkeyAction::ToggleHelp),
        _ => None,
    }
}

/// Таблица горячих клавиш для отображения в окне помощи (F1).
pub const HELP_TEXT: &str = "\
ZUI-TAD Shell DE — горячие клавиши

DESKTOP ENVIRONMENT
  Ctrl+1..9         Переключение на workspace 1-9
  Ctrl+Shift+1..9   Переместить окно на workspace
  Ctrl+Tab          Следующий workspace
  Ctrl+Shift+Tab    Предыдущий workspace
  Ctrl+Space        App launcher (поиск по .desktop)
  ↑/↓ в launcher    Выбор приложения
  Enter в launcher  Запуск приложения
  Esc в launcher    Закрыть launcher

НАВИГАЦИЯ ПО ХОЛСТУ
  Space             Сбросить камеру к началу workspace
  +/-               Зум к центру экрана
  0                 Зум 1:1
  F                 Перелететь к фокусному VO
  Tab / Shift+Tab   Следующий / предыдущий сегмент
  Esc               Снять фокус

VO (виртуальные объекты)
  Delete            Удалить VO под курсором
  Ctrl+D            Дублировать VO
  Ctrl+N            Создать текстовый документ
  Ctrl+Shift+N      Создать mindmap
  Ctrl+Alt+N        Создать таблицу

РЕДАКТИРОВАНИЕ
  Ctrl+Z / Ctrl+Y   Undo / Redo
  Ctrl+S            Сохранить в sled
  Ctrl+E            Экспорт активного RO в .md
  Backspace/Delete  Удалить символ
  Стрелки           Двигать курсор
  Home/End          В начало/конец строки
  Enter             Новый абзац

ИНТЕГРАЦИЯ С WM
  Ctrl+T            Запустить терминал
  Ctrl+B            Запустить браузер
  Ctrl+Shift+E      Запустить редактор кода
  Ctrl+F            Файловый менеджер
  Ctrl+Shift+C      Калькулятор

СИСТЕМА
  Ctrl+Q            Выход
  F1 / ?            Эта справка
  F12               Скриншот в data/screenshot.png

МЫШЬ
  Колесо            Зум к курсору
  Средняя + drag    Панорамирование
  ЛКМ               Фокус на VO / переход по порталу
  ЛКМ + drag        Перетаскивание VO
  Drag файла        Импорт .txt/.md/.png/.jpg
";

/// Список приложений WM по умолчанию.
/// Можно переопределить переменными окружения:
///   ZUI_TERMINAL=alacritty ZUI_BROWSER=firefox ZUI_EDITOR=code ZUI_FILES=thunar
pub struct WmApps {
    pub terminal: String,
    pub browser: String,
    pub editor: String,
    pub files: String,
    pub calc: String,
}

impl Default for WmApps {
    fn default() -> Self {
        Self {
            terminal: std::env::var("ZUI_TERMINAL").unwrap_or_else(|_| auto_detect("terminal")),
            browser:  std::env::var("ZUI_BROWSER").unwrap_or_else(|_| auto_detect("browser")),
            editor:   std::env::var("ZUI_EDITOR").unwrap_or_else(|_| auto_detect("editor")),
            files:    std::env::var("ZUI_FILES").unwrap_or_else(|_| auto_detect("files")),
            calc:     std::env::var("ZUI_CALC").unwrap_or_else(|_| auto_detect("calc")),
        }
    }
}

fn auto_detect(kind: &str) -> String {
    // Список кандидатов — берём первый существующий в PATH.
    let candidates: &[&str] = match kind {
        "terminal" => &["alacritty", "kitty", "gnome-terminal", "konsole", "xterm", "foot"],
        "browser"  => &["firefox", "chromium", "google-chrome", "brave", "qutebrowser"],
        "editor"   => &["code", "subl", "vim", "nvim", "emacs", "gedit"],
        "files"    => &["thunar", "nautilus", "dolphin", "ranger", "pcmanfm"],
        "calc"     => &["gnome-calculator", "kcalc", "qalc", "bc"],
        _ => &[],
    };
    for c in candidates {
        if which(c).is_some() {
            return c.to_string();
        }
    }
    match kind {
        "terminal" => "xterm".to_string(),
        "browser" => "firefox".to_string(),
        "editor" => "vim".to_string(),
        "files" => "ls".to_string(),
        "calc" => "bc".to_string(),
        _ => String::new(),
    }
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(cmd);
        if full.is_file() {
            return Some(full);
        }
    }
    None
}
