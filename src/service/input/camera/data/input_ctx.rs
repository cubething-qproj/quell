//! Camera input actions.

use crate::prelude::*;

/// Orbits the tracking camera.
#[derive(InputAction, Reflect)]
#[action_output(Vec2)]
pub struct PARotateCam;

/// Zooms the tracking camera by changing its field of view.
#[derive(InputAction, Reflect)]
#[action_output(f32)]
pub struct PAZoomCam;
