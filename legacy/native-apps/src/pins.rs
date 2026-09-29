//! Built-in TAD apps pinned on the desktop.

use crate::{create_native_ro, create_native_vo, NativeAppKind};
use cgmath::Point2;
use tad_core::{RealObject, VirtualObject};

pub fn native_desktop_vos(desktop_ro: uuid::Uuid) -> (Vec<RealObject>, Vec<VirtualObject>) {
    let apps = [
        (NativeAppKind::Settings, Point2::new(50.0, 50.0)),
        (NativeAppKind::Files, Point2::new(400.0, 50.0)),
        (NativeAppKind::Calculator, Point2::new(750.0, 50.0)),
    ];
    let mut ros = Vec::new();
    let mut vos = Vec::new();
    for (kind, pos) in apps {
        let mut ro = create_native_ro(kind);
        crate::load_state_into_ro(&mut ro);
        let vo = create_native_vo(&ro, desktop_ro, pos);
        ros.push(ro);
        vos.push(vo);
    }
    (ros, vos)
}
