use crate::prelude::*;
use bevy_editor_cam::prelude::MinimalEditorCamPlugin;
use bevy::{
    feathers::FeathersPlugins,
    gizmos::config::GizmoConfig,
};

mod data;
mod panel;
mod screen;
mod systems;
mod terrain;
#[cfg(test)]
mod test;

pub mod prelude {
    pub use super::screen::HexPathsScreen;
}

/// Registers the hex paths prototype and its editable resources.
pub fn plugin(app: &mut App) {
    app.add_plugins((FeathersPlugins, MinimalEditorCamPlugin))
        .init_resource::<data::HexPathsSettings>()
        .init_resource::<data::HexTerrainTuning>()
        .init_resource::<data::HexPathsView>()
        .init_resource::<data::HexPathsAppearance>()
        .init_resource::<data::PanelState>()
        .register_type::<data::HexPathsSettings>()
        .register_type::<data::HexTerrainTuning>()
        .register_type::<data::HexPathsView>()
        .register_type::<data::HexPathsAppearance>()
        .register_screen::<HexPathsScreen>()
        .insert_gizmo_config(
            data::HexPathsGizmos,
            GizmoConfig {
                render_layers: RenderLayers::layer(31),
                ..default()
            },
        );
}
