//! Winit + GLES backend: canvas render loop + input.
//!
//! Ключевая идея: мы НЕ раскладываем окна по экрану — мы показываем камеру,
//! смотрящую на бесконечный холст. Каждый кадр:
//!   1. `layout_windows` превращает `Scene` в экранные квады (culling + zoom);
//!   2. каждое окно рисуется `render_elements_from_surface_tree` в позицию квада
//!      с масштабом `zoom` (так зум работает без пере-конфигурации клиентов);
//!   3. ввод переводится в мировые координаты и работает через `Scene`/`Interact`.
//!
//! Здесь нет DRM/TTY: это вложенный (nested) бэкенд для разработки.

use crate::state::{ClientData, CompositorState};
use ::winit::window::CursorIcon;
use anyhow::Result;
use canvas_engine::{layout_windows, Dir, Mode, ResizeHandle, WindowId};
use cgmath::Point2;
use parking_lot::Mutex;
use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, Axis, ButtonState, InputEvent, KeyState, KeyboardKeyEvent,
            PointerAxisEvent, PointerButtonEvent,
        },
        renderer::{
            element::{
                surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
                Kind,
            },
            gles::GlesRenderer,
            utils::draw_render_elements,
            Color32F, Frame, Renderer,
        },
        winit::{self, WinitEvent},
    },
    input::{
        keyboard::{keysyms, FilterResult, KeyboardHandle},
        pointer::{ButtonEvent, MotionEvent, PointerHandle},
    },
    reexports::wayland_server::Display,
    utils::{Logical, Point, Rectangle, Size, Transform},
    wayland::{
        compositor::{with_surface_tree_downward, SurfaceAttributes, TraversalAction},
        seat::WaylandFocus,
    },
};
use std::sync::Arc;
use tad_core::GraphStore;
use tracing_subscriber::EnvFilter;
use wayland_server::{protocol::wl_surface::WlSurface, ListeningSocket};

/// Сколько пикселей прилипания в МИРОВЫХ координатах (постоянно на экране).
const SNAP_PX: f32 = 26.0;
/// Толщина зоны ресайза у края окна (в ЭКРАННЫХ px).
const RESIZE_MARGIN_PX: f32 = 9.0;
/// Минимальный размер окна на холсте.
const MIN_WINDOW: cgmath::Vector2<f32> = cgmath::Vector2::new(120.0, 80.0);

pub fn run_compositor(store: Arc<Mutex<GraphStore>>, _embedded: bool) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("compositor=info".parse()?))
        .try_init()
        .ok();

    run_winit_backend(store)
}

