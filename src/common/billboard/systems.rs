//! Camera-facing orientation shared by screens that render billboard quads.

use super::data::Billboard;
use crate::prelude::*;

/// Copies the local rotation of the entity marked by `C` to every [`Billboard`].
///
/// Requires exactly one matching entity with a [`Transform`] and without
/// [`Billboard`]; otherwise the system skips. `C` chooses the camera, without
/// testing [`Camera::is_active`]. Billboards are not filtered by screen, and must
/// use a coordinate frame compatible with the camera's local rotation.
pub fn face_camera<C: Component>(
    camera: Single<&Transform, (With<C>, Without<Billboard>)>,
    mut billboards: Query<&mut Transform, With<Billboard>>,
) {
    for mut transform in &mut billboards {
        transform.rotation = camera.rotation;
    }
}
