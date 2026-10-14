//! Dev-only sandbox screens, each isolating one feature with its own debug
//! tooling. Start on one with `--screen <name>`.

mod camera_test;
mod greybox_arena;
mod hex_paths;
use crate::prelude::*;

/// The dev screen types.
pub mod prelude {
    pub use super::camera_test::prelude::*;
    pub use super::greybox_arena::prelude::*;
    pub use super::hex_paths::prelude::*;
}

/// Registers the dev screens.
pub fn plugin(app: &mut App) {
    app.add_plugins(camera_test::plugin);
    app.add_plugins(greybox_arena::plugin);
    app.add_plugins(hex_paths::plugin);
}
