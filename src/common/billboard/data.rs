//! Components used by the shared billboard facing system.

use crate::prelude::*;

/// Marks a quad whose rotation follows the camera selected by its facing system.
///
/// Schedule [`face_camera`] with the screen's camera
/// marker type. It copies that camera's rotation; spawning, materials, and
/// screen ownership remain the caller's responsibility. If several cameras
/// need distinct billboard sets, a `BillboardTarget` relationship could
/// associate each billboard with its intended camera instead.
#[derive(Component)]
pub struct Billboard;
