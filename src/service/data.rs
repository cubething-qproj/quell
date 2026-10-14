//! Data shared across services.

use avian3d::prelude::*;
use std::ops::BitOr;

/// Physics collision layers. `|` combines layers into a bitmask.
#[derive(PhysicsLayer, Copy, Clone, Debug, Default)]
pub enum CollisionLayer {
    /// Level geometry and anything without a more specific layer.
    #[default]
    Default,
    /// The player's body.
    Player,
    /// Camera collision probes.
    Camera,
}
impl BitOr for CollisionLayer {
    type Output = u32;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.to_bits() | rhs.to_bits()
    }
}
