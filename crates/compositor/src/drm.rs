//! DRM/TTY: слой 0 — сессия, карта, план вывода.
//!
//! Что здесь ЕСТЬ: libseat-сессия, открытие DRM-карты через неё, перечисление
//! crtc и коннекторов с режимами (через sysfs, чтобы не зависеть от
//! невыверенных API). Это запускается на TTY и печатает план вывода.
//!
//! Чего здесь НЕТ: поверхностей, dumb-буферов, композитинга и page-flip.
//! Точный план и выверенные сигнатуры — `docs/DRM-DESIGN.md` (путь A:
//! dumb buffer + наш CPU-композитор, GL не нужен, т.к. dmabuf мы не
//! регистрируем и клиенты идут через wl_shm).

use anyhow::{Context, Result};
use smithay::backend::{
    drm::{DrmDevice, DrmDeviceFd},
    session::{libseat::LibSeatSession, Session},
};
use smithay::reexports::rustix::fs::OFlags;
use std::path::{Path, PathBuf};

/// Список карт `/dev/dri/card*`.
fn card_paths() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().starts_with("card"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Коннекторы и их режимы — из sysfs (не требует DRM-API).
fn connectors_for(card: &Path) -> Vec<(String, String, Vec<String>)> {
    let name = card
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/sys/class/drm") else {
        return out;
    };
    for e in dir.flatten() {
        let cname = e.file_name().to_string_lossy().into_owned();
        if !cname.starts_with(&format!("{name}-")) {
            continue;
        }
        let status = std::fs::read_to_string(format!("/sys/class/drm/{cname}/status"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let modes = std::fs::read_to_string(format!("/sys/class/drm/{cname}/modes"))
            .map(|s| s.lines().take(4).map(|l| l.to_string()).collect())
            .unwrap_or_default();
        out.push((cname, status, modes));
    }
    out.sort();
    out
}

/// Слой 0: поднять сессию, открыть карты, напечатать план. Возвращает число
/// пригодных карт (0 → смысла идти дальше нет).
pub fn probe_and_plan() -> Result<usize> {
    let (mut session, _notifier) =
        LibSeatSession::new().context("libseat: не удалось открыть сессию (нужен seat/VT)")?;
    let seat = session.seat();
    tracing::info!(
        "libseat: seat={seat}, active={} (VT-переключение доступно)",
        session.is_active()
    );

    let cards = card_paths();
    if cards.is_empty() {
        anyhow::bail!("нет /dev/dri/card*");
    }

    let mut usable = 0;
    for path in cards {
        // Открываем ИМЕННО через сессию: это даёт master-права и обработку VT.
        let fd = match session.open(&path, OFlags::RDWR | OFlags::CLOEXEC) {
            Ok(fd) => fd,
            Err(e) => {
                tracing::warn!("{}: сессия не дала fd: {e:?}", path.display());
                continue;
            }
        };
        let (device, _dev_notifier) = match DrmDevice::new(DrmDeviceFd::new(fd.into()), false) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("{}: DrmDevice::new: {e}", path.display());
                continue;
            }
        };
        usable += 1;
        println!("карта {}: crtc={}", path.display(), device.crtcs().len());
        for (name, status, modes) in connectors_for(&path) {
            let mark = if status == "connected" { "●" } else { "○" };
            println!(
                "  {mark} {name:<22} {status:<12} режимы: {}",
                modes.join(", ")
            );
        }
    }

    println!();
    if usable == 0 {
        println!("итог: ни одна карта не открылась (занята? нет прав? мы внутри чужой сессии?)");
    } else {
        println!("итог: карт открыто {usable}. Следующий шаг — dumb buffer + CPU-композитор + page_flip,");
        println!("план и сигнатуры: docs/DRM-DESIGN.md (путь A).");
    }
    Ok(usable)
}

// ---------------------------------------------------------------- слой 1: рендер

