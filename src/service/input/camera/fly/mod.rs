use crate::prelude::*;
use bevy::camera_controller::free_camera::FreeCameraPlugin;

mod bundle;
mod data;
mod events;

#[cfg(test)]
mod tests;

pub mod prelude {
    pub use super::bundle::flycam_bundle;
    pub(crate) use super::data::FreeCameraInput;
    pub use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraState};
}

pub fn plugin(app: &mut App) {
    app.add_plugins((FreeCameraPlugin, events::plugin));
}
