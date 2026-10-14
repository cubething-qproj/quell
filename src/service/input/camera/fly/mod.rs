//! The free camera, for `dev` builds.

mod bundle;

#[cfg(test)]
mod tests;

/// The free-camera bundle and Bevy's free-camera types.
pub mod prelude {
    pub use super::bundle::flycam_bundle;
    pub use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraState};
}
