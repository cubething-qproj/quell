//! Player movement over the current layout's corner graph.
//!
//! A surface press asks [`Walk::plan`](data::Walk::plan) for a route through
//! [`Layout::walking_route`](super::layout::Layout::walking_route). The walker
//! owns its position and route separately from [`Board`](super::data::Board),
//! so advancing it does not rebuild the terrain or landmarks. [`systems`]
//! creates the player billboard, previews the hovered destination, handles
//! picking, and projects the interpolated player position onto the exaggerated
//! preview. Board replacement ends the walk.
//!
//! This slice exposes the whole network. Local discovery, arrival-based card
//! reveal, and a camera that follows movement belong to subsequent slices.

use bevy::prelude::*;

pub(super) mod data;
pub(super) mod systems;
#[cfg(test)]
mod test;

/// Registers walking controls; the screen schedules movement and cleanup.
pub(super) fn plugin(app: &mut App) {
    app.init_resource::<data::WalkingSettings>()
        .register_type::<data::WalkingSettings>();
}
