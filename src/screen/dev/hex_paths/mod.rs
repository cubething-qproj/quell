//! A dev screen for inspecting terrain-first hex trail generation.
//!
//! The generation pipeline is [`Region`](terrain::erosion::Region) → [`Terrain`](terrain::Terrain)
//! → [`Layout`](layout::Layout) → [`Board`](data::Board). Erosion simulates drainage across a
//! large region; window selection crops useful ground into a hex board; layout
//! places landmarks and trails over that fixed ground. Regeneration repeats
//! the layout stage with [`Pins`](layout::Pins), retaining the terrain.
//!
//! [`data`] contains settings, preview state, and marker components. [`systems`]
//! turns generated data into meshes, billboards, and overlays. [`screen`] owns
//! loading, update ordering, and cleanup; [`panel`] edits settings and requests
//! new layouts. Generation is synchronous; [`RegionCache`](data::RegionCache) reuses the last
//! matching erosion result. [`walking`] adds a separate player over the laid
//! corner graph without changing generation. The preview has no woodland sight
//! model, and its drawn trails still follow hex edges.
//!
//! The algorithm follows the round-11 design and hex-lab's `exp/15-density`,
//! `trunk-sight` arm; the relationships and pipelines are documented here.

use crate::prelude::*;
use bevy::{feathers::FeathersPlugins, prelude::*};
use bevy_editor_cam::prelude::MinimalEditorCamPlugin;

mod data;
mod layout;
mod panel;
mod screen;
mod systems;
mod terrain;
#[cfg(test)]
mod test;
mod walking;

/// The screen type exported to the dev-screen registry.
pub mod prelude {
    pub use super::screen::HexPathsScreen;
}

/// Registers [`HexPathsScreen`], its settings and preview resources.
///
/// Also installs UI, orbit-camera, and mesh-picking plugins, registers the
/// surface and marker rebuild observers, and configures the
/// separate [`HexPathsGizmos`](data::HexPathsGizmos) render layer. Generation is scheduled by
/// the screen, rather than running during plugin registration.
pub fn plugin(app: &mut App) {
    if !app.is_plugin_added::<MeshPickingPlugin>() {
        app.add_plugins(MeshPickingPlugin);
    }
    app.add_plugins((FeathersPlugins, MinimalEditorCamPlugin))
        .add_plugins((data::plugin, walking::plugin))
        .add_observer(systems::spawn_surface)
        .add_observer(systems::spawn_markers)
        .add_observer(systems::paint_markers)
        .register_screen::<HexPathsScreen>();
}