fn run_winit_backend(store: Arc<Mutex<GraphStore>>) -> Result<()> {
    let mut display: Display<CompositorState> = Display::new()?;
    let mut state = CompositorState::new(&mut display, store);

    // ВАЖНО (само-дедлок): winit собран с wayland-бэкендом. Если выставить
    // WAYLAND_DISPLAY на СВОЙ сокет ДО winit::init, winit подключится к нашему
    // же сокету и зависнет в registry-roundtrip: accept() ещё не вызывается,
    // мы стоим внутри init. Поэтому окно создаём ПЕРВЫМ, а сокет публикуем после.
    let (mut backend, mut winit) =
        winit::init::<GlesRenderer>().map_err(|e| anyhow::anyhow!("winit init: {e}"))?;

    let listener = ListeningSocket::bind_auto("zui-tad", 0..32)
        .or_else(|_| ListeningSocket::bind("zui-tad-0"))?;
    let socket_name = listener
        .socket_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "zui-tad-0".into());
    std::env::set_var("WAYLAND_DISPLAY", &socket_name);
    tracing::info!("ZUI-TAD canvas compositor: clients welcome at WAYLAND_DISPLAY={socket_name}");
    tracing::info!("winit+GLES backend up: {:?}", backend.window_size());
    let size = backend.window_size();
    state.set_viewport(size.w as u32, size.h as u32);
    state.register_output(size.w as u32, size.h as u32);

    let keyboard = state
        .seat
        .add_keyboard(Default::default(), 200, 25)
        .map_err(|e| anyhow::anyhow!("keyboard: {e}"))?;
    state.keyboard = Some(keyboard.clone());
    let pointer = state.seat.add_pointer();
    state.pointer = Some(pointer.clone());

    let start_time = std::time::Instant::now();
    let mut iterations: u64 = 0;

    loop {
        let status = winit.dispatch_new_events(|event| {
            handle_winit_event(&mut state, event, &keyboard, &pointer);
        });
        let _ = status;
        iterations += 1;

        // Курсор: нужен только &backend, поэтому строго до мутабельного bind().
        set_hover_cursor(&state, backend.window());
        backend.bind().map_err(|e| anyhow::anyhow!("bind: {e}"))?;
        let size = backend.window_size();
        state.set_viewport(size.w as u32, size.h as u32);
        state.frame_time_ms = start_time.elapsed().as_millis() as u32;
        let damage = Rectangle::from_size(size);
        let (vw, vh) = (size.w as f32, size.h as f32);

        // 1. Холст → экран.
        let quads = layout_windows(&state.canvas, &state.camera, state.viewport());

        // 2. Render elements: позиция квада + масштаб камеры.
        let mut elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
        for q in &quads {
            if q.suspended {
                continue; // TODO(shell): рисовать плейсхолдер suspended-окна
            }
            if q.quad.clip(vw, vh).is_none() {
                continue;
            }
            if let Some(entry) = state.entry_by_id(q.id) {
                if let Some(tl) = entry.window.toplevel() {
                    elements.extend(render_elements_from_surface_tree(
                        backend.renderer(),
                        tl.wl_surface(),
                        (q.quad.x.round() as i32, q.quad.y.round() as i32),
                        q.zoom as f64,
                        1.0,
                        Kind::Unspecified,
                    ));
                }
            }
        }

        // 3. Кадр.
        let bg = Color32F::new(0.032, 0.036, 0.042, 1.0);
        let mut frame = backend
            .renderer()
            .render(size, Transform::Flipped180)
            .map_err(|e| anyhow::anyhow!("render: {e}"))?;
        frame
            .clear(bg, &[damage])
            .map_err(|e| anyhow::anyhow!("clear: {e}"))?;
        draw_render_elements(&mut frame, 1.0, &elements, &[damage])
            .map_err(|e| anyhow::anyhow!("draw: {e}"))?;
        let _ = frame.finish();

        // 4. Кадровые колбэки клиентам.
        for q in &quads {
            if let Some(entry) = state.entry_by_id(q.id) {
                if let Some(surface) = entry.window.wl_surface() {
                    send_frames_surface_tree(&surface, state.frame_time_ms);
                }
            }
        }

        if let Some(stream) = listener.accept()? {
            tracing::info!("client connected to compositor socket");
            display
                .handle()
                .insert_client(stream, Arc::new(ClientData::default()))
                .map_err(|e| anyhow::anyhow!("client: {e}"))?;
        }
        if iterations == 1 {
            tracing::info!("render loop entered (first iteration)");
        }
        if iterations % 120 == 0 {
            let rects: Vec<String> = state
                .canvas
                .windows()
                .iter()
                .map(|w| {
                    let s = state.camera.world_to_screen(w.pos);
                    format!(
                        "[{} {}x{} screen {:.0},{:.0}]",
                        w.id, w.size.x as i32, w.size.y as i32, s.x, s.y
                    )
                })
                .collect();
            tracing::info!(
                "loop it={} t={}ms windows={} zoom={:.2} {}",
                iterations,
                state.frame_time_ms,
                state.canvas.len(),
                state.camera.zoom,
                rects.join(" ")
            );
        }

        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;
        backend
            .submit(Some(&[damage]))
            .map_err(|e| anyhow::anyhow!("submit: {e}"))?;
    }
}

// ---------------------------------------------------------------- input

