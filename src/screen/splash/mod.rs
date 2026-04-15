use crate::prelude::*;

mod systems;
pub mod prelude {
    pub use super::systems::SplashScreen;
}

pub fn plugin(app: &mut App) {
    app.register_screen::<SplashScreen>();
}
