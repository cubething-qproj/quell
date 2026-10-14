//! The UI camera.

use crate::prelude::*;
use bevy::prelude::*;

/// Marks the persistent camera that renders UI above the world.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub(crate) struct UiCamera;

/// Spawns the UI camera once at startup. It is [`Persistent`], so it survives
/// screen changes.
fn spawn_ui_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("UI Camera"),
        Persistent,
        UiCamera,
        Camera2d,
        IsDefaultUiCamera,
        Camera {
            order: CameraOrder::Ui as isize,
            ..default()
        },
        RenderLayers::from(RenderLayer::DEFAULT),
    ));
}

/// Spawns the UI camera at startup.
pub fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_ui_camera);
    app.register_type::<UiCamera>();
}
