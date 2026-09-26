mod bundle;

#[cfg(test)]
mod tests;

pub mod prelude {
    pub use super::bundle::flycam_bundle;
    pub use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraState};
}
