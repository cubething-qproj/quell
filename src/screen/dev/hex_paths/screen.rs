use super::{panel, systems};
use crate::prelude::*;

/// Dev screen for inspecting hex path generation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct HexPathsScreen;

impl Screen for HexPathsScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::Loading, systems::init);
        builder.add_systems(ScreenSchedule::OnReady, systems::request_board);
        builder.add_systems(
            ScreenSchedule::Update,
            (
                systems::spawn_board.run_if(resource_changed::<super::data::HexPathsSettings>),
                systems::fit_camera,
                systems::draw_board,
                panel::sync_settings,
                panel::sync_view,
                panel::update_stats,
                panel::resize_panel,
            )
                .chain(),
        );
        builder.add_systems(ScreenSchedule::OnUnload, systems::unload);
        builder
    }

    fn name() -> String {
        "hex_paths".into()
    }
}
