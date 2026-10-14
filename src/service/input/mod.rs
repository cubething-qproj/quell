//! Input: the global context, cursor capture, and camera controls.
//!
//! Screens request [`ICtxGlobal`] and [`ICtxCaptureCursor`] through their spawn
//! events. Camera and player contexts follow the selected camera and window
//! focus. Active contexts evaluate in [`ContextPriority`] order.

use crate::prelude::*;

mod camera;
mod cursor;
mod data;
mod events;

/// Input contexts, actions, events, and camera bundles.
pub mod prelude {
    pub use super::camera::prelude::*;
    pub use super::cursor::prelude::*;
    pub use super::data::*;
}

/// Registers the global context, cursor capture, and camera input.
pub fn plugin(app: &mut App) {
    app.add_plugins(events::plugin)
        .add_plugins((cursor::plugin, camera::plugin))
        .add_input_context::<ICtxGlobal>();
}
