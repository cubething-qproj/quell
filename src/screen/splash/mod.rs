//! The splash screen, the default starting screen.

use crate::prelude::*;

mod systems;
/// The screen type.
pub mod prelude {
    pub use super::systems::SplashScreen;
}

/// Registers [`SplashScreen`].
pub fn plugin(app: &mut App) {
    app.register_screen::<SplashScreen>();
}
