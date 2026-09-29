//! Context toolbar and cross-document VO operations.

use cgmath::Point2;
use editor_core::{editor_kind_for_segment, EditorKind, FocusTarget};
use skia_renderer::fonts::FontCache;
use tad_core::{ObjectKind, RealObject, RoId, Segment, VirtualObject, VoId};
use tiny_skia::Pixmap;

pub const TOOLBAR_HEIGHT: f32 = 36.0;

pub fn toolbar_label(kind: EditorKind) -> &'static str {
    match kind {
        EditorKind::Text => "Text editor",
        EditorKind::Vector => "Vector editor",
        EditorKind::Table => "Table editor",
        EditorKind::Image => "Image viewer",
        EditorKind::Portal => "Portal link",
        EditorKind::Custom => "Custom tool",
    }
}

/// Draw contextual micro-toolbar at the bottom edge of the viewport.
pub fn draw_context_toolbar(
    pm: &mut Pixmap,
    fonts: &FontCache,
    width: u32,
    height: u32,
    focus: FocusTarget,
    ro_index: &std::collections::HashMap<RoId, RealObject>,
    all_vos: &[VirtualObject],
) {
    let (label, hint) = match focus {
        FocusTarget::Segment(vo_id, seg_id) => {
            let Some(vo) = all_vos.iter().find(|v| v.id == vo_id) else { return; };
            let Some(ro) = ro_index.get(&vo.target_ro) else { return; };
            let Some(seg) = ro.document.root_segments.iter().find(|s| s.id == seg_id) else { return; };
            let kind = editor_kind_for_segment(&seg.segment);
            (
                toolbar_label(kind),
                "Ctrl+Z/Y undo/redo  Ctrl+S save  Tab next segment",
            )
        }
        FocusTarget::Vo(vo_id) => {
            let Some(vo) = all_vos.iter().find(|v| v.id == vo_id) else { return; };
            let Some(ro) = ro_index.get(&vo.target_ro) else { return; };
            let label = match ro.kind {
            ObjectKind::Mindmap => "Mindmap",
            ObjectKind::Table => "Table",
            ObjectKind::Text => "Document",
            ObjectKind::Portal => "Portal",
            ObjectKind::WaylandWindow => "Embedded app",
            ObjectKind::Folder => "Desktop map",
            ObjectKind::Image => "Image",
            ObjectKind::Files => "Files",
            ObjectKind::Calculator => "Calculator",
            ObjectKind::Settings => "Settings",
        };
            (label, "Zoom >0.8 for edit focus  Drag to move  Drop on doc to reparent")
        }
        FocusTarget::None => return,
    };

    use ab_glyph::PxScale;
    use tiny_skia::{Paint, Rect, Transform};
    let w = width as f32;
    let h = height as f32;
    let y0 = h - TOOLBAR_HEIGHT;

    let mut bg = Paint::default();
    bg.set_color_rgba8(0x12, 0x14, 0x1a, 0xf0);
    if let Some(r) = Rect::from_xywh(0.0, y0, w, TOOLBAR_HEIGHT) {
        pm.fill_rect(r, &bg, Transform::identity(), None);
    }
    let mut accent = Paint::default();
    accent.set_color_rgba8(0xff, 0xd7, 0x3a, 0xff);
    if let Some(r) = Rect::from_xywh(0.0, y0, w, 2.0) {
        pm.fill_rect(r, &accent, Transform::identity(), None);
    }

    skia_renderer::painter::Painter::draw_text(
        pm,
        &fonts.bold,
        label,
        16.0,
        y0 + 22.0,
        PxScale::from(14.0),
        [0xff, 0xd7, 0x3a, 0xff],
    );
    skia_renderer::painter::Painter::draw_text(
        pm,
        &fonts.regular,
        hint,
        180.0,
        y0 + 22.0,
        PxScale::from(12.0),
        [0xaa, 0xae, 0xb8, 0xff],
    );
}

/// If `dragged` is released over another document VO, reparent into that document.
pub fn try_reparent_vo(
    dragged: VoId,
    drop_world: Point2<f32>,
    culled: &[canvas_engine::CulledVo],
    ro_index: &std::collections::HashMap<RoId, RealObject>,
) -> Option<(VoId, RoId, Point2<f32>)> {
    let target = canvas_engine::hit_test(culled, drop_world)?;
    if target.vo.id == dragged {
        return None;
    }
    let dragged_parent = culled.iter().find(|c| c.vo.id == dragged).map(|c| c.vo.parent_ro)?;
    let drop_ro = target.vo.target_ro;
    if drop_ro == dragged_parent {
        return None;
    }
    let ro = ro_index.get(&drop_ro)?;
    if ro.kind == ObjectKind::WaylandWindow {
        return None;
    }
    // Local coordinates inside the target document.
    let local = Point2::new(
        drop_world.x - target.vo.pos.x + 20.0,
        drop_world.y - target.vo.pos.y + 20.0,
    );
    Some((dragged, drop_ro, local))
}
