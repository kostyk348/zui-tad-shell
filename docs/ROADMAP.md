# ROADMAP: что дальше и почему так

Честное разделение: «сделано» = собрано, запущено и проверено. Всё, что ниже,
помечено состоянием проверки.

## Сделано и проверено вживую

* холст (камера, кластеры, snap, directional jump, MRU, overview, закладки);
* реальные клиенты на холсте: drag, resize за 8 краёв (с `xdg configure`),
  maximize/fullscreen/minimize, `move_request`/`resize_request` от клиента;
* оболочка **внутри** композитора: панель, миникарта, ЭКГ-виталы, подсказки,
  тосты, командное меню, справка (клик по панели открывает меню, строка
  меню выполняет действие — проверено: смена темы на живой сессии);
* сессия холста: `session.json`, dormant-восстановление, усыновление места;
* протоколы: `xdg-decoration` (client-side), `primary-selection`,
  `xdg-activation`, курсоры-поверхности клиента;
* 117 тестов, clippy без предупреждений, CI (ядро + композитор).

## Не сделано — с точными причинами

### 1. DRM/TTY-бэкенд + мультимонитор (главный блок)

Состояние: **слои 0 и 1 написаны**.
* слой 0 (`--drm`) — проверен вживую: libseat-сессия, карта, crtc и коннекторы
  с режимами (eDP-1 → 1920x1200);
* слой 1 (`--drm-render`) — **компилируется**: `present_frame()` собирает кадр
  CPU-композитором (`cpu.rs`) и отправляет его в dumb buffer
  (`create_dumb_buffer → map_dumb_buffer → add_framebuffer → page_flip`).
  Ключ к разгадке: маппируемый буфер — это `drm::control::dumbbuffer::DumbBuffer`
  из drm-крейта, а smithay'ев `allocator::dumb::DumbBuffer` не имеет `map`;
  путь собран только на API drm-крейта.

### Последний блок: calloop (выверенные факты для исполнителя)

Факты, проверенные по исходникам (чтобы не искать заново):

| Нужно | Что есть |
|---|---|
| winit как источник | `WinitEventLoop` **реализует `calloop::EventSource`** (`NEEDS_EXTRA_LIFECYCLE_EVENTS = true`) |
| клиентский сокет | `smithay::wayland::socket::ListeningSocketSource::{new_auto, with_name}` — источник, отдаёт `UnixStream` |
| диспатч Wayland | `Display` держать **внутри состояния** (канонический пример — `smithay/src/wayland/socket.rs`): callback делает `state.display.handle().insert_client(..)`; диспатч — приём «take → `dispatch_clients(&mut state)` → put back» |
| ввод | `LibinputInputBackend` — источник; поля событий те же трейты smithay (`delta()`, `button_code()`, `state()`, `key_code()`, `amount(axis)`), что и у winit → обработчик ввода можно сделать обобщённым `handle_input<B: InputBackend>` и переиспользовать в обоих бэкендах |
| vblank | `DrmDeviceNotifier` — источник (`DrmEvent::VBlank { crtc }`); для честного флипа нужны ДВА dumb-буфера (double buffering) |
| idle | `IdleNotifierState::new::<CompositorState>(&dh, loop_handle)` — таймеры вставляются в этот же цикл, `notify_activity(&seat)` на ввод |
| пуск | `event_loop.run(None, &mut state, |_| {})` |

Порядок, который не ломает рабочий nested-путь:
1. обобщённый `handle_input<B>` + переключение nested-цикла на calloop → **проверяется здесь** (kitty/X11/grim должны работать как раньше);
2. затем DRM-сессия на том же цикле (+ libinput, + vblank с двойным буфером, + idle) → проверяется только на TTY;
3. мультимонитор: цикл по `crtc`/коннекторам, у каждого свой `DrmSurface` и вьюпорт камеры.

Старое описание блока (для контекста):

Что осталось, одним блоком — **переход на calloop**:
* vblank-синхронизация (`DrmDeviceNotifier` — calloop-источник; сейчас кадры
  идут по таймеру, возможен tearing);
