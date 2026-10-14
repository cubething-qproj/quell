//! Game-service registration and the APIs screens use to compose behaviour.
//!
//! Plugins install shared data, observers, and component-driven global updates;
//! screens schedule the additional systems exposed through
//! [`ServiceSystems`](crate::prelude::ServiceSystems).

use crate::prelude::*;

mod data;

#[cfg(feature = "dev")]
mod dev;
mod input;
#[cfg(all(test, feature = "dev"))]
pub(crate) use input::plugin as input_plugin;
pub mod player;
mod third_party;
mod ui;
mod util;

/// Every service's public types, plus the external-crate re-exports.
pub mod prelude {
    pub use super::data::*;
    pub use super::input::prelude::*;
    pub use super::player::prelude::*;
    pub use super::third_party::prelude::*;
    pub use super::util::*;
}

/// Adds every service, plus dev tooling in `dev` builds.
pub fn plugin(app: &mut App) {
    app.add_plugins((third_party::plugin, input::plugin, ui::plugin, player::plugin));
    #[cfg(feature = "dev")]
    app.add_plugins(dev::plugin);
}
