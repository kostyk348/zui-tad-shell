//! # Phosphor — визуальный язык оболочки ZUI-TAD
//!
//! Арт-дирекшн: **фосфорный монохром** — Dead Space RIG (оранжевый сигнал,
//! сегментированные индикаторы, техническая разметка) × Signalis (амбер CRT,
//! сканлайны, дизеринг, пыльный фосфор). Никакого стекла, скруглений и
//! «современных» градиентов: только линии, капслок с разрядкой и свечение.
//!
//! Крейт полностью CPU-рендер (tiny-skia) и не знает про Wayland — поэтому
//! панель/OSD/лок можно снять в PNG и посмотреть, а логику покрыть тестами.

pub mod bg;
pub mod config;
pub mod demo;
pub mod fx;
pub mod hud;
pub mod icons;
pub mod menu;
pub mod osd;
pub mod panel;
pub mod texture;
pub mod theme;
pub mod widgets;

pub use bg::{render as render_bg, BgKind};
pub use fx::{crt, CrtParams};
pub use hud::{Ecg, Toast};
pub use osd::draw_osd;
pub use panel::{draw_top_panel, PanelData, PanelLayout};
pub use theme::{Metrics, Mode, Palette};
pub use widgets::{bar_cells, data_row, frame, hairline, label, text_width, tick_row};
