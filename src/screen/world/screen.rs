use crate::prelude::*;

#[derive(AssetCollection, Resource, Default, Debug)]
pub struct WorldAssets {
    _player_assets: PlayerAssets,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct WorldScreen;
impl Screen for WorldScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::Loading, init);
        builder.add_systems(
            ScreenSchedule::Update,
            (player_systems().take(), tracking_cam_systems().take()),
        );
        builder
    }
}

fn init(mut commands: Commands) {
    debug!("in world: init");
    commands.trigger(SpawnPlayerRoot);
    commands.trigger(SpawnWorldgenRoot);
}

pub fn plugin(app: &mut App) {
    app.register_screen_loading_state::<WorldScreen>();
    app.add_loading_state(
        LoadingState::new(ScreenLoadingState::loading::<WorldScreen>())
            .continue_to_state(ScreenLoadingState::ready::<WorldScreen>())
            .load_collection::<WorldAssets>(),
    );
}
