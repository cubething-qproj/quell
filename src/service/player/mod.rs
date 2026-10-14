//! The player character.
//!
//! Inserting [`PlayerSpawn`] requests a [`PlayerController`] at the scene's
//! spawn point. Its setup hook installs the model, collider, input, and camera.
//! Input observers record movement; [`player_systems`] convert it to world-space
//! [`PlayerMotor`] intent. Fixed-step movement then resolves the body's motion.

mod data;
mod events;
mod movement;
mod systems;

use crate::prelude::*;

/// Player components, settings, input, and [`player_systems`].
pub mod prelude {
    pub use super::data::*;
    pub use super::systems::systems as player_systems;
}

/// Registers [`PlayerSettings`], the player observers, movement, and the
/// player input context. Screens run [`player_systems`] themselves.
pub fn plugin(app: &mut App) {
    app.init_resource::<PlayerSettings>()
        .register_type::<PlayerSettings>()
        .add_plugins((events::plugin, movement::plugin))
        .add_input_context::<ICtxDefault>();
}
