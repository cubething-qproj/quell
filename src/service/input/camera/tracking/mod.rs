use crate::prelude::*;

mod bundle;
mod data;
mod events;
mod systems;

#[cfg(test)]
mod collision_tests;
#[cfg(test)]
mod tests;

pub mod prelude {
    pub use super::bundle::tracking_cam_bundle;
    pub use super::data::SpringArm;
}

pub fn plugin(app: &mut App) {
    app.init_resource::<PlayerCameraSettings>()
        .register_type::<PlayerCameraSettings>()
        .add_plugins((events::plugin, systems::plugin))
        .add_input_context::<SpringArm>();
}
