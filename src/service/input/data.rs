//! The global input context.

use crate::prelude::*;

/// Screen-local quit context installed by [`SpawnGlobalCtx`].
/// It stays active when movement and camera contexts are disabled.
#[derive(Component)]
pub struct ICtxGlobal;

/// Requests app exit when Escape completes with the cursor free.
/// [`PAReleaseCursor`] handles releasing a captured cursor.
#[derive(InputAction)]
#[action_output(bool)]
pub struct PAQuit;

/// Spawns a screen-scoped [`ICtxGlobal`]. Trigger it when a screen loads.
#[derive(Event)]
pub struct SpawnGlobalCtx;
