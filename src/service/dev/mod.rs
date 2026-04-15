use bevy_inspector_egui::{bevy_egui::EguiPlugin, quick::WorldInspectorPlugin};

use crate::prelude::*;

mod console;
mod gizmos;

pub mod prelude {
    pub use super::gizmos::prelude::*;
}

pub fn plugin(app: &mut App) {
    app.add_plugins((gizmos::plugin, console::plugin));
    app.add_plugins((EguiPlugin::default(), WorldInspectorPlugin::new()));
}
