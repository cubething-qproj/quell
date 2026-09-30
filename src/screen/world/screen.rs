use bevy::asset::{DependencyLoadState, LoadState, RecursiveDependencyLoadState};
use jackdaw_runtime::JackdawSceneRoot;

use crate::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct WorldScreen;
impl Screen for WorldScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::Loading, load_world);
        builder.add_systems(ScreenSchedule::OnReady, init);
        builder.add_systems(ScreenSchedule::Update, player_systems().take());
        builder
    }
}

fn load_world(
    mut screen: ScreenInfoMut<WorldScreen>,
    server: Res<AssetServer>,
    player: Res<PlayerAssets>,
    mut exit: MessageWriter<AppExit>,
) {
    match server.get_load_states(&player.model) {
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
            error!(
                "Failed to load player model {:?}: {error}",
                player.model.path()
            );
            exit.write(AppExit::error());
        }
        _ => {}
    }
}

fn init(mut commands: Commands, server: Res<AssetServer>) {
    debug!("in world: init");
    // The level's `PlayerSpawn` spawns the player.
    commands.spawn((
        JackdawSceneRoot(server.load("scenes/scene.bsn")),
        ScreenScoped,
    ));
    commands.trigger(SpawnWorldgenRoot);
    commands.trigger(SpawnGlobalCtx);
    commands.trigger(SpawnCursorCapture);
    #[cfg(feature = "dev")]
    {
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
}

pub fn plugin(app: &mut App) {
    app.init_collection::<PlayerAssets>();
    app.register_screen::<WorldScreen>();
}