fn handle_winit_event(
    state: &mut CompositorState,
    event: WinitEvent,
    keyboard: &KeyboardHandle<CompositorState>,
    pointer: &PointerHandle<CompositorState>,
) {
    match event {
        WinitEvent::Resized { size, .. } => {
            state.set_viewport(size.w as u32, size.h as u32);
            state.update_output_mode(size.w as u32, size.h as u32);
        }
        WinitEvent::CloseRequested => std::process::exit(0),
        WinitEvent::Input(InputEvent::Keyboard { event }) => {
            let keycode = event.key_code();
            let key_state = event.state();
            let serial = state.next_serial();
            let time = state.frame_time_ms;
            keyboard.input::<(), _>(state, keycode, key_state, serial, time, |st, mods, sym| {
                if key_state != KeyState::Pressed {
                    return FilterResult::Forward;
                }
                let raw = sym.modified_sym();
                if handle_shortcut(st, mods.logo, mods.alt, mods.shift, raw.into(), serial) {
                    FilterResult::Intercept(())
                } else {
                    FilterResult::Forward
                }
            });
        }
        WinitEvent::Input(InputEvent::PointerMotionAbsolute { event }) => {
            state.pointer_pos = Point2::new(event.x() as f32, event.y() as f32);
            apply_pointer_motion(state);
            update_pointer_focus(state, pointer);
        }
        WinitEvent::Input(InputEvent::PointerButton { event }) => {
            let serial = state.next_serial();
            let time = state.frame_time_ms;
            let pressed = event.state() == ButtonState::Pressed;
            let code = event.button_code();

            if code == 0x110 {
                // BTN_LEFT
                if pressed {
                    on_primary_press(state, keyboard, serial);
                } else {
                    let was_resize = state.interact.resize_handle().is_some();
                    if let Some(id) = state.interact.grab_id() {
                        if was_resize {
                            // финальный размер — без троттлинга, чтобы клиент догнал
                            push_configure(state, id, true);
                        } else {
                            let threshold = snap_threshold(state);
                            state.canvas.snap(id, threshold);
                        }
                    }
                    state.interact.end();
                }
            }
            update_pointer_focus(state, pointer);
            pointer.button(
                state,
                &ButtonEvent {
                    serial,
                    time,
                    button: code,
                    state: event.state(),
                },
            );
        }
        WinitEvent::Input(InputEvent::PointerAxis { event }) => {
            let dx = event.amount(Axis::Horizontal).unwrap_or(0.0) as f32;
            let dy = event.amount(Axis::Vertical).unwrap_or(0.0) as f32;
            let mods = keyboard.modifier_state();
            if mods.logo {
                let factor = (1.0 - dy * 0.0025).clamp(0.5, 2.0);
                let cursor = cgmath::Vector2::new(state.pointer_pos.x, state.pointer_pos.y);
                state.camera.zoom_at(cursor, factor);
            } else if mods.alt {
                state.camera.pan(cgmath::Vector2::new(dx, 0.0));
            } else {
                state.camera.pan(cgmath::Vector2::new(dx, dy));
            }
        }
        _ => {}
    }
}

/// Позиция курсора изменилась: применить drag/pan.
fn apply_pointer_motion(state: &mut CompositorState) {
    let screen = state.pointer_pos;
    let canvas_pt = state.canvas_point();
    let (dw, ds) = state.interact.cursor(canvas_pt, screen);

    match state.interact.mode {
        Mode::Move { id, .. } => {
            state.canvas.nudge(id, dw);
            let threshold = snap_threshold(state);
            state.canvas.snap(id, threshold);
        }
        Mode::Resize { id, .. } => {
            if let Some((pos, size)) = state.interact.resize_plan(MIN_WINDOW) {
                state.canvas.move_to(id, pos);
                state.canvas.resize(id, size);
                push_configure(state, id, false);
            }
        }
        Mode::Pan => {
            state.camera.pan(cgmath::Vector2::new(ds.x, ds.y));
        }
        _ => {}
    }
}

fn on_primary_press(
    state: &mut CompositorState,
    keyboard: &KeyboardHandle<CompositorState>,
    serial: smithay::utils::Serial,
) {
    let canvas_pt = state.canvas_point();
    let mods = keyboard.modifier_state();

    // Mod+ЛКМ = панорама холста (driftwm `Mod` + LMB drag).
    if mods.logo {
        state.interact.begin_pan(canvas_pt, state.pointer_pos);
        return;
    }

    match state.canvas.hit_test(canvas_pt) {
        Some(id) => {
            let zone = resize_zone(state, id, canvas_pt);
            state.focus_window(id, serial);
            match zone {
                // Край окна — ресайз (как в любом DE).
                Some(handle) => {
                    if let Some(w) = state.canvas.get(id) {
                        let (pos, size) = (w.pos, w.size);
                        state.interact.begin_resize(
                            id,
                            handle,
                            pos,
                            size,
                            canvas_pt,
                            state.pointer_pos,
                        );
                    }
                }
                // Двойной клик по заголовку — fit-window (maximize на холсте).
                None => {
                    let now = std::time::Instant::now();
                    let double = matches!(
                        state.last_click,
                        Some((t, wid)) if wid == id && now.duration_since(t) < std::time::Duration::from_millis(400)
                    );
                    state.last_click = Some((now, id));
                    if double {
                        fit_window(state);
                        state.interact.end();
                    } else {
                        state.interact.begin_move(id, canvas_pt, state.pointer_pos);
                    }
                }
            }
        }
        None => {
            // Клик по пустому холсту — панорама.
            state.interact.begin_pan(canvas_pt, state.pointer_pos);
        }
    }
}

