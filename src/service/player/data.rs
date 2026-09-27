use crate::prelude::*;
use std::f32::consts::FRAC_PI_4;

#[derive(Resource, Reflect, Debug)]
#[reflect(Resource)]
pub struct PlayerSettings {
    /// Length of the capsule's cylindrical segment, excluding the rounded caps;
    /// total height is `capsule_height + 2 * capsule_radius`.
    /// Applied when spawning the player, not to an existing collider.
    pub capsule_height: f32,
    /// Applied when spawning the player, not to an existing collider.
    pub capsule_radius: f32,
    pub default_speed: f32,
    /// Steepest walkable surface, in radians from horizontal.
    pub max_slope: f32,
    /// Distance of the downward ground probe and snap after each move.
    pub ground_snap: f32,
    /// Facing slew rate, in rad/s.
    pub turn_speed: f32,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            capsule_height: 3.,
            capsule_radius: 0.5,
            default_speed: 10.,
            max_slope: FRAC_PI_4,
            ground_snap: 0.2,
            turn_speed: 10.,
        }
    }
}

impl PlayerSettings {
    /// Height of the capsule's center when its base rests at y = 0.
    pub fn resting_height(&self) -> f32 {
        self.capsule_height / 2. + self.capsule_radius
    }
}

#[derive(SystemSet, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlayerSystems;

#[derive(Event, Reflect, Copy, Clone, Debug)]
pub struct SpawnPlayerRoot;

#[derive(Component, Default)]
#[require(Name::new("Player Controller"))]
pub struct PlayerController {
    pub last_move: Option<Vec3>,
}

/// Default player input context
#[derive(Component)]
pub struct ICtxDefault;

/// PlayerAction_Move
#[derive(InputAction, Reflect)]
#[action_output(Vec3)]
pub struct PAMove;

#[derive(AssetCollection, Resource, Default, Debug)]
pub struct PlayerAssets {
    #[asset(path = "models/basil.glb#Scene0")]
    pub model: Handle<WorldAsset>,
}

/// Movement intent consumed by the fixed-step player controller.
#[derive(Component, Default)]
#[require(
    RigidBody::Kinematic,
    // Only move-and-slide moves the body.
    CustomPositionIntegration,
    // No speculative contact impulses from the kinematic body.
    SpeculativeMargin::ZERO,
    TransformInterpolation
)]
pub struct PlayerMotor {
    /// World-space horizontal velocity to walk at, in m/s.
    pub desired_velocity: Vec3,
    /// Facing to turn toward; `None` keeps the current facing.
    pub desired_forward: Option<Dir3>,
}
