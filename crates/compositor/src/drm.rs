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
