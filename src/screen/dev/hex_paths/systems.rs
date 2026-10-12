use super::{
    data::*,
    terrain::{Terrain, world},
};
use crate::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    camera::Viewport,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    mesh::{Indices, PrimitiveTopology},
    window::PrimaryWindow,
};
use bevy_editor_cam::prelude::EditorCam;
use rand::{SeedableRng, rngs::StdRng};

/// Replaces the current board; invalid settings clear it and report an error.
pub(super) fn spawn_board(
    mut commands: Commands,
    settings: Res<HexPathsSettings>,
    tuning: Res<HexTerrainTuning>,
    roots: Query<Entity, With<BoardRoot>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    let seed = settings.seed.unwrap_or_else(rand::random);
    match settings.validate().and(tuning.validate()) {
        Ok(()) => {
            commands.spawn((
                BoardRoot,
                ScreenScoped,
                Name::new("Hex Board"),
                Transform::default(),
            ));
            let terrain = Terrain::generate(&settings, &tuning, &mut StdRng::seed_from_u64(seed));
            commands.insert_resource(Board::new(terrain));
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

/// Rebuilds the terrain surface under the board root, coloured by height,
/// hill-shaded, and tinted by the current view's measure.
pub(super) fn spawn_surface(
    mut commands: Commands,
    board: Option<Res<Board>>,
    tuning: Res<HexTerrainTuning>,
    view: Res<HexPathsView>,
    appearance: Res<HexPathsAppearance>,
    roots: Query<Entity, With<BoardRoot>>,
    surfaces: Query<Entity, With<TerrainSurface>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(board), Ok(root)) = (board, roots.single()) else {
        return;
    };
    for surface in &surfaces {
        commands.entity(surface).despawn();
    }
    commands.spawn((
        TerrainSurface,
        Name::new("Terrain"),
        Mesh3d(meshes.add(surface_mesh(&board.terrain, &tuning, view.view, &appearance))),
        MeshMaterial3d(materials.add(StandardMaterial {
            unlit: true,
            ..default()
        })),
        RenderLayers::layer(31),
        ChildOf(root),
    ));
}

/// Keeps the camera's viewport to the window area beside the settings panel.
pub(super) fn fit_viewport(
    appearance: Res<HexPathsAppearance>,
    window: Single<Ref<Window>, With<PrimaryWindow>>,
    mut camera: Single<&mut Camera, With<HexPathsCamera>>,
) {
    if !window.is_changed() && !appearance.is_changed() && camera.viewport.is_some() {
        return;
    }
    let size = window.physical_size();
    if size.x == 0 || size.y == 0 {
        return;
    }
    let panel = (appearance.panel_width * window.scale_factor()) as u32;
    let panel = panel.min(size.x - 1);
    camera.viewport = Some(Viewport {
        physical_position: UVec2::new(panel, 0),
        physical_size: UVec2::new(size.x - panel, size.y),
        ..default()
    });
}

/// Points the camera at the middle of a new board from the view's starting
/// distance, keeping its current orientation.
pub(super) fn aim_camera(
    board: Res<Board>,
    view: Res<HexPathsView>,
    mut camera: Single<&mut Transform, With<HexPathsCamera>>,
) {
    camera.translation = board.centre - camera.forward() * view.initial_distance;
}

/// Orbits the camera about the board's middle while the right mouse button
/// is held over the board, and zooms towards it with the mouse wheel.
pub(super) fn orbit_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    appearance: Res<HexPathsAppearance>,
    window: Single<&Window, With<PrimaryWindow>>,
    board: Option<Res<Board>>,
    camera: Single<(&Camera, &Transform, &mut EditorCam), With<HexPathsCamera>>,
) {
    let (camera, transform, mut controller) = camera.into_inner();
    if mouse.just_released(MouseButton::Right) {
        controller.end_move();
    }
    let Some(board) = board else {
        return;
    };
    let over_board = window.cursor_position().is_some_and(|cursor| {
        camera
            .logical_viewport_rect()
            .is_some_and(|rect| rect.contains(cursor))
    });
    let anchor = transform
        .compute_affine()
        .inverse()
        .transform_point3(board.centre)
        .as_dvec3();
    let zoom = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y * appearance.zoom_per_line,
        MouseScrollUnit::Pixel => scroll.delta.y,
    };
    if over_board && mouse.just_pressed(MouseButton::Right) {
        controller.start_orbit(Some(anchor));
    } else if over_board && zoom != 0. && !controller.current_motion.is_user_controlled() {
        controller.start_zoom(Some(anchor));
    }
    if mouse.pressed(MouseButton::Right) {
        controller.send_screenspace_input(motion.delta);
    }
    controller.send_zoom_input(zoom);
    // A wheel zoom ends once its smoothed input settles.
    let settled = controller.current_motion.inputs().is_none_or(|inputs| {
        inputs.zoom_velocity_abs(controller.smoothing.zoom.mul_f32(2.)) == 0.
    });
    if controller.current_motion.is_zooming_only() && settled {
        controller.end_move();
    }
}