/// Отправить кадр холста на DRM-поверхность через dumb buffer.
///
/// Путь A (без GL): мы не регистрируем `zwp_linux_dmabuf`, клиенты рисуют в
/// `wl_shm`, а композитор и так CPU (`crate::cpu::compose`). Здесь кадр
/// копируется в память dumb-буфера и уходит в `page_flip`.
///
/// ⚠ Компилируется, но в рантайме НЕ проверено: в среде разработки X11 держит
/// DRM-master и свободного VT нет. Это единственная проверка, возможная здесь.
pub fn present_frame(
    state: &mut crate::state::CompositorState,
    device: &DrmDevice,
    surface: &smithay::backend::drm::DrmSurface,
    size: (u32, u32),
) -> Result<()> {
    use smithay::backend::drm::{PlaneConfig, PlaneState};
    use smithay::reexports::drm::buffer::{Buffer as DrmBuffer, DrmFourcc};
    use smithay::reexports::drm::control::Device as DrmControlDevice;
    use smithay::utils::{Physical, Rectangle, Transform};

    let (w, h) = (size.0.max(1), size.1.max(1));
    let fd = device.device_fd();

    // 1. Кадр на CPU — тот же композитор, что отдаёт screencopy.
    let frame = crate::cpu::compose(state, (w, h));

    // 2. Dumb buffer (drm-крейт: его буфер МАППИРУЕТСЯ, в отличие от
    //    smithay::backend::allocator::dumb::DumbBuffer, у которого нет map).
    let mut dumb = device
        .create_dumb_buffer((w, h), DrmFourcc::Xrgb8888, 32)
        .context("create_dumb_buffer")?;
    let pitch = DrmBuffer::pitch(&dumb) as usize;
    {
        let mut map = device
            .map_dumb_buffer(&mut dumb)
            .context("map_dumb_buffer")?;
        let src = frame.data();
        for y in 0..h as usize {
            let srow = y * w as usize * 4;
            let drow = y * pitch;
            for x in 0..w as usize {
                let s = srow + x * 4;
                let d = drow + x * 4;
                if d + 2 < map.len() {
                    // XRGB8888 в памяти — BGRA
                    map[d] = src[s + 2];
                    map[d + 1] = src[s + 1];
                    map[d + 2] = src[s];
                }
            }
        }
    }

    // 3. Framebuffer (drm-крейт умеет делать его прямо из DumbBuffer).
    let fb = device
        .add_framebuffer(&dumb, 24, 32)
        .context("add_framebuffer")?;

    // 4. Page-flip на primary-плоскости.
    let planes = surface.planes();
    let plane = planes.primary.first().context("нет primary-плоскости")?;
    let cfg = PlaneConfig {
        src: Rectangle::from_size((w as f64, h as f64).into()),
        dst: Rectangle::<i32, Physical>::from_size((w as i32, h as i32).into()),
        transform: Transform::Normal,
        alpha: 1.0,
        damage_clips: None,
        fb,
        fence: None,
    };
    surface
        .page_flip(
            [PlaneState {
                handle: plane.handle,
                config: Some(cfg),
            }],
            true,
        )
        .context("page_flip")?;
    let _ = fd;
    Ok(())
}

/// Слой 1: сессия + первый подключённый выход + цикл рендера.
///
/// Осознанные ограничения этой версии (и почему так):
///   * НЕТ vblank-синхронизации: `DrmDeviceNotifier` — calloop-источник, а цикл
///     у нас свой; кадры идут с фиксированным интервалом (возможен tearing);
///   * НЕТ ввода: `LibinputInputBackend` — тоже calloop-источник;
///   * один выход (первый подключённый), без hotplug.
/// Всё это снимается переходом на calloop — см. docs/ROADMAP.md.
///
/// Запуск: `zui-compositor --drm-render` ИЗ TTY (забирает консоль!).
pub fn run_drm_session(
    store: std::sync::Arc<parking_lot::Mutex<tad_core::GraphStore>>,
) -> Result<()> {
    use smithay::reexports::drm::control::Device as DrmControlDevice;

    let (mut session, _notifier) =
        LibSeatSession::new().context("libseat (нужен seat/VT: запускать из TTY)")?;
    tracing::info!(
        "libseat: seat={}, active={}",
        session.seat(),
        session.is_active()
    );

    let path = card_paths()
        .into_iter()
        .next()
        .context("нет /dev/dri/card*")?;
    let fd = session
        .open(&path, OFlags::RDWR | OFlags::CLOEXEC)
        .with_context(|| format!("сессия не дала fd на {}", path.display()))?;
    let (mut device, _dev_notifier) =
        DrmDevice::new(DrmDeviceFd::new(fd.into()), false).context("DrmDevice::new")?;

    // Первый подключённый коннектор и его предпочтительный режим.
    let res = device.resource_handles().context("resource_handles")?;
    let mut chosen: Option<(
        smithay::reexports::drm::control::connector::Handle,
        smithay::reexports::drm::control::Mode,
    )> = None;
    for c in &res.connectors {
        if let Ok(info) = device.get_connector(*c, true) {
            if info.state() == smithay::reexports::drm::control::connector::State::Connected {
                if let Some(mode) = info.modes().first().copied() {
                    chosen = Some((*c, mode));
                    break;
                }
            }
        }
    }
    let (connector, mode) = chosen.context("нет подключённого коннектора с режимом")?;
    let crtc = res.crtcs.first().copied().context("нет crtc")?;
    let surface = device
        .create_surface(crtc, mode, &[connector])
        .context("create_surface")?;
    tracing::info!(
        "TTY-сессия: {} режим {}x{}@{} — рендер пошёл (без vblank/ввода, см. ROADMAP)",
        path.display(),
        mode.size().0,
        mode.size().1,
        mode.vrefresh()
    );

    let mut display: smithay::reexports::wayland_server::Display<crate::state::CompositorState> =
        smithay::reexports::wayland_server::Display::new()?;
    let mut state = crate::state::CompositorState::new(&mut display, store);
    state.set_viewport(mode.size().0 as u32, mode.size().1 as u32);

    let frame_time = std::time::Duration::from_micros(16_666);
    loop {
        let t0 = std::time::Instant::now();
        if let Err(e) = present_frame(
            &mut state,
            &device,
            &surface,
            (mode.size().0 as u32, mode.size().1 as u32),
        ) {
            tracing::warn!("present_frame: {e:#}");
        }
        let _ = display.dispatch_clients(&mut state);
        let _ = display.flush_clients();
        let spent = t0.elapsed();
        if spent < frame_time {
            std::thread::sleep(frame_time - spent);
        }
    }
}
