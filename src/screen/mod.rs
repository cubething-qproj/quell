//! Screen registration and the types used to request screen transitions.

use crate::prelude::*;

#[cfg(feature = "dev")]
mod dev;

mod splash;
mod world;

/// The screen types, for switching between screens.
pub mod prelude {
    pub use super::splash::prelude::*;
    pub use super::world::prelude::*;

    #[cfg(feature = "dev")]
    pub use super::dev::prelude::*;
}

/// Registers every screen, including the dev screens in `dev` builds.
pub fn plugin(app: &mut App) {
    #[cfg(feature = "dev")]
    app.add_plugins(dev::plugin);
    app.add_plugins(world::plugin);
    app.add_plugins(splash::plugin);
}
