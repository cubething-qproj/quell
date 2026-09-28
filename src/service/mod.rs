use crate::prelude::*;

mod data;

mod dev;
mod input;
#[cfg(test)]
pub(crate) use input::plugin as input_plugin;
pub mod player;
mod third_party;
mod ui;
mod util;
mod worldgen;

pub mod prelude {
    pub use super::data::*;
    pub use super::dev::prelude::*;
    pub use super::input::prelude::*;
    pub use super::player::prelude::*;
    pub use super::third_party::prelude::*;
    pub use super::util::*;
    pub use super::worldgen::prelude::*;
}

pub fn plugin(app: &mut App) {
    app.add_plugins((
        third_party::plugin,
        input::plugin,
        ui::plugin,
        worldgen::plugin,
        player::plugin,
        dev::plugin,
    ));
}
