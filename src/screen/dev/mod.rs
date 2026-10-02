mod camera_test;
mod greybox_arena;
use crate::prelude::*;

pub mod prelude {
    pub use super::camera_test::prelude::*;
    pub use super::greybox_arena::prelude::*;
}

pub fn plugin(app: &mut App) {
    app.add_plugins(camera_test::plugin);
    app.add_plugins(greybox_arena::plugin);
}
