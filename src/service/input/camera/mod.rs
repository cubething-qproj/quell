use crate::prelude::*;

mod controller;
mod data;
#[cfg(feature = "dev")]
mod fly;
mod tracking;

pub mod prelude {
    pub use super::camera_systems;
    pub use super::controller::prelude::*;
    pub use super::data::*;
    #[cfg(feature = "dev")]
    pub use super::fly::prelude::*;
    pub use super::tracking::prelude::*;
    #[doc(hidden)]
    pub use bevy::camera::visibility::RenderLayers;
}

/// Adds in all camera systems to this screen.
/// Currently only consists of tracking camera systems.
pub fn camera_systems() -> ServiceSystems {
    ServiceSystems::new(tracking_cam_systems().take())
}

pub fn plugin(app: &mut App) {
    app.add_plugins((tracking::plugin, controller::plugin))
        .init_resource::<ActiveCamera>();
    #[cfg(feature = "dev")]
    app.add_plugins(fly::plugin);
}