/// Зона ресайза под курсором (None — центр окна, значит перетаскивание).
fn resize_zone(
    state: &CompositorState,
    id: WindowId,
    canvas_pt: Point2<f32>,
) -> Option<ResizeHandle> {
    let w = state.canvas.get(id)?;
    let margin = (RESIZE_MARGIN_PX / state.camera.zoom.max(0.05))
        .min(w.size.x / 3.0)
        .min(w.size.y / 3.0);
    let local = cgmath::Vector2::new(canvas_pt.x - w.pos.x, canvas_pt.y - w.pos.y);
    ResizeHandle::from_point(w.size, local, margin)
}

/// Курсор под текущий режим/зону (клиент может попросить спрятать его).
fn set_hover_cursor(state: &CompositorState, win: &::winit::window::Window) {
    // В cursor-icon нет варианта "скрыть" — используем видимость окна.
    if state.cursor_hidden {
        win.set_cursor_visible(false);
        return;
    }
    win.set_cursor_visible(true);
    let icon = if let Some(h) = state.interact.resize_handle() {
        cursor_for(h)
    } else if state.interact.is_dragging() {
        CursorIcon::Grabbing
    } else {
        let pt = state.canvas_point();
        match state
            .canvas
            .hit_test(pt)
            .and_then(|id| resize_zone(state, id, pt))
        {
            Some(h) => cursor_for(h),
            None => CursorIcon::Default,
        }
    };
    win.set_cursor_icon(icon);
}

/// Хендл ресайза → системный курсор (winit реэкспортирует cursor-icon).
fn cursor_for(h: ResizeHandle) -> CursorIcon {
    match h {
        ResizeHandle::N | ResizeHandle::S => CursorIcon::NsResize,
        ResizeHandle::E | ResizeHandle::W => CursorIcon::EwResize,
        ResizeHandle::Nw | ResizeHandle::Se => CursorIcon::NwseResize,
        ResizeHandle::Ne | ResizeHandle::Sw => CursorIcon::NeswResize,
    }
}

/// Отправить клиенту актуальный размер холста (троттлинг при live-ресайзе).
fn push_configure(state: &mut CompositorState, id: WindowId, force: bool) {
    if !force && state.last_configure.elapsed() < std::time::Duration::from_millis(24) {
        return;
    }
    state.last_configure = std::time::Instant::now();
    let Some((w, h)) = state
        .canvas
        .get(id)
        .map(|win| (win.size.x.round() as i32, win.size.y.round() as i32))
    else {
        return;
    };
    if let Some(entry) = state.entry_by_id(id) {
        if let Some(tl) = entry.window.toplevel() {
            tl.with_pending_state(|s| {
                s.size = Some(Size::from((w, h)));
            });
            tl.send_configure();
        }
    }
}

/// Пересчитать pointer-focus: найти окно под курсором и отдать ему local-координаты.
fn update_pointer_focus(state: &mut CompositorState, pointer: &PointerHandle<CompositorState>) {
    let canvas_pt = state.canvas_point();
    let hit = state
        .canvas
        .hit_test(canvas_pt)
        .and_then(|id| state.canvas.get(id).map(|w| (id, w.pos)));

    let (focus, local) = match hit {
        Some((id, wpos)) => {
            let local = Point::<f64, Logical>::from((
                (canvas_pt.x - wpos.x) as f64,
                (canvas_pt.y - wpos.y) as f64,
            ));
            let surface = state.entry_by_id(id).and_then(|e| e.window.wl_surface());
            (
                surface.map(|s| (s.into_owned(), Point::<f64, Logical>::from((0.0, 0.0)))),
                local,
            )
        }
        None => (None, Point::<f64, Logical>::from((0.0, 0.0))),
    };

    let serial = state.next_serial();
    let time = state.frame_time_ms;
    pointer.motion(
        state,
        focus,
        &MotionEvent {
            location: local,
            serial,
            time,
        },
    );
}

// ---------------------------------------------------------------- shortcuts

fn snap_threshold(state: &CompositorState) -> f32 {
    SNAP_PX / state.camera.zoom.max(0.05)
}

fn fly_to(state: &mut CompositorState, id: WindowId) {
    if let Some(w) = state.canvas.get(id) {
        state.camera.center = w.center();
    }
}

fn jump(state: &mut CompositorState, dir: Dir) {
    let from = state.camera.center;
    if let Some(id) = state.canvas.nearest_in_direction(from, dir) {
        let serial = state.next_serial();
        state.focus_window(id, serial);
        fly_to(state, id);
    }
}

