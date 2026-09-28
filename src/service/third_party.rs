use crate::prelude::*;

pub mod prelude {
    pub use avian3d::prelude::*;
    pub use bevy::prelude::*;
    pub use bevy_asset_loader::prelude::*;
    pub use bevy_enhanced_input::prelude::*;
    // fix ambiguous glob exports
    pub use bevy_enhanced_input::prelude::{Cancel, Press, Release};
    pub use tiny_bail::prelude::*;
}

pub fn plugin(app: &mut App) {
    app.add_plugins(avian3d::PhysicsPlugins::default());
    app.add_plugins((EnhancedInputPlugin,));
}
