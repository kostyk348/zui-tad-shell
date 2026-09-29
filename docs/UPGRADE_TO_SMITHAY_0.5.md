# Миграция на smithay 0.5 + DRM backend

## Почему smithay 0.3, а не 0.5?

Текущая версия использует smithay 0.3 — это **рабочая** версия, которая:
- Собирается без `libudev-dev`, `libdrm-dev`, `libinput-dev` и т.д.
- Запускается внутри другой DE (через winit backend)
- Не требует `sudo` для установки системных dev-пакетов

Smithay 0.5 даёт:
- Настоящий DRM/KMS backend (запуск с TTY)
- libinput через evdev (без winit)
- Полный XDG shell (приложения как Wayland-клиенты)
- Layer shell, foreign toplevel, session lock
- Window embedding (захват surface как текстуры)
- Multi-output + hotplug

Но smithay 0.5 требует:
- `sudo apt install libudev-dev libseat-dev libdrm-dev libgbm-dev libinput-dev pkg-config`
- Реализации **множества trait'ов** (GlobalDispatch, Dispatch, SeatHandler, XdgShellHandler, LayerShellHandler, BufferHandler, ShmHandler, etc.)
- ~2000 строк дополнительного кода

Я не могу проверить сборку smithay 0.5 в своём окружении (нет dev-пакетов, нет sudo), поэтому этот документ — **детальный план**, который вы выполняете у себя.

---

## Шаг 1. Установить dev-зависимости

```bash
sudo apt update
sudo apt install -y \
    build-essential pkg-config \
    libwayland-dev libxkbcommon-dev libx11-dev libxrandr-dev \
    libxinerama-dev libxcursor-dev libxi-dev libegl1 libgles2 \
    libudev-dev libseat-dev libdrm-dev libgbm-dev libinput-dev \
    fonts-dejavu-core wmctrl xdotool
```

Проверка:
```bash
pkg-config --modversion libudev
pkg-config --modversion libdrm
pkg-config --modversion libinput
pkg-config --modversion gbm
# Все должны вывести версии
```

## Шаг 2. Настроить rustup

У вас в логе:
```
error: rustup could not choose a version of cargo to run, because one wasn't specified explicitly
```

Решение:
```bash
rustup default stable
rustc --version  # должно быть >= 1.80
cargo --version
```

## Шаг 3. Обновить Cargo.toml

В `Cargo.toml` заменить:
```toml
# Было (smithay 0.3):
smithay = { version = "0.3", default-features = false, features = [
    "backend_winit", "backend_egl",
    "renderer_gl", "wayland_frontend",
    "input", "xwayland", "use_system_lib"
] }

# Стало (smithay 0.5):
smithay = { version = "0.5", default-features = false, features = [
    "backend_winit", "backend_egl",
    "backend_drm", "backend_udev", "backend_gbm",
    "backend_libinput", "backend_session",
    "renderer_gl", "wayland_frontend",
    "desktop", "input", "xwayland", "use_system_lib"
] }
```

## Шаг 4. Реализовать trait'ы

В smithay 0.5 нужно реализовать trait'ы для `CompositorState`. Создать файл `crates/compositor/src/handlers.rs`:

