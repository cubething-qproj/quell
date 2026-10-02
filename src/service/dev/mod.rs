use bevy_inspector_egui::{
    bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPreUpdateSet},
    quick::WorldInspectorPlugin,
};

use crate::prelude::*;

// mod console;
// mod gizmos;

pub mod prelude {
    // pub use super::gizmos::prelude::*;
}

pub fn plugin(app: &mut App) {
    // app.add_plugins((gizmos::plugin, console::plugin));
    // Egui clears mouse/keyboard input it's using, so clicks and typing in the
    // inspector don't reach gameplay (e.g. cursor capture). Enhanced input must
    // read after that.
    app.insert_resource(EguiGlobalSettings {
        enable_absorb_bevy_input_system: true,
        ..default()
    })
    .configure_sets(
        PreUpdate,
        EnhancedInputSystems::Prepare.after(EguiPreUpdateSet::ProcessInput),
    );
    app.add_plugins((EguiPlugin::default(), WorldInspectorPlugin::new()));
}
