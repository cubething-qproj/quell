//! Player components, settings, input actions, and assets.

use crate::prelude::*;
use jackdaw_runtime::EditorPreview;
use std::f32::consts::FRAC_PI_4;

/// Body dimensions and movement tuning used by [`PlayerController`] setup and
/// the fixed-step motor. Field docs distinguish spawn-time values from live settings.
#[derive(Resource, Reflect, Debug)]
#[reflect(Resource)]
pub struct PlayerSettings {
    /// Length of the capsule's cylindrical segment, excluding the rounded caps;
    /// total height is `capsule_height + 2 * capsule_radius`.
    /// Applied when spawning the player, not to an existing collider.
    pub capsule_height: f32,
    /// Radius of the capsule and its rounded caps.
    /// Applied when spawning the player, not to an existing collider.
    pub capsule_radius: f32,
    /// Walking speed, in m/s. Applies to an existing player.
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

    /// Whether a surface with outward `normal` is shallow enough to walk on.
    pub fn walkable(&self, normal: Vec3) -> bool {
        normal.angle_between(Vec3::Y) <= self.max_slope
    }
}

/// The fixed-step player movement systems.
#[derive(SystemSet, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlayerSystems;

/// Spawns the player with its base resting at `transform`.
#[derive(Event, Reflect, Copy, Clone, Debug, Default)]
pub struct SpawnPlayerRoot {
    /// Where the base of the player's capsule rests.
    pub transform: Transform,
}

/// Level-authored point where the player spawns, placed on the floor.
#[derive(Component, Reflect, Default)]
// Keep the preview in sync with `PlayerAssets::model`.
#[reflect(Component, Default, @EditorPreview::gltf("models/basil.glb"))]
pub struct PlayerSpawn;

/// Connects player input and presentation to [`PlayerMotor`] movement.
///
/// Add after [`PlayerAssets`] and [`PlayerSettings`] are available. Its setup
/// hook installs the model, collider, bindings, and a camera linked through
/// [`PlayerCameraOf`]. Required components supply the motor and input context.
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
#[require(
    Name::new("Player Controller"),
    PlayerMotor,
    CollisionLayers = CollisionLayers::new(CollisionLayer::Player, LayerMask::ALL),
    ICtxDefault,
    ContextActivity<ICtxDefault> = ContextActivity::ACTIVE,
)]
#[component(on_add = super::events::on_add_player_controller)]
pub struct PlayerController {
    /// Smoothed camera-local input scaled to speed. [`player_systems`] consumes
    /// it and rotates it into world-space motor intent; `None` means no input.
    pub last_move: Option<Vec3>,
    /// Keeps facing fixed after raw input stops, while smoothed motion settles.
    pub rotation_locked: bool,
}

/// Links a camera entity to its player and the player's lifetime.
/// [`PlayerCameras`] records the inverse relationship.
#[derive(Component)]
#[relationship(relationship_target = PlayerCameras)]
pub struct PlayerCameraOf(pub Entity);

/// Camera entities linked by [`PlayerCameraOf`], removed when the player despawns.
#[derive(Component)]
#[relationship_target(relationship = PlayerCameraOf, linked_spawn)]
pub struct PlayerCameras(Vec<Entity>);

/// Default player input context.
#[derive(Component, Default)]
pub struct ICtxDefault;

/// Smoothed movement input.
#[derive(InputAction, Reflect)]
#[action_output(Vec3)]
pub struct PAMove;

/// Unsmoothed movement input used to unlock facing while held and lock it on completion.
/// This separates facing changes from the residual velocity in [`PAMove`].
#[derive(InputAction, Reflect)]
#[action_output(Vec3)]
pub struct PARawMoveInput;

/// Assets the player needs before it can spawn.
#[derive(AssetCollection, Resource, Default, Debug)]
pub struct PlayerAssets {
    // Keep `PlayerSpawn`'s editor preview in sync with this path.
    /// The player model.
    #[asset(path = "models/basil.glb#Scene0")]
    pub model: Handle<WorldAsset>,
}

/// Movement intent consumed by the fixed-step player controller.
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
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
impl PlayerMotor {
    /// A motor with no movement intent.
    pub fn new() -> Self {
        Self::default()
    }
    /// Sets the desired world-space velocity and returns the motor.
    pub fn with_vel(self, desired_velocity: Vec3) -> Self {
        Self {
            desired_velocity,
            ..self
        }
    }
    /// Sets the desired facing and returns the motor.
    pub fn with_forward(self, desired_forward: Option<Dir3>) -> Self {
        Self {
            desired_forward,
            ..self
        }
    }
}
