//! A sandbox for switching between the free camera and the tracking camera.
//!
//! [`data`] holds the orbiting target and the switch-camera input context.
//! [`screen`] spawns the test scene and swaps which camera is active on Tab.

use crate::prelude::*;

mod data;
mod screen;

/// The screen type and the input context other sandboxes reuse.
pub mod prelude {
    pub use super::data::*;
    pub use super::screen::CameraTestScreen;
}

/// Registers [`CameraTestScreen`], its input context, and the camera switch.
pub fn plugin(app: &mut App) {
    app.register_screen::<CameraTestScreen>()
        .add_input_context::<CameraTestInput>()
        .add_observer(screen::switch_camera);
}
