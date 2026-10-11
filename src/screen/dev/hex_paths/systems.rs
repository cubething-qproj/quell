use super::{data::*, panel, screen::HexPathsScreen};
use crate::prelude::*;
use bevy::{
    camera::{ScalingMode, Viewport},
    window::PrimaryWindow,
};

/// Creates the board camera and panel once when entering the screen.
pub(super) fn init(
    mut commands: Commands,
    appearance: Res<HexPathsAppearance>,
    mut ui_camera: Single<(Entity, &mut Camera), With<IsDefaultUiCamera>>,
    mut screen: ScreenInfoMut<HexPathsScreen>,
) {
    commands.insert_resource(UiCameraState {
        entity: ui_camera.0,
        clear_color: ui_camera.1.clear_color,
    });
    ui_camera.1.clear_color = ClearColorConfig::None;
    commands.spawn((
        Name::new("Hex Board Camera"),
        ScreenScoped,
        HexPathsCamera,
        Camera2d,
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
pub(super) fn request_board(mut settings: ResMut<HexPathsSettings>) {
    settings.set_changed();
}

/// Replaces the current board; invalid settings clear it and report an error.
pub(super) fn spawn_board(
    mut commands: Commands,
    settings: Res<HexPathsSettings>,
    roots: Query<Entity, With<BoardRoot>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    let seed = settings.seed.unwrap_or_else(rand::random);
    match settings.validate() {
        Ok(()) => {
            commands.spawn((
                BoardRoot,
                ScreenScoped,
                Name::new("Hex Board"),
                Transform::default(),
            ));
            commands.insert_resource(Board::new(&settings));
            commands.insert_resource(BoardStats { seed, error: None });
        }
        Err(error) => {
            commands.remove_resource::<Board>();
            commands.insert_resource(BoardStats {
                seed,
                error: Some(error),
            });
        }
    }
}

/// Fits the complete board into the window area beside the settings panel.
pub(super) fn fit_camera(
    board: Option<Res<Board>>,
    appearance: Res<HexPathsAppearance>,
    window: Single<Ref<Window>, With<PrimaryWindow>>,
    mut camera: Single<(&mut Camera, &mut Projection), With<HexPathsCamera>>,
) {
    let Some(board) = board else {
        return;
    };
    if !board.is_changed() && !appearance.is_changed() && !window.is_changed() {
        return;
    }
    let size = window.physical_size();
    if size.x == 0 || size.y == 0 {
        return;
    }
    let panel = (appearance.panel_width * window.scale_factor()) as u32;
    let panel = panel.min(size.x - 1);
    camera.0.viewport = Some(Viewport {
        physical_position: UVec2::new(panel, 0),
        physical_size: UVec2::new(size.x - panel, size.y),
        ..default()
    });
    if let Projection::Orthographic(projection) = &mut *camera.1 {
        let size = board.size + Vec2::splat(appearance.board_padding * 2.);
        projection.scaling_mode = ScalingMode::AutoMin {
            min_width: size.x,
            min_height: size.y,
        };
    }
}

/// Draws the grid outline.
pub(super) fn draw_board(
    board: Option<Res<Board>>,
    appearance: Res<HexPathsAppearance>,
    mut gizmos: Gizmos<HexPathsGizmos>,
) {
    let Some(board) = board else {
        return;
    };
    for hex in board.cells() {
        gizmos.lineloop_2d(board.layout.hex_corners(hex), appearance.grid);
    }
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
