use crate::prelude::*;

/// Enables cursor capture
#[derive(Component, Default)]
pub struct ICtxCaptureCursor {
    /// Accept a new capture click only after buttons held across a reset are released.
    pub(crate) armed: bool,
}

#[derive(InputAction)]
#[action_output(bool)]
pub struct PACaptureCursor;

#[derive(InputAction)]
#[action_output(bool)]
pub struct PAReleaseCursor;

#[derive(Event)]
pub struct SpawnCursorCapture;

/// Releases camera input on Escape, a mode switch, or a screen transition.
#[derive(Event)]
pub struct ReleaseCursor;