```rust
use crate::state::CompositorState;
use smithay::reexports::wayland_server::{
    backend::{GlobalId, ObjectId},
    protocol::{
        wl_compositor::WlCompositor,
        wl_output::WlOutput,
        wl_seat::WlSeat,
        wl_shm::WlShm,
        wl_subcompositor::WlSubcompositor,
        wl_surface::WlSurface,
    },
    Resource, Dispatch, GlobalDispatch, Client,
};
use smithay::wayland::{
    compositor::CompositorHandler,
    data_device::DataDeviceHandler,
    output::OutputHandler,
    seat::SeatHandler,
    shell::xdg::XdgShellHandler,
    shell::wlr_layer::LayerShellHandler,
    shm::ShmHandler,
    foreign_toplevel::ForeignToplevelHandler,
    session_lock::SessionLockHandler,
};
use smithay::desktop::{PopupHandler, LayerSurface, Window};

// Реализация всех trait'ов для CompositorState.
// Примеры можно посмотреть в smithay/examples/ (tinywl.rs, ankou.rs).

impl CompositorHandler for CompositorState {
    fn compositor_state(&mut self) -> &mut smithay::wayland::compositor::CompositorState {
        &mut self.compositor
    }
    fn client_compositor_state<'a>(&self, _client: &'a Client) -> &'a smithay::wayland::compositor::CompositorState {
        // ...
    }
}

impl XdgShellHandler for CompositorState {
    fn xdg_shell_state(&mut self) -> &mut smithay::wayland::shell::xdg::XdgShellState {
        &mut self.xdg_shell
    }
    fn new_toplevel(&mut self, surface: smithay::wayland::shell::xdg::ToplevelSurface) {
        // Создать Window, добавить в space, зарегистрировать как VO
    }
    fn new_popup(&mut self, surface: smithay::wayland::shell::xdg::PopupSurface, _positioner: smithay::wayland::shell::xdg::PositionerState) {
        // Popup handling
    }
}

impl SeatHandler for CompositorState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    fn seat_state(&mut self) -> &mut smithay::wayland::seat::SeatState<CompositorState> {
        &mut self.seat_state
    }
    fn focus_changed(&mut self, _seat: &smithay::input::Seat<CompositorState>, _surface: Option<&WlSurface>) {}
    fn cursor_image(&mut self, _seat: &smithay::input::Seat<CompositorState>, _image: smithay::wayland::seat::CursorImageStatus) {}
}

impl LayerShellHandler for CompositorState {
    fn layer_shell_state(&mut self) -> &mut smithay::wayland::shell::wlr_layer::WlrLayerShellState {
        &mut self.layer_shell
    }
    fn new_layer_surface(&mut self, surface: smithay::wayland::shell::wlr_layer::LayerSurface, _output: Option<smithay::output::Output>, _namespace: String, _layer: smithay::wayland::shell::wlr_layer::Layer) {
        // Map layer surface
    }
}

impl ForeignToplevelHandler for CompositorState {
    fn foreign_toplevel_manager_state(&mut self) -> &mut smithay::wayland::foreign_toplevel::ForeignToplevelManagerState {
        &mut self.foreign_toplevel
    }
}

impl SessionLockHandler for CompositorState {
    fn session_lock_state(&mut self) -> &mut smithay::wayland::session_lock::SessionLockManagerState {
        &mut self.session_lock
    }
    fn lock(&mut self, _confirmation: smithay::wayland::session_lock::LockSurface) {
        self.locked = true;
    }
    fn unlock(&mut self) {
        self.locked = false;
    }
}

impl ShmHandler for CompositorState {
    fn shm_state(&self) -> &smithay::wayland::shm::ShmState {
        &self.shm
    }
}

impl OutputHandler for CompositorState {}
impl DataDeviceHandler for CompositorState {}
impl PopupHandler for CompositorState {}
```

## Шаг 5. Реализовать DRM backend

В `crates/compositor/src/backend.rs`:

```rust
#[cfg(feature = "drm")]
fn run_drm_backend(store: Arc<Mutex<GraphStore>>) -> Result<()> {
    use smithay::backend::drm::DrmDevice;
    use smithay::backend::libinput::LibinputSessionInterface;
    use smithay::backend::session::auto::AutoSession;
    use smithay::backend::udev::UdevBackend;
    use smithay::reexports::calloop::EventLoop;
    use smithay::reexports::wayland_server::Display;

    let mut display: Display<CompositorState> = Display::new()?;
    let mut state = CompositorState::new(&mut display, store);

    // 1. Session via logind
    let (session, _notifier) = AutoSession::new(LibinputSessionInterface::default())
        .ok_or_else(|| anyhow::anyhow!("Не удалось открыть session"))?;

    // 2. Udev — обнаружение GPU
    let udev = UdevBackend::new("zui-tad", session.signal_token())?;
    let primary = udev.device_list().next()
        .ok_or_else(|| anyhow::anyhow!("No DRM device"))?;

    // 3. DRM device + GBM + GLES2 renderer
    let mut event_loop: EventLoop<CompositorState> = EventLoop::try_new()?;
    let _drm = DrmDevice::new(primary.devnode().to_owned(), false, &event_loop.handle())?;

    // 4. Wayland socket
    let socket = display.handle().add_socket_auto()?;
    std::env::set_var("WAYLAND_DISPLAY", socket.to_string_lossy().to_string());

    // 5. Main loop
    loop {
        event_loop.dispatch(std::time::Duration::from_millis(16), &mut state)?;
        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;
    }
}
```

