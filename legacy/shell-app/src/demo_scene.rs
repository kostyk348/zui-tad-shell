//! # Демонстрационная сцена
//!
//! Генерирует 3 RO и связи между ними:
//!
//!   Project 1 (Text):    текстовый документ с таблицей расходов + портал к Project 2.
//!   Project 2 (Mindmap): mindmap-схема идей со стрелками-ссылками на заметки.
//!   Project 3 (Table):   отдельный табличный документ с бюджетом.
//!
//! Граф связей (циклический!):
//!   Project1 ──portal──▶ Project2
//!   Project2 ──link───▶ Project1   (mindmap ссылается на текст)
//!   Project1 ──link───▶ Project3   (текст ссылается на таблицу)

use cgmath::{Point2, Vector2};
use tad_core::{
    ObjectKind, RealObject, RoId, Segment, TadDocument, VectorKind, VectorShape, VirtualObject,
};
use uuid::Uuid;

use native_apps;

pub struct DemoScene {
    pub desktop_ro: RoId,
    pub project1_ro: RoId,
    pub project2_ro: RoId,
    pub project3_ro: RoId,
    /// Все RO, готовые к записи в GraphStore.
    pub ros: Vec<RealObject>,
    /// Все VO.
    pub vos: Vec<VirtualObject>,
}

