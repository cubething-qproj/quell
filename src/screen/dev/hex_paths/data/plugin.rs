//! Registration of preview resources, reflected settings, and the gizmo layer.

use super::{HexPathsAppearance, HexPathsGizmos, HexPathsView, PanelState, RegionCache};
use super::{HexPathsSettings, HexSettingsCollection, HexTerrainTuning};
use crate::prelude::*;
use bevy::gizmos::config::GizmoConfig;
use bevy_common_assets::ron::RonAssetPlugin;

/// Initialises the hex preview's data without scheduling generation or rendering.
pub fn plugin(app: &mut App) {
    app.add_plugins((
        RonAssetPlugin::<HexPathsSettings>::new(&["paths.ron"]),
        RonAssetPlugin::<HexTerrainTuning>::new(&["tuning.ron"]),
    ))
    .init_collection::<HexSettingsCollection>()
    .init_resource::<HexPathsView>()
    .init_resource::<HexPathsAppearance>()
    .init_resource::<PanelState>()
    .init_resource::<RegionCache>()
    .insert_gizmo_config(
        HexPathsGizmos,
        GizmoConfig {
            render_layers: RenderLayers::from(RenderLayer::HEX_PREVIEW),
            ..default()
        },
    );
}
