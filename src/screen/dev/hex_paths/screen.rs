use super::{data::*, panel, systems};
use crate::prelude::*;
use bevy::feathers::{dark_theme::create_dark_theme, theme::UiTheme};
use bevy_editor_cam::prelude::{EditorCam, EnabledMotion};

/// Dev screen for inspecting hex path generation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct HexPathsScreen;

impl Screen for HexPathsScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::Loading, init);
        builder.add_systems(ScreenSchedule::OnReady, request_board);
        builder.add_systems(
            ScreenSchedule::Update,
            (
                systems::spawn_board.run_if(
                    resource_changed::<HexPathsSettings>.or_else(resource_changed::<HexTerrainTuning>),
                ),
                systems::spawn_surface.run_if(
                    resource_exists_and_changed::<Board>
                        .or_else(resource_changed::<HexPathsView>)
                        .or_else(resource_changed::<HexPathsAppearance>),
                ),
                systems::fit_viewport,
                systems::aim_camera.run_if(resource_exists_and_changed::<Board>),
                systems::orbit_camera,
                systems::draw_board,
                panel::sync_settings,
                panel::update_stats,
                panel::resize_panel,
            )
                .chain(),
        );
        builder.add_systems(ScreenSchedule::OnUnload, unload);
        builder
    }

    fn name() -> String {
        "hex_paths".into()
    }
}

/// Sets the dev theme and creates the board camera and panel once when
/// entering the screen.
fn init(
    mut commands: Commands,
    appearance: Res<HexPathsAppearance>,
    view: Res<HexPathsView>,
    mut ui_camera: Single<(Entity, &mut Camera), With<IsDefaultUiCamera>>,
    mut screen: ScreenInfoMut<HexPathsScreen>,
) {
    commands.insert_resource(UiTheme(create_dark_theme()));
    commands.insert_resource(UiCameraState {
        entity: ui_camera.0,
        clear_color: ui_camera.1.clear_color,
    });
    ui_camera.1.clear_color = ClearColorConfig::None;
    commands.spawn((
        Name::new("Hex Board Camera"),
        ScreenScoped,
        HexPathsCamera,
        Camera3d::default(),
        // Panning will follow the player or terrain instead.
        EditorCam {
            enabled_motion: EnabledMotion {
                pan: false,
                orbit: true,
                zoom: true,
            },
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(-view.initial_tilt.to_radians())),
        Camera {
            order: CameraOrder::World as isize,
            ..default()
        },
        RenderLayers::layer(31),
    ));
    commands.queue_spawn_scene(panel::scene(appearance.panel_width));
    screen.finish_loading();
}

/// Requests a board on each screen entry; Update is the sole generation path.
fn request_board(mut settings: ResMut<HexPathsSettings>) {
    settings.set_changed();
}

/// Removes play-time resources when the screen's entities are cleaned up.
pub(super) fn unload(
    mut commands: Commands,
    mut panel: ResMut<PanelState>,
    ui_state: Option<Res<UiCameraState>>,
    mut cameras: Query<&mut Camera>,
) {
    if let Some(state) = ui_state
        && let Ok(mut camera) = cameras.get_mut(state.entity)
    {
        camera.clear_color = state.clear_color;
    }
    commands.remove_resource::<UiCameraState>();
    commands.remove_resource::<Board>();
    commands.remove_resource::<BoardStats>();
    panel.seed_error = None;
}
