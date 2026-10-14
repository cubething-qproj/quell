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
//! matching erosion result. The preview has no player walking or woodland sight
//! model, and its drawn trails still follow hex edges.
//!
//! The algorithm follows the round-11 design and hex-lab's `exp/15-density`,
//! `trunk-sight` arm; the relationships and pipelines are documented here.

use crate::prelude::*;
use bevy::feathers::FeathersPlugins;
use bevy_editor_cam::prelude::MinimalEditorCamPlugin;

mod data;
mod layout;
mod panel;
mod screen;
mod systems;
mod terrain;
#[cfg(test)]
mod test;

/// The screen type exported to the dev-screen registry.
pub mod prelude {
    pub use super::screen::HexPathsScreen;
}

/// Registers [`HexPathsScreen`], its settings and preview resources.
///
/// Also installs the billboard preview's UI and orbit-camera plugins and the
/// separate [`HexPathsGizmos`](data::HexPathsGizmos) render layer. Generation is scheduled by
/// the screen, rather than running during plugin registration.
pub fn plugin(app: &mut App) {
    app.add_plugins((FeathersPlugins, MinimalEditorCamPlugin))
        .add_plugins(data::plugin)
        .register_screen::<HexPathsScreen>();
}
