use crate::prelude::*;

/// PlayerAction_RotateCamera
#[derive(InputAction, Reflect)]
#[action_output(Vec2)]
pub struct PARotateCam;

/// PlayerAction_ZoomCamera
#[derive(InputAction, Reflect)]
#[action_output(f32)]
pub struct PAZoomCam;
