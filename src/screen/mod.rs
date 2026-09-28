use crate::prelude::*;

mod dev;

mod splash;
mod world;

pub mod prelude {
    pub use super::splash::prelude::*;
    pub use super::world::prelude::*;

    #[allow(unused_imports)] // TEMP
    pub use super::dev::prelude::*;
}

pub fn plugin(app: &mut App) {
    app.add_plugins(dev::plugin);
    app.add_plugins(world::plugin);
    app.add_plugins(splash::plugin);
}
