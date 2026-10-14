//! Components used by the shared billboard facing system.

use crate::prelude::*;

/// Marks a quad for [`face_camera`] to orient toward a selected camera.
///
/// Add it to a quad with a [`Transform`], and schedule `face_camera::<CameraMarker>`
/// in the owning screen. This marker does not create geometry or install a system.
///
/// A future `BillboardTarget` relationship could associate individual billboards
/// with different cameras if callers need more than one facing target.
#[derive(Component)]
pub struct Billboard;
