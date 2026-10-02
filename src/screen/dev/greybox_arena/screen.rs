use bevy::asset::{DependencyLoadState, LoadState, RecursiveDependencyLoadState};
use jackdaw_runtime::{JackdawScene, JackdawSceneRoot};

use crate::prelude::*;

/// The Jackdaw-authored greybox arena.
const SCENE: &str = "scenes/greybox_scene.bsn";

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct GreyboxArenaScreen;
impl Screen for GreyboxArenaScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::Loading, load_arena);
        builder.add_systems(ScreenSchedule::OnReady, init);
        builder.add_systems(ScreenSchedule::Update, player_systems().take());
        builder
    }
    fn name() -> String {
        "greybox_arena".into()
    }
}

/// Wait for the arena scene and its material dependencies to load.
fn load_arena(
    mut screen: ScreenInfoMut<GreyboxArenaScreen>,
    server: Res<AssetServer>,
    mut exit: MessageWriter<AppExit>,
) {
    let scene: Handle<JackdawScene> = server.load(SCENE);
    match server.get_load_states(&scene) {
        Some((
            LoadState::Loaded,
            DependencyLoadState::Loaded,
            RecursiveDependencyLoadState::Loaded,
        )) => {
            screen.finish_loading();
        }
        Some((LoadState::Failed(error), _, _))
        | Some((_, DependencyLoadState::Failed(error), _))
        | Some((_, _, RecursiveDependencyLoadState::Failed(error))) => {
            error!("Failed to load greybox arena {:?}: {error}", scene.path());
            exit.write(AppExit::error());
        }
        _ => {}
    }
}

fn init(mut commands: Commands, server: Res<AssetServer>) {
    debug!("in greybox_arena: init");
    // The level's `PlayerSpawn` spawns the player.
    commands.spawn((JackdawSceneRoot(server.load(SCENE)), ScreenScoped));
    commands.trigger(SpawnGlobalCtx);
    commands.trigger(SpawnCursorCapture);
    // The player camera is the default view; Tab swaps in the flycam.
    commands.spawn(flycam_bundle()).insert((
        Camera {
            order: CameraOrder::World as isize,
            is_active: false,
            ..default()
        },
        ScreenScoped,
        Name::new("Free Camera"),
    ));
    commands.spawn(CameraTestInput::bundle());
}