/// Outlines the playable tiles on the terrain.
pub(super) fn draw_board(
    board: Option<Res<Board>>,
    appearance: Res<HexPathsAppearance>,
    mut gizmos: Gizmos<HexPathsGizmos>,
) {
    let Some(board) = board else {
        return;
    };
    let terrain = &board.terrain;
    for (corners, _) in terrain
        .mesh
        .tile_corners
        .iter()
        .zip(&terrain.mesh.playable)
        .filter(|(_, playable)| **playable)
    {
        let point = |c: usize| {
            world(terrain.mesh.positions[c], terrain.heights[c] + appearance.grid_lift)
        };
        gizmos.linestrip(
            corners.iter().chain([&corners[0]]).map(|&c| point(c)),
            appearance.grid,
        );
    }
}

/// The terrain as triangles around each tile centre. Colours follow the
/// height ramp, shaded by slope, tinted by `view`, with the margin faded
/// towards paper.
fn surface_mesh(
    terrain: &Terrain,
    tuning: &HexTerrainTuning,
    view: TerrainView,
    appearance: &HexPathsAppearance,
) -> Mesh {
    let mesh = &terrain.mesh;
    let heights = terrain.point_heights();
    let positions: Vec<Vec3> = mesh
        .positions
        .iter()
        .chain(&mesh.centers)
        .zip(&heights)
        .map(|(&p, &h)| world(p, h))
        .collect();
    // Wind every triangle to face up.
    let triangles: Vec<[usize; 3]> = mesh
        .triangles()
        .map(|[a, b, c]| {
            let up = (positions[b] - positions[a])
                .cross(positions[c] - positions[a])
                .y;
            if up >= 0. { [a, b, c] } else { [a, c, b] }
        })
        .collect();
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for &[a, b, c] in &triangles {
        // Unnormalised, so larger triangles weigh more.
        let normal = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
        [a, b, c].iter().for_each(|&i| normals[i] += normal);
    }
    let normals: Vec<Vec3> = normals.into_iter().map(Vec3::normalize_or_zero).collect();
    let inside: Vec<bool> = mesh
        .playable_corners()
        .into_iter()
        .chain(mesh.playable.iter().copied())
        .collect();
    let light = appearance.light.normalize();
    let paper = appearance.paper.to_linear();
    let (tint, amounts) = view_tint(terrain, tuning, view, appearance);
    let colours: Vec<[f32; 4]> = heights
        .iter()
        .zip(&normals)
        .zip(&inside)
        .zip(&amounts)
        .map(|(((&h, &n), &inside), &amount)| {
            let shade =
                appearance.shade_floor + appearance.shade_gain * n.dot(light).max(0.) / light.y;
            let lit = (appearance.ramp(h) * shade).mix(&tint, amount);
            let veil = if inside { 0. } else { appearance.margin_veil };
            lit.mix(&paper, veil).with_alpha(1.).to_f32_array()
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    .with_inserted_indices(Indices::U32(
        triangles.into_iter().flatten().map(|i| i as u32).collect(),
    ))
}

/// The colour `view` tints the terrain with, and how strongly at each mesh
/// point (corners, then centres), 0–1.
fn view_tint(
    terrain: &Terrain,
    tuning: &HexTerrainTuning,
    view: TerrainView,
    appearance: &HexPathsAppearance,
) -> (LinearRgba, Vec<f32>) {
    let mesh = &terrain.mesh;
    let corners = match view {
        TerrainView::Heightmap => vec![0.; mesh.positions.len()],
        TerrainView::Cost => {
            // Shown as `aversion × |rise|`, the log of the step cost, scaled
            // so the top few percent of playable edges are full colour.
            let steepness: Vec<f32> = terrain
                .step_costs(tuning.aversion)
                .into_iter()
                .map(f32::ln)
                .collect();
            let mut playable: Vec<f32> =
                steepness.iter().copied().filter(|s| s.is_finite()).collect();
            playable.sort_by(f32::total_cmp);
            let top = playable
                .get(((playable.len() as f32 - 1.) * appearance.cost_percentile) as usize)
                .copied()
                .unwrap_or(1.)
                .max(f32::EPSILON);
            let mut corners = vec![0f32; mesh.positions.len()];
            for (&[a, b], &s) in mesh.edges.iter().zip(&steepness) {
                if s.is_finite() {
                    let amount = (s / top).min(1.);
                    corners[a] = corners[a].max(amount);
                    corners[b] = corners[b].max(amount);
                }
            }
            corners
        }
    };
    let centres = mesh
        .tile_corners
        .iter()
        .map(|tile| tile.iter().map(|&c| corners[c]).sum::<f32>() / 6.);
    let amounts = corners.iter().copied().chain(centres).collect();
    (appearance.cost.to_linear(), amounts)
}
