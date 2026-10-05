# DRM/TTY: дизайн и разведка (выверено по smithay 0.5.1)

Статус: **слой 0 реализован и проверен вживую** (сессия + карта + план вывода).
Композитинг и page-flip — не сделаны. Всё ниже — имена и сигнатуры, вычитанные
из исходников `smithay-0.5.1`, а не по памяти.

## Проверено вживую (2026-09-29)

`zui-compositor --drm` под X11-сессией:

```
libseat: seat=seat0, active=true (VT-переключение доступно)
карта /dev/dri/card1: crtc=1
  ● card1-eDP-1   connected  режимы: 1920x1200, 1920x1200, 1920x1080, 1600x1200
итог: карт открыто 1
```

То есть `LibSeatSession::new()` + `Session::open(OFlags::RDWR|CLOEXEC)` +
`DrmDevice::new(DrmDeviceFd::from(fd), false)` + `DrmDevice::crtcs()` работают.
Перечисление коннекторов и режимов сделано через sysfs (`/sys/class/drm/*/modes`) —
без невыверенных API. Модсет НЕ выполнялся: X11 держит консоль, забирать
master у живой сессии нельзя (это выкинуло бы пользователя из X).

## Главный вывод

У нас **нет `zwp_linux_dmabuf`** — клиенты рисуют через `wl_shm`. Значит
TTY-бэкенд **не обязан тянуть GL**: достаточно dumb-буфера ядра и нашего
CPU-композитора (`phosphor` уже CPU-first). Это два разных пути, и они
неравноценны по риску.

## Путь A (предпочтительный): dumb buffer + CPU-композитор

Плюсы: ноль EGL/GBM/allocator-дженериков; переиспользует `layout_windows`,
`phosphor` (панель/меню/HUD), и даёт бонусом тривиальный screencopy.

Что нужно проверить ПЕРВЫМ делом (два load-bearing API, не подтверждены):

1. **Запись в dumb buffer.** `drm::dumb::DumbBuffer::map(&DrmDeviceFd)` возвращает
   `DumbMapping`; нужно, чтобы он давал `&mut [u8]` (`DerefMut`). Если только
   `&[u8]` — путь A отпадает (или требует mmap вручную).
2. **Чтение клиентских `wl_shm`.** У нас уже есть:
   `smithay::wayland::shm::with_buffer_contents(buffer, |ptr, len, BufferData{offset,width,height,stride,format}|)` —
   `BufferData` имеет **публичные поля** ✓. То есть пиксели окна читаются без GL:
   остаётся написать blit (масштаб под zoom + альфа) в наш `Pixmap`.

Компоненты:
- `session`: `LibSeatSession::new() -> (session, notifier)`, `Session::{open, seat, is_active, change_vt}` ✓
- `device`: `DrmDevice::new(fd, disable_connectors) -> (DrmDevice, DrmDeviceNotifier)` ✓, `DrmDevice::crtcs()` ✓
- `surface`: `DrmSurface::{use_mode(mode), current_mode(), crtc(), page_flip(planes, event), commit_pending()}` ✓,
  `PlaneState { handle: plane::Handle, config: Option<PlaneConfig> }` ✓ (легаси-путь сам соберёт fb из dumb)
- `framebuffer`: `drm::dumb::framebuffer_from_dumb_buffer(&DrmDeviceFd, &DumbBuffer, use_opaque) -> DumbFramebuffer` ✓
- ввод: `input` (libinput-rs) вручную: `Libinput::new_with_udev(LibinputUdev::new(&session))`,
  `udev_assign_seat`, затем `dispatch()` + итерация событий в своём цикле.
  ⚠ smithay-овский `LibinputInputBackend` — **calloop-источник**, ручного pump нет.

## Путь B (стандартный): GBM/EGL + `DrmOutputManager`

Подтверждённые сигнатуры:
- `DrmOutputManager::new(device, allocator, exporter, gbm: Option<GbmDevice<G>>, color_formats, renderer_formats)`
- `initialize_output::<R, E>(&mut self, crtc, mode: control::Mode, connectors: &[connector::Handle], output_mode_source, planes: Option<Planes>, renderer: &mut R, render_elements: &DrmOutputRenderElements<R, E>) -> DrmOutput<A, F, U, G>`
- `with_compositors(|&HashMap<crtc::Handle, Mutex<DrmCompositor<A,F,U,G>>>|)`, `use_mode(...)`, `pause()`, `activate(disable_connectors)`
- `DrmCompositor::{render_frame(renderer, elements, clear, FrameFlags), queue_frame(user_data), frame_submitted()}`

Цена: тип-параметры `A`(allocator), `F`(exporter), `U`(user data), `G`(gbm) + `E: RenderElement<R>`,
плюс `GbmAllocator::new(gbm, GbmBufferFlags::RENDERING|SCANOUT)`, `GbmFramebufferExporter`,
`EGLDisplay`/`EGLContext::new(&display)`/`GlesRenderer`. Это ~350-400 строк, и проверяются
они только сборкой типов — рантайм всё равно недоступен без TTY.

## Общее для обоих путей: цикл

`LibinputInputBackend`, `UdevBackend` и `ListeningSocket` — **calloop-источники**
(`wayland/socket.rs` показывает `event_loop.handle().insert_source(listening_socket, ...)`,
`Display::dispatch_clients` вызывается вручную). Значит DRM-путь = **перевести цикл на
calloop**: event-loop + сессия + udev + libinput + сокет + таймер рендера. Тот же переход
разблокирует `ext-idle-notify` (`IdleNotifierState::new(display, loop_handle)`).

## Порядок работ (когда есть TTY)

1. `--check` → убедиться, что нет вложенной сессии и есть `card*`/коннекторы.
2. Проверить два load-bearing API пути A (`DumbBuffer::map` мутабельность; blit shm).
3. Путь A: session → device → connector+mode → dumb fb → compose (CPU) → `page_flip`.
4. libinput вручную (свой цикл) или calloop-переход, если решаем делать idle.
5. Мультимонитор: по `DrmSurface` на crtc, у каждого свой viewport камеры.
6. Hotplug: `UdevEvent::{Added, Changed, Removed}` (или ручной rescан `/dev/dri`).

## Что НЕ проверено в этом файле

Ни одна строка пути A/B не запускалась: X11 держит DRM-master, свободного VT нет,
`vkms` не загружен. Это карта, а не результат.
