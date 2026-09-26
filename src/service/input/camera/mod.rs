use crate::prelude::*;
#[cfg(feature = "dev")]
use q_cam::free::FreeCameraInputPlugin;

mod data;
#[cfg(feature = "dev")]
mod fly;
mod tracking;

pub mod prelude {

    pub use super::data::*;
    #[cfg(feature = "dev")]
    pub use super::fly::prelude::*;
    pub use super::tracking::prelude::*;
    #[doc(hidden)]
    pub use bevy::camera::visibility::RenderLayers;

    pub use q_cam::tracking::{
        SpringArm, SpringArmCameraPlugin, SpringArmCameraSettings, SpringArmCollisionFilter,
    };
}

pub fn plugin(app: &mut App) {
    app.insert_resource(SpringArmCollisionFilter {
        included: CollisionLayer::Default.into(),
        excluded_memberships: CollisionLayer::Player.into(),
    })
    .add_plugins((SpringArmCameraPlugin, tracking::plugin));
    #[cfg(feature = "dev")]
    app.add_plugins(FreeCameraInputPlugin);
}
