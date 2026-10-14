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
//! [`Exploration`](data::Exploration) restricts routing and trail drawing to
//! edges within sight, hides distant landmarks, and remembers cards on
//! arrival. The Reveal control overrides card images without granting route
//! knowledge. Player camera focus follows the walker's billboard.

use bevy::prelude::*;

pub(super) mod data;
mod exploration;
pub(super) mod systems;
#[cfg(test)]
mod test;

/// Registers walking controls; the screen schedules movement and cleanup.
pub(super) fn plugin(app: &mut App) {
    app.init_resource::<data::WalkingSettings>()
        .register_type::<data::WalkingSettings>();
}
