//! Dev-only tooling: the egui world inspector.

use bevy_inspector_egui::{
    bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPreUpdateSet},
    quick::WorldInspectorPlugin,
};

use crate::prelude::*;

/// Adds the world inspector and orders input so egui absorbs the input it uses.
pub fn plugin(app: &mut App) {
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
