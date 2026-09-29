//! # Демонстрационный сценарий
//!
//! Воспроизводит 5-шаговый сценарий из ТЗ, сохраняя PNG на каждом шаге:
//!
//!   1. Общий вид: 3 проекта + порталы.
//!   2. Зум к Project 2 (mindmap).
//!   3. Редактирование текста в Project 2.
//!   4. Зум обратно, виден портал.
//!   5. Клик по порталу → полёт к Project 1.
//!   6. Drag таблицы из P1 в P2.

use crate::ShellState;
use anyhow::Result;
use canvas_engine::{cull, Camera};
use cgmath::{Point2, Vector2};
use editor_core::{FocusTarget, TextEditor, ToolCommand};
use skia_renderer::SkiaRenderer;
use tad_core::{RealObject, RoId, Segment, VirtualObject};

pub fn run_demo_scenario(state: &mut ShellState, snap_dir: &str) -> Result<()> {
    let mut renderer = SkiaRenderer::new(1600, 1000)?;

    // ── Шаг 1: общий вид ──────────────────────────────────────────────────
    state.camera.center = Point2::new(800.0, 500.0);
    state.camera.zoom = 0.35;
    render_snapshot(state, &mut renderer, &format!("{snap_dir}/01_overview.png"))?;
    eprintln!("[1/6] Обзор: zoom=0.35, видны 3 проекта + портал P1->P2");

    // ── Шаг 2: зум к Project 2 (mindmap) ──────────────────────────────────
    let p2_vo = state.all_vos.iter()
        .find(|v| v.target_ro == find_p2(state))
        .cloned()
        .unwrap();
    state.camera.center = Point2::new(
        p2_vo.pos.x + p2_vo.size.x * 0.5,
        p2_vo.pos.y + p2_vo.size.y * 0.5,
    );
    state.camera.zoom = 1.0;
    render_snapshot(state, &mut renderer, &format!("{snap_dir}/02_zoom_p2.png"))?;
    eprintln!("[2/6] Зум к mindmap: focus активирован, scale>0.8");

    // ── Шаг 3: редактирование текста в Project 2 ──────────────────────────
    let p2_id = find_p2(state);
    let ro = state.ro_index.get(&p2_id).cloned().unwrap();
    let mut text_editor = TextEditor::new(ro.document.clone());
    if let Some(seg) = ro.document.root_segments.iter()
        .find(|s| matches!(s.segment, Segment::Link { .. })) {
        text_editor.activate_segment(seg.id);
        for ch in " (обновлено)".chars() {
            text_editor.handle_char(ch);
        }
    }
    // Записываем изменённый документ обратно.
    if let Some(ro) = state.ro_index.get_mut(&p2_id) {
        ro.document = text_editor.doc.clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        ro.updated_at = now;
        let ro_clone = ro.clone();
        state.store.lock().put_ro(&ro_clone)?;
    }
    render_snapshot(state, &mut renderer, &format!("{snap_dir}/03_edited.png"))?;
    eprintln!("[3/6] Текст в P2 отредактирован (добавлено '(обновлено)')");

    // ── Шаг 4: отдаляем, виден портал ─────────────────────────────────────
    state.camera.center = Point2::new(800.0, 500.0);
    state.camera.zoom = 0.20;
    render_snapshot(state, &mut renderer, &format!("{snap_dir}/04_zoom_out.png"))?;
    eprintln!("[4/6] Отдалили: zoom=0.20, видны иконки всех проектов");

    // ── Шаг 5: клик по порталу → полёт к Project 1 ────────────────────────
    let p1_id = find_p1(state);
    let p1_vo = state.all_vos.iter()
        .find(|v| v.target_ro == p1_id)
        .cloned()
        .unwrap();
    // Анимируем полёт: 60 кадров, 60 fps.
    let start_center = state.camera.center;
    let start_zoom = state.camera.zoom;
    let target_center = Point2::new(
        p1_vo.pos.x + p1_vo.size.x * 0.5,
        p1_vo.pos.y + p1_vo.size.y * 0.5,
    );
    let target_zoom = 1.0;
    let total_frames = 45;
    for i in 0..=total_frames {
        let t = i as f32 / total_frames as f32;
        let e = if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 };
        state.camera.center = Point2::new(
            start_center.x + (target_center.x - start_center.x) * e,
            start_center.y + (target_center.y - start_center.y) * e,
        );
        state.camera.zoom = start_zoom + (target_zoom - start_zoom) * e;
        if i % 5 == 0 || i == total_frames {
            render_snapshot(state, &mut renderer,
                &format!("{snap_dir}/05_fly_{:02}.png", i))?;
        }
    }
    eprintln!("[5/6] Полёт к P1: 10 кадров анимации сохранено");

    // ── Шаг 6: drag таблицы из P1 в P2 (переносим VO-ссылку) ─────────────
    // Берём VO, который указывает на P3 (таблица бюджета) — переносим его
    // в зону Project 2, создавая новую связь P2 -> P3.
    let p3_id = find_p3(state);
    let p3_vo_on_desktop = state.all_vos.iter()
        .find(|v| v.target_ro == p3_id)
        .cloned()
        .unwrap();

    let p2_vo = state.all_vos.iter()
        .find(|v| v.target_ro == p2_id)
        .cloned()
        .unwrap();

    // Создаём новый VO внутри P2, указывающий на P3.
    let new_vo = VirtualObject::new(
        p3_id,
        p2_id,
        Point2::new(400.0, 400.0),
        Vector2::new(400.0, 300.0),
    );
    state.store.lock().put_vo(&new_vo)?;
    state.all_vos.push(new_vo);

    // Перемещаем камеру, чтобы видеть обе P2 и новую таблицу.
    state.camera.center = Point2::new(
        p2_vo.pos.x + p2_vo.size.x * 0.5,
        p2_vo.pos.y + p2_vo.size.y * 0.5,
    );
    state.camera.zoom = 0.8;
    render_snapshot(state, &mut renderer, &format!("{snap_dir}/06_drag_table.png"))?;
    eprintln!("[6/6] Таблица P3 перенесена в P2 как новая VO-ссылка");

    // ── Финальный «скриншот» ─────────────────────────────────────────────
    state.camera.center = Point2::new(800.0, 500.0);
    state.camera.zoom = 0.35;
    render_snapshot(state, &mut renderer, &format!("{snap_dir}/00_final.png"))?;
    eprintln!("[done] Финальный скриншот сохранён");

    Ok(())
}

fn render_snapshot(state: &mut ShellState, r: &mut SkiaRenderer, path: &str) -> Result<()> {
    let culled = cull(&state.camera, &state.all_vos);
    let frame = r.render(&state.camera, &culled, &state.ro_index);
    frame.pixmap.save_png(path)?;
    Ok(())
}

fn find_p1(state: &ShellState) -> RoId {
    state.ro_index.values()
        .find(|ro| ro.title.contains("Проект 1"))
        .map(|ro| ro.id)
        .unwrap()
}
fn find_p2(state: &ShellState) -> RoId {
    state.ro_index.values()
        .find(|ro| ro.title.contains("Проект 2"))
        .map(|ro| ro.id)
        .unwrap()
}
fn find_p3(state: &ShellState) -> RoId {
    state.ro_index.values()
        .find(|ro| ro.title.contains("Проект 3"))
        .map(|ro| ro.id)
        .unwrap()
}
