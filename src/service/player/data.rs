use crate::prelude::*;

#[derive(Resource, Reflect, Debug)]
#[reflect(Resource)]
pub struct PlayerSettings {
    /// Applied when spawning the player, not to an existing collider.
    pub capsule_height: f32,
    /// Applied when spawning the player and its movement sensor.
    pub capsule_radius: f32,
    pub default_speed: f32,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            capsule_height: 3.,
            capsule_radius: 0.5,
            default_speed: 10.,
        }
    }
}

#[derive(Resource, Reflect, Debug)]
#[reflect(Resource)]
pub struct PlayerCameraSettings {
    /// Currently unused; orbit uses the mouse/stick binding sensitivities.
    pub rotation_speed: f32,
    pub zoom_speed: f32,
}

impl Default for PlayerCameraSettings {
    fn default() -> Self {
        Self {
            rotation_speed: 10.,
            zoom_speed: -5.,
        }
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

#[derive(TnuaScheme)]
#[scheme(basis = TnuaBuiltinWalk)]
pub enum PlayerControlScheme {}

pub type PlayerTnuaController = TnuaController<PlayerControlScheme>;
