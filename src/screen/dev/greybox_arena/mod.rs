use crate::prelude::*;

mod screen;

pub mod prelude {
    pub use super::screen::GreyboxArenaScreen;
}

pub fn plugin(app: &mut App) {
    app.register_screen::<GreyboxArenaScreen>();
}