fn zoom_center(state: &mut CompositorState, factor: f32) {
    let vp = state.viewport();
    let center = cgmath::Vector2::new(vp.x as f32 * 0.5, vp.y as f32 * 0.5);
    state.camera.zoom_at(center, factor);
}

fn fit_window(state: &mut CompositorState) {
    let id = match state.canvas.focus() {
        Some(id) => id,
        None => return,
    };
    let viewport = state.viewport();
    let plan = match state.canvas.fit_window_plan(id, viewport, 24.0) {
        Some(p) => p,
        None => return,
    };
    state.camera.center = plan.camera_center;
    state.camera.zoom = plan.zoom;
    state.canvas.move_to(id, plan.window_pos);
    state.canvas.resize(id, plan.window_size);
    if let Some(entry) = state.entry_by_id(id) {
        if let Some(tl) = entry.window.toplevel() {
            let size = Size::from((
                plan.window_size.x.round() as i32,
                plan.window_size.y.round() as i32,
            ));
            tl.with_pending_state(|s| {
                s.size = Some(size);
            });
            tl.send_configure();
        }
    }
}

fn close_focused(state: &mut CompositorState) {
    if let Some(id) = state.canvas.focus() {
        if let Some(entry) = state.entry_by_id(id) {
            if let Some(tl) = entry.window.toplevel() {
                tl.send_close();
            }
        }
    }
}

fn bookmark(state: &mut CompositorState, digit: u32, shift: bool) {
    let slot = (digit.saturating_sub(keysyms::KEY_1)) as usize;
    if shift {
        let center = state.camera.center;
        state.canvas.set_bookmark(slot, center);
        tracing::info!("bookmark {} set at {:?}", slot + 1, center);
    } else if let Some(center) = state.canvas.bookmark(slot) {
        state.camera.center = center;
    }
}

fn spawn_terminal() {
    let candidates: Vec<String> = std::env::var("TERMINAL")
        .ok()
        .into_iter()
        .chain(
            ["foot", "alacritty", "kitty", "wezterm", "xterm"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    for cmd in candidates {
        if std::process::Command::new(&cmd).spawn().is_ok() {
            tracing::info!("spawned terminal: {cmd}");
            return;
        }
    }
    tracing::warn!("no terminal found ($TERMINAL / foot / alacritty / kitty / wezterm / xterm)");
}

/// Хоткеи холста. `true` = событие поглощено.
fn handle_shortcut(
    state: &mut CompositorState,
    logo: bool,
    alt: bool,
    shift: bool,
    sym: u32,
    serial: smithay::utils::Serial,
) -> bool {
    // Alt+Tab — MRU-кольцо по всем окнам (hold-to-commit).
    if alt && sym == keysyms::KEY_Tab {
        if let Some(next) = state.canvas.mru_cycle(state.canvas.focus(), !shift) {
            state.focus_window(next, serial);
            fly_to(state, next);
        }
        return true;
    }
    if !logo {
        return false;
    }
    match sym {
        keysyms::KEY_Left => jump(state, Dir::Left),
        keysyms::KEY_Right => jump(state, Dir::Right),
        keysyms::KEY_Up => jump(state, Dir::Up),
        keysyms::KEY_Down => jump(state, Dir::Down),
        // Overview: всё на экран (driftwm Mod+W).
        keysyms::KEY_w => {
            let vp = state.viewport();
            if let Some(plan) = state.canvas.zoom_to_fit(vp, 48.0) {
                state.camera.center = plan.camera_center;
                state.camera.zoom = plan.zoom;
            }
        }
        // Home (origin, zoom 1.0).
        keysyms::KEY_a => {
            state.camera.center = state.canvas.origin;
            state.camera.zoom = 1.0;
        }
        // Center focused.
        keysyms::KEY_c => {
            if let Some(id) = state.canvas.focus() {
                fly_to(state, id);
            }
        }
        // Fit window (maximize на холсте).
        keysyms::KEY_m => fit_window(state),
        keysyms::KEY_equal => zoom_center(state, 1.25),
        keysyms::KEY_minus => zoom_center(state, 0.8),
        keysyms::KEY_0 => state.camera.zoom = 1.0,
        keysyms::KEY_Return => spawn_terminal(),
        keysyms::KEY_q => close_focused(state),
        keysyms::KEY_1 | keysyms::KEY_2 | keysyms::KEY_3 | keysyms::KEY_4 => {
            bookmark(state, sym, shift)
        }
        _ => return false,
    }
    true
}

// ---------------------------------------------------------------- misc

fn send_frames_surface_tree(surface: &WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}
