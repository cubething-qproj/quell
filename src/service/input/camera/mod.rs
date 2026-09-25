use crate::prelude::*;

mod data;
#[cfg(feature = "dev")]
mod fly;
mod tracking;

pub mod prelude {

    pub use super::data::*;
    #[cfg(feature = "dev")]
    pub use super::fly::prelude::*;
    pub use super::tracking::prelude::*;
    #[doc(hidden)]
    pub use bevy::camera::visibility::RenderLayers;
}

pub fn plugin(app: &mut App) {
    app.add_plugins(tracking::plugin);
    #[cfg(feature = "dev")]
    app.add_plugins(fly::plugin);
}