## Шаг 6. Window embedding

После миграции на 0.5 в `window_embedding.rs`:

```rust
use smithay::backend::renderer::{
    gles2::Gles2Renderer,
    ImportDma, ImportEgl, Renderer, Texture,
};
use smithay::desktop::Window;

pub fn capture_window(renderer: &mut Gles2Renderer, window: &Window) -> Option<CapturedTexture> {
    let surface = window.toplevel().wl_surface();
    let geo = window.geometry();
    let w = geo.size.w as u32;
    let h = geo.size.h as u32;

    // 1. Рендерим в offscreen Gles2Buffer
    let texture = renderer.render_texture(surface, (w, h), smithay::backend::renderer::Transform::Flipped180, 1.0)?;
    // 2. glReadPixels → Vec<u8>
    let rgba = renderer.read_pixels(&texture, ..)?;
    Some(CapturedTexture { width: w, height: h, rgba, last_update: std::time::Instant::now() })
}
```

## Шаг 7. Сборка и установка

```bash
cargo build --release --features compositor/drm
sudo ./install.sh
```

## Шаг 8. Запуск

### Способ A: через display manager
1. Выйти из текущей сессии
2. На экране входа выбрать "ZUI-TAD Shell"
3. Ввести пароль

### Способ B: с TTY
```bash
# Ctrl+Alt+F2 → TTY
# Логин →
./zui-tad-shell --compositor
# Запустится DRM backend, откроется direct rendering
```

## Что вы получите после миграции

- ✅ Полноценный Wayland-композитор (сопоставимо со sway/Hyprland)
- ✅ Запуск с TTY без другой DE под собой
- ✅ Окна приложений как настоящие Wayland-клиенты
- ✅ Window embedding (захват surface как текстуры → VO на холсте)
- ✅ Multi-monitor + hotplug
- ✅ Layer shell (waybar работает)
- ✅ Foreign toplevel (waybar видит окна)
- ✅ Session lock protocol (swaylock/hyprlock работают безопасно)
- ✅ XWayland (X11 приложения)
- ✅ libinput через evdev

## Что НЕ будет (без дополнительных доработок)

- ❌ Свои приложения (Files, Settings) — нужны отдельные программы
- ❌ Settings daemon — используете swaybg/waybar/wl-clipboard
- ❌ System tray — используете waybar module
- ❌ Accessibility — нужно реализовать отдельно
- ❌ Search — встроенный launcher уже есть

## Литература

- [smithay book](https://smithay.github.io/book/)
- [smithay examples](https://github.com/Smithay/smithay/tree/master/src/examples) — `tinywl.rs`, `ankou.rs`
- [Anvil](https://github.com/Smithay/anvil) — полный reference compositor
- [sway](https://github.com/swaywm/sway) — wlroots-based, для сравнения
- [Hyprland](https://github.com/hyprwm/Hyprland) — wlroots-based, для сравнения

## Оценка времени

- Шаги 1-3 (deps, rustup, Cargo.toml): 10 минут
- Шаг 4 (trait'ы): 4-8 часов (если разбираться в smithay API)
- Шаг 5 (DRM backend): 2-4 часа
- Шаг 6 (window embedding): 2-3 часа
- Шаги 7-8 (сборка, установка, тест): 1 час

**Итого: 1-2 рабочих дня** для разработчика, знакомого с Rust и Wayland.
