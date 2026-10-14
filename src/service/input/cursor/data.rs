//! Cursor capture: its input context, actions, and events.

use crate::prelude::*;

/// Screen-local capture context installed by [`SpawnCursorCapture`].
/// Its armed state prevents clicks begun before a reset from recapturing input.
#[derive(Component, Default)]
pub struct ICtxCaptureCursor {
    /// Accept a new capture click only after buttons held across a reset are released.
    pub(crate) armed: bool,
}

/// Requests tracking-camera capture when a left-click gesture completes.
#[derive(InputAction)]
#[action_output(bool)]
pub struct PACaptureCursor;

/// Releases the cursor on Escape.
#[derive(InputAction)]
#[action_output(bool)]
pub struct PAReleaseCursor;

/// Spawns a screen-scoped [`ICtxCaptureCursor`]. Trigger it when a screen loads.
#[derive(Event)]
pub struct SpawnCursorCapture;

/// Releases camera input on Escape, a mode switch, or a screen transition.
#[derive(Event)]
pub struct ReleaseCursor;