* ввод (`LibinputInputBackend` — тоже calloop-источник; сейчас TTY-сессия
  рендерит, но не принимает ввод);
* `ext-idle-notify` (`IdleNotifierState::new(display, loop_handle)`);
* мультимонитор (цикл по crtc/коннекторам — структура готова, включён первый выход).

Рантайм-проверка слоя 1 возможна только на машине с TTY:
`zui-compositor --drm-render` (забирает консоль). Причина — не «сложно», а «нечем проверить»:
в этой среде X11 держит DRM-master, свободного VT нет, `vkms` не загружен.
Писать 400+ строк бэкенда, который нельзя запустить ни разу, — против правила
«сделано = проверено». Разведка API сделана, сигнатуры выверены:

| Что нужно | Что есть в smithay 0.5.1 |
|---|---|
| сессия/VT | `backend::session::libseat::LibSeatSession` (+ `Session::{open, seat, is_active, change_vt}`) |
| устройство | `DrmDevice::new(fd, disable_connectors) -> (DrmDevice, DrmDeviceNotifier)`, `DrmDevice::device_id()` |
| вывод | `DrmSurface::{use_mode, current_mode, crtc, page_flip, commit_pending}` |
| композитинг | `drm::compositor::DrmCompositor::{new, render_frame, queue_frame, frame_submitted}` (+ GBM-аллокатор и экспортёр) |
| ввод | `LibinputInputBackend` — **calloop-источник** (ручного `dispatch_new_events` нет) |
| устройства | `UdevBackend` — тоже calloop-источник; клиентские сокеты: `ListeningSocket` как calloop-источник (`wayland/socket.rs`), `Display::dispatch_clients` |

Вывод разведки: DRM-путь требует **перевести цикл на calloop** (сейчас у нас
winit-цикл). Это архитектурный шаг, а не патч: calloop-event-loop + регистрация
сессии/udev/libinput/сокета + таймер рендера. Тот же шаг разблокирует
`ext-idle-notify` (он тоже хочет `LoopHandle`).

Порядок работ, когда будет доступен TTY:
1. `crates/compositor/src/drm.rs`: session → devices → connectors → outputs;
2. общий `render_outputs()` (переиспользовать layout + shell-слой — они уже не
   знают про winit);
3. libinput → существующий обработчик ввода (он принимает нормализованные
   события, а не winit-типы — переделки минимум);
4. hotplug через `UdevEvent::{Added, Changed, Removed}`;
5. мультимонитор: несколько `DrmCompositor`, каждый со своим Output и камерой-вьюпортом.

Проверка на машине владельца: `zui-compositor --check` → ожидаем «нет вложенной
сессии», затем запуск из TTY (или сессия из DM).

### 2. screencopy

Состояние: **заблокировано API smithay 0.5.1**. Модуля нет; GL-путь закрыт —
`GlesMapping` (результат `ExportMem::copy_framebuffer`) не имеет публичного
доступа к пикселям, `copy_to_slice` в крейте отсутствует. Варианты: (а) CPU-путь
DRM (`docs/DRM-DESIGN.md`) — там кадр и так лежит в нашей памяти, screencopy
становится тривиальным; (б) `xdg-desktop-portal-wlr`; (в) свой `wlr-screencopy`
модуль + `render_to_texture`/dmabuf-экспорт.

### 3. ext-idle-notify

Состояние: **блокировано тем же calloop-переходом** (см. п.1). Сам протокол в
smithay есть: `IdleNotifierState::new(display, loop_handle)` + `notify_activity(seat)`.

### 4. Named-курсоры (cursor-shape-v1)

Состояние: **сделано**. smithay и winit используют один и тот же тип
(`cursor_icon::CursorIcon`), поэтому `CursorImageStatus::Named(icon)`
пробрасывается в `Window::set_cursor_icon` напрямую, без маппинга имён.

## Известные ограничения среды разработки

* вложенный запуск под X11 без рабочего WM не получает синтетические клавиши
  `xdotool` — горячие клавиши композитора проверяются в живой сессии;
* белые полосы при ресайзе `alacritty` (GTK CSD) — клиентская перерисовка,
  с `kitty` не воспроизводится.
