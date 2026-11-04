use crate::prelude::*;

mod scene;
mod events;

#[cfg(test)]
mod collision_tests;
#[cfg(test)]
mod tests;

pub mod prelude {
    pub use super::scene::tracking_cam_scene;
}

pub fn plugin(app: &mut App) {
    app.add_plugins(events::plugin)
        .add_input_context::<SpringArm>();
}
