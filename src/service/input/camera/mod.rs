//! Camera input: the tracking camera, plus the free camera in `dev` builds.
//!
//! Tracking uses [`SpringArm`] from q_cam; the dev free camera uses Bevy's
//! controller with q_cam's input gating. Bundles configure render layers and
//! input bindings, while the cursor service controls capture and activation.

use crate::prelude::*;
#[cfg(feature = "dev")]
use q_cam::free::FreeCameraInputPlugin;

mod data;
#[cfg(feature = "dev")]
mod fly;
mod tracking;

/// Camera bundles, render layers, camera order, and q_cam re-exports.
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

/// Installs tracking input and spring-arm collision against [`CollisionLayer::Default`],
/// excluding the player. Also installs free-camera input in `dev` builds.
pub fn plugin(app: &mut App) {
    app.insert_resource(SpringArmCollisionFilter {
        included: CollisionLayer::Default.into(),
        excluded_memberships: CollisionLayer::Player.into(),
    })
    .add_plugins((SpringArmCameraPlugin, tracking::plugin));
    #[cfg(feature = "dev")]
    app.add_plugins(FreeCameraInputPlugin);
}
