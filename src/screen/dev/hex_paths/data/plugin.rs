//! Registration of preview resources, reflected settings, and the gizmo layer.

use super::{
    HexPathsAppearance, HexPathsGizmos, HexPathsSettings, HexPathsView, HexTerrainTuning,
    PanelState, RegionCache,
};
use crate::prelude::*;
use bevy::gizmos::config::GizmoConfig;

/// Initialises the hex preview's data without scheduling generation or rendering.
pub fn plugin(app: &mut App) {
    app.init_resource::<HexPathsSettings>()
        .init_resource::<HexTerrainTuning>()
        .init_resource::<HexPathsView>()
        .init_resource::<HexPathsAppearance>()
        .init_resource::<PanelState>()
        .init_resource::<RegionCache>()
        .register_type::<HexPathsSettings>()
        .register_type::<HexTerrainTuning>()
        .register_type::<HexPathsView>()
        .register_type::<HexPathsAppearance>()
        .insert_gizmo_config(
            HexPathsGizmos,
            GizmoConfig {
                render_layers: RenderLayers::layer(31),
                ..default()
            },
        );
}
