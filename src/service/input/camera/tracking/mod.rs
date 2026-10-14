//! The tracking camera: a spring arm following a target.

use crate::prelude::*;

mod bundle;
mod events;

#[cfg(test)]
mod collision_tests;
#[cfg(test)]
mod tests;

/// The tracking-camera bundle.
pub mod prelude {
    pub use super::bundle::tracking_cam_bundle;
}

/// Registers the tracking camera's input context and its observers.
pub fn plugin(app: &mut App) {
    app.add_plugins(events::plugin)
        .add_input_context::<SpringArm>();
}
