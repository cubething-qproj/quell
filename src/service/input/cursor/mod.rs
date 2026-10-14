//! Cursor capture and release.

use crate::prelude::*;

mod data;
mod events;

/// The capture context, its actions, and events.
pub mod prelude {
    pub use super::data::*;
}

/// Registers the capture context and its observers.
pub fn plugin(app: &mut App) {
    app.add_plugins(events::plugin)
        .add_input_context::<ICtxCaptureCursor>();
}