pub fn build_demo_scene() -> DemoScene {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let p1_id = Uuid::new_v4();
    let p2_id = Uuid::new_v4();
    let p3_id = Uuid::new_v4();

    // ── Project 1: Text + table + portal ─────────────────────────────────
    let mut p1_doc = TadDocument::new();
    p1_doc.push(
        Segment::Heading {
            level: 1,
            text: "Проект 1: Отчёт о расходах".into(),
        },
        20.0,
        20.0,
        760.0,
        40.0,
    );
    p1_doc.push(
        Segment::Text {
            text: "Сводный отчёт за Q1 2025. Включает прямые и косвенные расходы.".into(),
        },
        20.0,
        70.0,
        760.0,
        40.0,
    );
    p1_doc.push(
        Segment::Table {
            rows: vec![
                vec!["Категория".into(), "Сумма".into(), "Дата".into()],
                vec!["Серверы".into(), "120 000 ₽".into(), "2025-01-15".into()],
                vec!["Лицензии".into(), "45 000 ₽".into(), "2025-02-01".into()],
                vec!["Маркетинг".into(), "200 000 ₽".into(), "2025-03-10".into()],
                vec!["Итого".into(), "365 000 ₽".into(), "—".into()],
            ],
        },
        20.0,
        130.0,
        760.0,
        200.0,
    );
    p1_doc.push(
        Segment::Link {
            target_ro: p2_id,
            label: "→ Портал: Проект 2 (Mindmap)".into(),
        },
        20.0,
        360.0,
        420.0,
        36.0,
    );
    p1_doc.push(
        Segment::Link {
            target_ro: p3_id,
            label: "→ Проект 3: Бюджет Q2".into(),
        },
        20.0,
        410.0,
        420.0,
        36.0,
    );
    let p1 = RealObject {
        id: p1_id,
        title: "Проект 1 — Отчёт о расходах".into(),
        kind: ObjectKind::Text,
        document: p1_doc,
        meta: Default::default(),
        doc_size: (800.0, 600.0),
        created_at: now,
        updated_at: now,
    };

    // ── Project 2: Mindmap ───────────────────────────────────────────────
    let mut p2_doc = TadDocument::new();
    // Главная нода mindmap — прямоугольник в центре.
    let mut shapes = Vec::new();
    shapes.push(VectorShape {
        kind: VectorKind::Rect {
            x: 350.0,
            y: 250.0,
            w: 200.0,
            h: 60.0,
        },
        stroke: Some([0.95, 0.85, 0.3, 1.0]),
        fill: Some([0.4, 0.3, 0.1, 0.8]),
    });
    shapes.push(VectorShape {
        kind: VectorKind::Text {
            x: 370.0,
            y: 280.0,
            text: "Идея: ZUI-Shell".into(),
            size: 18.0,
        },
        stroke: Some([1.0, 1.0, 1.0, 1.0]),
        fill: None,
    });
    // Три ветки — линии + ноды.
    let branches = [
        ("Концепция", 50.0, 80.0),
        ("Реализация", 600.0, 80.0),
        ("Демо", 350.0, 450.0),
    ];
    for (label, bx, by) in branches {
        shapes.push(VectorShape {
            kind: VectorKind::Rect {
                x: bx,
                y: by,
                w: 160.0,
                h: 40.0,
            },
            stroke: Some([0.4, 0.7, 0.9, 1.0]),
            fill: Some([0.1, 0.2, 0.3, 0.8]),
        });
        shapes.push(VectorShape {
            kind: VectorKind::Line {
                x1: 450.0,
                y1: 280.0,
                x2: bx + 80.0,
                y2: by + 20.0,
                width: 2.0,
            },
            stroke: Some([0.6, 0.7, 0.8, 0.8]),
            fill: None,
        });
        shapes.push(VectorShape {
            kind: VectorKind::Text {
                x: bx + 10.0,
                y: by + 25.0,
                text: label.into(),
                size: 14.0,
            },
            stroke: Some([1.0, 1.0, 1.0, 1.0]),
            fill: None,
        });
    }
    p2_doc.push(Segment::Vector { shapes }, 0.0, 0.0, 800.0, 600.0);
    p2_doc.push(
        Segment::Link {
            target_ro: p1_id,
            label: "→ Подробнее в Проекте 1".into(),
        },
        20.0,
        550.0,
        400.0,
        30.0,
    );

    let p2 = RealObject {
        id: p2_id,
        title: "Проект 2 — Mindmap".into(),
        kind: ObjectKind::Mindmap,
        document: p2_doc,
        meta: Default::default(),
        doc_size: (800.0, 600.0),
        created_at: now,
        updated_at: now,
    };

    // ── Project 3: Standalone table ──────────────────────────────────────
    let mut p3_doc = TadDocument::new();
    p3_doc.push(
        Segment::Heading {
            level: 1,
            text: "Проект 3: Бюджет Q2".into(),
        },
        20.0,
        20.0,
        760.0,
        40.0,
    );
    p3_doc.push(
        Segment::Table {
            rows: vec![
                vec!["Статья".into(), "План".into(), "Факт".into(), "Δ".into()],
                vec![
                    "Разработка".into(),
                    "300 000 ₽".into(),
                    "310 000 ₽".into(),
                    "+10 000".into(),
                ],
                vec![
                    "Инфраструктура".into(),
                    "150 000 ₽".into(),
                    "140 000 ₽".into(),
                    "−10 000".into(),
                ],
                vec![
                    "Дизайн".into(),
                    "100 000 ₽".into(),
                    "105 000 ₽".into(),
                    "+5 000".into(),
                ],
            ],
        },
        20.0,
        80.0,
        760.0,
        200.0,
    );

    let p3 = RealObject {
        id: p3_id,
        title: "Проект 3 — Бюджет Q2".into(),
        kind: ObjectKind::Table,
        document: p3_doc,
        meta: Default::default(),
        doc_size: (800.0, 600.0),
        created_at: now,
        updated_at: now,
    };

    // ── Desktop RO (контейнер всех трёх VO) ──────────────────────────────
    let desktop_id = Uuid::new_v4();
    let mut desktop_doc = TadDocument::new();
    desktop_doc.push(
        Segment::Heading {
            level: 1,
            text: "Рабочий стол".into(),
        },
        50.0,
        50.0,
        700.0,
        60.0,
    );
    desktop_doc.push(
        Segment::Text {
            text: "Главная карта проектов. Зум к любому проекту для редактирования.".into(),
        },
        50.0,
        110.0,
        700.0,
        30.0,
    );

    let desktop = RealObject {
        id: desktop_id,
        title: "Desktop".into(),
        kind: ObjectKind::Folder,
        document: desktop_doc,
        meta: Default::default(),
        doc_size: (1600.0, 1000.0),
        created_at: now,
        updated_at: now,
    };

    // ── VO: размещение трёх проектов на холсте Desktop ───────────────────
    let vo1 = VirtualObject::new(
        p1_id,
        desktop_id,
        Point2::new(100.0, 200.0),
        Vector2::new(800.0, 600.0),
    );
    let vo2 = VirtualObject::new(
        p2_id,
        desktop_id,
        Point2::new(1000.0, 200.0),
        Vector2::new(800.0, 600.0),
    );
    let vo3 = VirtualObject::new(
        p3_id,
        desktop_id,
        Point2::new(550.0, 900.0),
        Vector2::new(800.0, 600.0),
    );

    // ── Внутренние VO: портал внутри Project 1 на Project 2 ──────────────
    let mut p1_inner_vo = VirtualObject::new(
        p2_id,
        p1_id,
        Point2::new(500.0, 400.0),
        Vector2::new(300.0, 200.0),
    );
    p1_inner_vo.scale = 1.0;
    p1_inner_vo.z = 1;

    let (mut native_ros, native_vos) = native_apps::pins::native_desktop_vos(desktop_id);

    let mut ros = vec![desktop, p1, p2, p3];
    ros.append(&mut native_ros);
    let mut vos = vec![vo1, vo2, vo3, p1_inner_vo];
    vos.extend(native_vos);

    DemoScene {
        desktop_ro: desktop_id,
        project1_ro: p1_id,
        project2_ro: p2_id,
        project3_ro: p3_id,
        ros,
        vos,
    }
}
