use crate::prelude::*;

mod data;
mod screen;

pub mod prelude {
    pub use super::data::*;
    pub use super::screen::CameraTestScreen;
}

pub fn plugin(app: &mut App) {
    app.register_screen::<CameraTestScreen>();
}
