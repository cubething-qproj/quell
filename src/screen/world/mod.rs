//! The main game screen.

use crate::prelude::*;

mod screen;

/// The screen type.
pub mod prelude {
    pub use super::screen::WorldScreen;
}

/// Registers [`WorldScreen`] and its assets.
pub fn plugin(app: &mut App) {
    app.add_plugins(screen::plugin);
}
