//! shell_shot — рендер мока оболочки в PNG (headless, CPU).
//!
//! Запуск:
//!   cargo run -p phosphor --bin shell_shot -- /tmp/shell.png rig       # с зерном
//!   cargo run -p phosphor --bin shell_shot -- /tmp/shell.png rig clean # без эффектов
//!
//! Рисует тот же контент, что и живое окно `zui-preview`, но статично: так
//! дизайн-язык можно смотреть глазами и складывать в docs/shots.

use anyhow::Result;
use phosphor::demo::{self, WinState};
use phosphor::fx::{crt, CrtParams};
use phosphor::hud::{self, Ecg, Toast};
use phosphor::menu::{self, MenuState};
use phosphor::panel::{draw_top_panel, PanelData};
use phosphor::theme::{Metrics, Mode, Palette};
use phosphor::widgets::Fonts;
use tiny_skia::Pixmap;

const W: u32 = 1920;
const H: u32 = 1080;

fn main() -> Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/shell.png".into());
    let mode = match std::env::args().nth(2).as_deref() {
        Some("signalis") => Mode::Signalis,
        Some("phosphor") => Mode::Phosphor,
        _ => Mode::Rig,
    };
    let variant = std::env::args().nth(3).unwrap_or_else(|| "grain".into());
    let clean = variant == "clean";

    let pal = Palette::of(mode);
    let m = Metrics::default();
    let fonts = Fonts::load_default()?;

    let mut pm = Pixmap::new(W, H).unwrap();
    demo::fill(&mut pm, pal.bg);

    // Статичная сетка холста (точки каждые 64px).
    grid_static(&mut pm, &pal);

    // Кластер из двух сцепленных окон.
    let a = (520.0, 430.0, 620.0, 420.0);
    let b = (1142.0, 430.0, 500.0, 420.0);
    demo::window_card(
        &mut pm,
        &fonts,
        &pal,
        &m,
        a,
        "terminal — build",
        "alacritty",
        WinState::Focused,
    );
    demo::window_card(
        &mut pm,
        &fonts,
        &pal,
        &m,
        b,
        "memory.rs",
        "zui-core",
        WinState::Live,
    );
    demo::cluster_bracket(
        &mut pm,
        &fonts,
        &pal,
        &m,
        (a.0, a.1, b.0 + b.2 - a.0, a.3),
        2,
    );

    // Suspended: место сохранено, приложение закрыто.
    demo::window_card(
        &mut pm,
        &fonts,
        &pal,
        &m,
        (560.0, 880.0, 460.0, 180.0),
        "browser — signalis wiki",
        "firefox",
        WinState::Suspended,
    );

    // PiP: pinned_to_screen, игнорирует камеру.
    demo::pip_card(&mut pm, &fonts, &pal, &m, (1500.0, 70.0, 360.0, 210.0));

    // Панель.
    let _ = draw_top_panel(
        &mut pm,
        W as f32,
        &fonts,
        &PanelData {
            title: "zui-tad",
            workspace: 3,
            canvas_pos: (120.0, -40.0),
            zoom: 1.25,
            windows: 7,
            clock: "21:47",
            date: "29 SEP",
            meters: [
                ("cpu", 0.37, false),
                ("ram", 0.62, false),
                ("vol", 0.80, false),
                ("bat", 0.12, true),
            ],
        },
        &pal,
        &m,
    );

    // RIG-модуль телеметрии.
    demo::telemetry_module(
        &mut pm,
        &fonts,
        &pal,
        &m,
        24.0,
        620.0,
        300.0,
        &[
            ("canvas", "∞"),
            ("windows", "7"),
            ("cluster", "2"),
            ("suspended", "1"),
            ("zoom", "1.25x"),
            ("fps", "60"),
        ],
        &[
            ("CPU", 0.37, false),
            ("RAM", 0.62, false),
            ("PWR", 0.12, true),
        ],
    );

    // Лаунчер.
    demo::launcher_overlay(
        &mut pm,
        &fonts,
        &pal,
        &m,
        (W as f32 - 560.0) * 0.5,
        110.0,
        560.0,
        "zui",
        &[
            ("zui-terminal", "terminal", true),
            ("zui-notes", "tad document", false),
            ("zui-files", "canvas objects", false),
            ("zui-settings", "shell config", false),
        ],
    );

    // HUD: миникарта, виталы, видоискатель, тост.
    let mut ecg = Ecg::new(160);
    for i in 0..160 {
        ecg.push_beat(i as f32 * 0.03, 64.0);
    }
    hud::minimap(
        &mut pm,
        &fonts,
        &pal,
        &m,
        (24.0, 886.0, 280.0, 170.0),
        (120.0, 80.0, 1520.0, 900.0),
        &[
            (120.0, 80.0, 620.0, 430.0, true),
            (740.0, 80.0, 520.0, 430.0, false),
            (620.0, 600.0, 480.0, 240.0, false),
        ],
        (0.0, 0.0, 1920.0, 1080.0),
    );
    hud::vitals(
        &mut pm, &fonts, &pal, &m, 1596.0, 268.0, 300.0, 68.0, &ecg, 64.0,
    );
    hud::viewport_frame(&mut pm, &pal, &m);
    hud::scan_beam(&mut pm, &pal, 0.42);
    let t = Toast::new("new window", "alacritty · 900×600 → canvas");
    let _ = hud::toast(&mut pm, &fonts, &pal, &m, W as f32, 40.0, &t);

    // OSD.
    let _ = phosphor::draw_osd(
        &mut pm, W as f32, H as f32, "volume", 0.62, "62%", &fonts, &pal, &m,
    );

    // Режимы UI: командное меню и справка (для доков и проверки глазами).
    if variant == "menu" {
        let st = MenuState::new();
        let _ = menu::draw(&mut pm, &fonts, &pal, &m, W as f32, H as f32, &st);
    } else if variant == "help" {
        let rows = menu::help_rows();
        menu::draw_help(&mut pm, &fonts, &pal, &m, W as f32, H as f32, &rows);
    }

    // Фосфор. `clean` — без дизеринга/зерна (для доков и диффов).
    let params = if clean {
        CrtParams::off()
    } else {
        CrtParams::from_palette(&pal, 1337)
    };
    crt(&mut pm, &params);
    pm.save_png(&out)?;
    println!(
        "saved {out}  mode={mode:?}  effects={}",
        if clean { "off" } else { "on" }
    );
    Ok(())
}

fn grid_static(pm: &mut Pixmap, pal: &Palette) {
    use tiny_skia::{Paint, Rect, Transform};
    let mut p = Paint::default();
    p.set_color_rgba8(pal.dim[0], pal.dim[1], pal.dim[2], 0x30);
    let mut y = 60.0;
    while y < H as f32 {
        let mut x = 20.0;
        while x < W as f32 {
            if let Some(r) = Rect::from_xywh(x, y, 1.0, 1.0) {
                pm.fill_rect(r, &p, Transform::identity(), None);
            }
            x += 64.0;
        }
        y += 64.0;
    }
}
