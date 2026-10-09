use super::{data::*, generate::BoardLayout, panel, screen::HexPathsScreen};
use crate::prelude::*;
use bevy::{
    camera::{ScalingMode, Viewport},
    window::PrimaryWindow,
};
use hexx::EdgeDirection;
use rand::{SeedableRng, rngs::StdRng};

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

/// Replaces the current board; failed generation clears it and reports an error.
pub(super) fn spawn_board(
    mut commands: Commands,
    settings: Res<HexPathsSettings>,
    roots: Query<Entity, With<BoardRoot>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    let seed = settings.seed.unwrap_or_else(rand::random);
    match BoardLayout::generate(&settings, &mut StdRng::seed_from_u64(seed)) {
        Ok(layout) => {
            let stats = BoardStats::from_layout(seed, &layout);
            let mut board = Board::new(&settings);
            let root = commands
                .spawn((
                    BoardRoot,
                    ScreenScoped,
                    Name::new("Hex Board"),
                    Transform::default(),
                ))
                .id();
            for (hex, tile) in layout.tiles {
                let mut entity =
                    commands.spawn((HexPos(hex), tile.kind, tile.spokes, ChildOf(root)));
                if hex == layout.spawn {
                    entity.insert(Revealed);
                }
                board.tiles.insert(hex, entity.id());
            }
            commands.spawn((
                HexPathsPlayer,
                HexPos(layout.spawn),
                Transform::from_translation(board.layout.hex_to_world_pos(layout.spawn).extend(0.)),
                ChildOf(root),
            ));
            commands.insert_resource(board);
            commands.insert_resource(stats);
        }
        Err(error) => {
            commands.remove_resource::<Board>();
            commands.insert_resource(BoardStats {
                seed,
                error: Some(error),
                ..default()
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

/// Draws outlines, known spokes, node markers, and the player token.
pub(super) fn draw_board(
    board: Option<Res<Board>>,
    view: Res<HexPathsView>,
    appearance: Res<HexPathsAppearance>,
    tiles: Query<(&HexPos, &TileKind, &Spokes, Has<Revealed>)>,
    player: Query<&Transform, With<HexPathsPlayer>>,
    mut gizmos: Gizmos<HexPathsGizmos>,
) {
    let Some(board) = board else {
        return;
    };
    for hex in board.cells() {
        gizmos.lineloop_2d(board.layout.hex_corners(hex), appearance.grid);
    }
    for (position, kind, spokes, revealed) in &tiles {
        let hex = position.0;
        let center = board.layout.hex_to_world_pos(hex);
        if revealed || view.show_paths {
            for dir in EdgeDirection::ALL_DIRECTIONS
                .into_iter()
                .filter(|&dir| spokes.has(dir))
            {
                let end = (center + board.layout.hex_to_world_pos(hex.neighbor(dir))) / 2.;
                gizmos.line_2d(center, end, appearance.paths);
                let paired = board
                    .tiles
                    .get(&hex.neighbor(dir))
                    .and_then(|&entity| tiles.get(entity).ok())
                    .is_some_and(|(_, _, other, _)| spokes.crosses(*other, dir));
                if !paired {
                    let size = appearance.dead_end_size;
                    gizmos.line_2d(
                        end - Vec2::splat(size),
                        end + Vec2::splat(size),
                        appearance.paths,
                    );
                    gizmos.line_2d(
                        end + Vec2::new(-size, size),
                        end + Vec2::new(size, -size),
                        appearance.paths,
                    );
                }
            }
        }
        match kind {
            TileKind::Spawn => {
                gizmos.rect_2d(
                    center,
                    Vec2::splat(appearance.marker_radius * 2.),
                    appearance.spawn,
                );
            }
            TileKind::Node(card) => {
                let color = if revealed || view.show_node_traits {
                    appearance.node_color(*card)
                } else {
                    appearance.hidden_node
                };
                gizmos.circle_2d(center, appearance.marker_radius, color);
            }
            TileKind::Path => {}
        }
    }
    for transform in &player {
        gizmos.circle_2d(
            transform.translation.truncate(),
            appearance.marker_radius / 2.,
            appearance.player,
        );
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
