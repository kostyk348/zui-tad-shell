//! # Input: горячие клавиши DE
//!
//!   Ctrl+1-9          — переключение workspace
//!   Ctrl+Shift+1-9    — переместить окно на workspace
//!   Ctrl+Space        — app launcher
//!   Ctrl+Alt+L        — lock screen
//!   Ctrl+Alt+Del      — выход из сессии
//!   Super+Enter       — терминал
//!   Super+D           — показать рабочий стол

use crate::state::CompositorState;

pub enum DeAction {
    SwitchWorkspace(u8),
    MoveWindowToWorkspace(u8),
    ToggleLauncher,
    Lock,
    Quit,
    LaunchTerminal,
    ShowDesktop,
    NextWorkspace,
    PrevWorkspace,
    None,
}

/// Обработка клавиши (keycode = x11 keysym).
pub fn handle_key(_state: &mut CompositorState, keycode: u32,
                  mods_ctrl: bool, mods_shift: bool, mods_alt: bool) -> DeAction {
    // Ctrl+1..9 → workspace (keysym 49..57 = '1'..'9')
    if mods_ctrl && !mods_alt {
        if (49..=57).contains(&keycode) {
            let ws = (keycode - 49) as u8;
            if mods_shift {
                return DeAction::MoveWindowToWorkspace(ws);
            } else {
                return DeAction::SwitchWorkspace(ws);
            }
        }
        if keycode == 32 { return DeAction::ToggleLauncher; } // Space
    }
    if mods_ctrl && mods_alt {
        if keycode == 108 { return DeAction::Lock; }       // L
        if keycode == 119 { return DeAction::Quit; }       // Delete
    }
    DeAction::None
}

pub fn apply_de_action(state: &mut CompositorState, action: DeAction) -> bool {
    match action {
        DeAction::SwitchWorkspace(id) => {
            state.workspaces.switch_to(id);
            state.current_workspace = id;
            tracing::info!("Workspace: {}", id + 1);
            true
        }
        DeAction::MoveWindowToWorkspace(id) => {
            if let Some((&id_win, _)) = state.windows.windows.iter().next() {
                if let Some(win) = state.windows.get_mut(id_win) {
                    win.workspace = id;
                    tracing::info!("Window {} → workspace {}", id_win, id + 1);
                }
            }
            true
        }
        DeAction::ToggleLauncher => {
            state.launcher_visible = !state.launcher_visible;
            state.launcher_query.clear();
            true
        }
        DeAction::NextWorkspace => {
            state.workspaces.next();
            true
        }
        DeAction::PrevWorkspace => {
            state.workspaces.prev();
            true
        }
        DeAction::Lock => {
            tracing::info!("Lock (не реализовано в прототипе)");
            true
        }
        DeAction::Quit => {
            tracing::info!("Quit DE");
            false
        }
        DeAction::LaunchTerminal => {
            let _ = std::process::Command::new(
                std::env::var("ZUI_TERMINAL").unwrap_or_else(|_| "xterm".into())
            ).spawn();
            true
        }
        DeAction::ShowDesktop => {
            for (_, win) in state.windows.windows.iter_mut() {
                win.minimized = !win.minimized;
            }
            true
        }
        DeAction::None => true,
    }
}
