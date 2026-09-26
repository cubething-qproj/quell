use crate::prelude::*;

mod bundle;
mod events;

#[cfg(test)]
mod collision_tests;
#[cfg(test)]
mod tests;

pub mod prelude {
    pub use super::bundle::tracking_cam_bundle;
}

pub fn plugin(app: &mut App) {
    app.add_plugins(events::plugin)
        .add_input_context::<SpringArm>();
}
