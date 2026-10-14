//! A sandbox for walking the player around a Jackdaw-authored greybox level.

use crate::prelude::*;

mod screen;

/// The screen type.
pub mod prelude {
    pub use super::screen::GreyboxArenaScreen;
}

/// Registers [`GreyboxArenaScreen`].
pub fn plugin(app: &mut App) {
    app.register_screen::<GreyboxArenaScreen>();
}
