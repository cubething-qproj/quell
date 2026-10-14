//! Camera-facing orientation shared by screens that render billboard quads.

use super::data::Billboard;
use crate::prelude::*;

/// Turns every [`Billboard`] toward the single camera marked by `C`.
///
/// The caller schedules this within the relevant screen, selecting that
/// screen's active camera marker. It skips when no unique camera matches,
/// and excludes billboards from the camera query to keep transform access
/// disjoint. A future `BillboardTarget` relationship could support distinct
/// targets per billboard when multiple cameras must coexist.
pub fn face_camera<C: Component>(
    camera: Single<&Transform, (With<C>, Without<Billboard>)>,
    mut billboards: Query<&mut Transform, With<Billboard>>,
) {
    for mut transform in &mut billboards {
        transform.rotation = camera.rotation;
    }
}
