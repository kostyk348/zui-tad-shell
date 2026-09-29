//! Native calculator TAD document.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tad_core::{ObjectKind, RealObject, Segment, TadDocument};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CalcState {
    pub display: String,
    pub accumulator: f64,
    pub pending_op: Option<char>,
    pub fresh: bool,
}

pub fn build_ro() -> RealObject {
    let mut ro = RealObject {
        id: Uuid::new_v4(),
        title: "Calculator".into(),
        kind: ObjectKind::Calculator,
        document: TadDocument::new(),
        meta: Default::default(),
        doc_size: (320.0, 420.0),
        created_at: now(),
        updated_at: now(),
    };
    let st = CalcState {
        display: "0".into(),
        fresh: true,
        ..Default::default()
    };
    write_doc(&mut ro.document, &st);
    ro.meta
        .insert("calc_state".into(), serde_json::to_string(&st).unwrap());
    ro
}

pub fn write_doc(doc: &mut TadDocument, st: &CalcState) {
    doc.root_segments.clear();
    doc.push(
        Segment::Heading {
            level: 1,
            text: "Calculator".into(),
        },
        10.0,
        10.0,
        300.0,
        32.0,
    );
    doc.push(
        Segment::Text {
            text: st.display.clone(),
        },
        10.0,
        50.0,
        300.0,
        48.0,
    );
    let keys = [
        "7", "8", "9", "/", "4", "5", "6", "*", "1", "2", "3", "-", "0", ".", "=", "+", "C",
    ];
    let grid: Vec<Vec<String>> = keys
        .chunks(4)
        .map(|row| row.iter().map(|s| (*s).into()).collect())
        .collect();
    doc.push(Segment::Table { rows: grid }, 10.0, 110.0, 300.0, 280.0);
}

pub fn handle_key(ch: char, st: &mut CalcState) -> bool {
    match ch {
        '0'..='9' => {
            if st.fresh {
                st.display = ch.to_string();
                st.fresh = false;
            } else {
                st.display.push(ch);
            }
            true
        }
        '.' if !st.display.contains('.') => {
            if st.fresh {
                st.display = "0.".into();
                st.fresh = false;
            } else {
                st.display.push('.');
            }
            true
        }
        '+' | '-' | '*' | '/' => {
            apply_pending(st);
            st.pending_op = Some(ch);
            st.fresh = true;
            true
        }
        '=' => {
            apply_pending(st);
            st.pending_op = None;
            st.fresh = true;
            true
        }
        'c' | 'C' => {
            *st = CalcState {
                display: "0".into(),
                fresh: true,
                ..Default::default()
            };
            true
        }
        _ => false,
    }
}

fn apply_pending(st: &mut CalcState) {
    let val: f64 = st.display.parse().unwrap_or(0.0);
    if let Some(op) = st.pending_op {
        st.accumulator = match op {
            '+' => st.accumulator + val,
            '-' => st.accumulator - val,
            '*' => st.accumulator * val,
            '/' => {
                if val.abs() < f64::EPSILON {
                    st.display = "Err".into();
                    return;
                }
                st.accumulator / val
            }
            _ => val,
        };
    } else {
        st.accumulator = val;
    }
    st.display = format_num(st.accumulator);
}

fn format_num(n: f64) -> String {
    if (n - n.round()).abs() < f64::EPSILON {
        format!("{}", n.round() as i64)
    } else {
        format!("{n:.6}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

pub fn init_meta(ro: &mut RealObject) {
    if !ro.meta.contains_key("calc_state") {
        let st = CalcState {
            display: "0".into(),
            fresh: true,
            ..Default::default()
        };
        ro.meta
            .insert("calc_state".into(), serde_json::to_string(&st).unwrap());
    }
}

pub fn persist_meta(ro: &mut RealObject) {
    if let Some(s) = ro.meta.get("calc_state") {
        if let Ok(st) = serde_json::from_str::<CalcState>(s) {
            write_doc(&mut ro.document, &st);
        }
    }
    ro.updated_at = now();
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
