use crate::prelude::*;
use bevy::{
    feathers::{FeathersPlugins, dark_theme::create_dark_theme, theme::UiTheme},
    gizmos::config::GizmoConfig,
};

mod data;
mod generate;
mod panel;
mod screen;
mod systems;
#[cfg(test)]
mod test;

pub mod prelude {
    pub use super::screen::HexPathsScreen;
}

/// Registers the hex paths prototype and its editable resources.
pub fn plugin(app: &mut App) {
    app.add_plugins(FeathersPlugins)
        .insert_resource(UiTheme(create_dark_theme()))
        .init_resource::<data::HexPathsSettings>()
        .init_resource::<data::HexPathsView>()
        .init_resource::<data::HexPathsAppearance>()
        .init_resource::<data::PanelState>()
        .add_message::<data::WalkTo>()
        .register_type::<data::HexPathsSettings>()
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
