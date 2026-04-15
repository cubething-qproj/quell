use crate::prelude::*;

#[cfg(feature = "dev")]
mod dev;

mod splash;
mod util;
mod world;

pub mod prelude {
    pub use super::splash::prelude::*;
    pub use super::world::prelude::*;

    #[allow(unused_imports)] // TEMP
    #[cfg(feature = "dev")]
    pub use super::dev::prelude::*;

    pub use super::util::*;
}

pub fn plugin(app: &mut App) {
    #[cfg(feature = "dev")]
    app.add_plugins(dev::plugin);
    app.add_plugins(world::plugin);
    app.add_plugins(splash::plugin);
}
