use bevy::asset::{DependencyLoadState, LoadState, RecursiveDependencyLoadState};

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

fn init(mut commands: Commands) {
    debug!("in world: init");
    commands.trigger(SpawnPlayerRoot);
    commands.trigger(SpawnWorldgenRoot);
}

pub fn plugin(app: &mut App) {
    app.init_collection::<PlayerAssets>();
    app.register_screen::<WorldScreen>();
}
