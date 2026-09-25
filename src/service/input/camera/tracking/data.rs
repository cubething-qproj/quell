use crate::prelude::*;

/// An unparented, childless camera following a target in world space.
/// Orbit orientation is owned by the camera's `Transform.rotation`.
#[derive(Component, Debug, Reflect)]
pub struct SpringArm {
    pub target: Entity,
    pub target_offset: Vec3,
    /// Desired sphere-center distance; finite and nonnegative.
    pub length: f32,
    /// Positive, finite probe radius. This does not guarantee near-plane clearance.
    pub probe_radius: f32,
}

impl SpringArm {
    pub fn new(target: Entity) -> Self {
        Self {
            target,
            target_offset: Vec3::ZERO,
            length: 10.,
            probe_radius: 0.3,
        }
    }
}
