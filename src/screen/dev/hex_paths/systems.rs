//! Bevy integration for pure terrain and layout generation, then preview rendering.
//!
//! [`spawn_board`] validates settings, obtains a cached or newly simulated
//! [`Region`], chooses [`Terrain`], builds [`Layout`], and installs [`Board`].
//! [`relayout`] keeps that terrain and attempts a new arrangement; failure
//! retains the old layout. Both repair tile-based camera focus after success.
//!
//! [`spawn_surface`] draws levelled layout heights, while [`spawn_markers`] and
//! [`paint_markers`] display landmark roles. Picking edits [`HexPathsView`];
//! camera systems resolve [`CameraFocus`] into an exaggerated world target.
//! [`draw_board`] overlays true topology as gizmos. [`screen`](super::screen) orders
//! these systems so generation precedes rendering and focus precedes aiming.
//! Visual height scaling never enters terrain costs, sight tests, or pace.

use super::{
    data::*,
    layout::{Card, Context, Layout, NodeKind, regeneration_seed},
    terrain::{
        Terrain,
        erosion::{Region, RegionKey},
        point_heights, world,
    },
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

/// Replaces the board after generation settings change, or on screen entry.
///
/// Validates settings, reuses [`RegionCache`] for a matching [`RegionKey`],
/// then generates [`Terrain`] and [`Layout`] into a measured [`Board`]. Window
/// selection has its own seeded RNG stream so cache hits do not change it.
/// Success creates a [`BoardRoot`] and repairs retained camera focus. Failure
/// removes the old board and hierarchy and records the attempted seed and
/// error in [`BoardStats`]; layout-only failures behave differently in [`relayout`].
pub(super) fn spawn_board(
    mut commands: Commands,
    settings: Res<HexPathsSettings>,
    tuning: Res<HexTerrainTuning>,
    mut cache: ResMut<RegionCache>,
    mut panel: ResMut<PanelState>,
    mut view: ResMut<HexPathsView>,
    roots: Query<Entity, With<BoardRoot>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    panel.regeneration_error = None;
    let seed = settings.seed.unwrap_or_else(rand::random);
    let board = settings.validate().and(tuning.validate()).and_then(|()| {
        let key = RegionKey::new(seed, &settings, &tuning);
        if cache.0.as_ref().is_none_or(|(cached, _)| *cached != key) {
            let region = Region::simulate(&key, &mut StdRng::seed_from_u64(seed));
            cache.0 = Some((key, region));
        }
        let (_, region) = cache.0.as_ref().expect("just simulated");
        // The window draws from its own stream, so a cached region gives the
        // same board as a fresh one.
        let mut rng = StdRng::seed_from_u64(seed.wrapping_add(1));
        let terrain = Terrain::generate(&settings, &tuning, region, &mut rng)?;
        let context = Context {
            terrain: &terrain,
            settings: &settings,
            tuning: &tuning,
        };
        let layout = Layout::generate(context, seed, None)?;
        Ok(Board::new(terrain, layout, (&settings, &tuning), seed, 0))
    });
    match board {
        Ok(board) => {
            commands.spawn((
                BoardRoot,
                ScreenScoped,
                Name::new("Hex Board"),
                Transform::default(),
                Visibility::default(),
            ));
            let focus = board.keep_focus(view.focus);
            if view.focus != focus {
                view.focus = focus;
            }
            commands.insert_resource(board);
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

/// Attempts a new [`Layout`] without changing the [`Board`]'s terrain.
///
/// `None` generates a fresh unpinned layout and deals its deck; `Some(false)`
/// partially regenerates around focus, and `Some(true)` applies Fear there.
/// Node focus selects that tile as the player position; otherwise spawn is used.
/// Success advances the layout count, measures the replacement, and repairs
/// focus and marker semantics. Failure reports through [`PanelState`] while
/// retaining the previous layout and count.
pub(super) fn relayout(
    regeneration: Option<bool>,
    settings: &HexPathsSettings,
    tuning: &HexTerrainTuning,
    view: &mut HexPathsView,
    board: &mut Board,
    panel: &mut PanelState,
) {
    let count = board.count + 1;
    let seed = regeneration_seed(board.seed, count);
    let context = Context {
        terrain: &board.terrain,
        settings,
        tuning,
    };
    let layout = match regeneration {
        None => Layout::generate(context, seed, None),
        Some(fear) => {
            let player = match board.keep_focus(view.focus) {
                CameraFocus::Node(tile) => tile,
                CameraFocus::Player | CameraFocus::Terrain => board.layout.spawn,
            };
            board.layout.regenerate(context, seed, player, fear)
        }
    };
    match layout {
        Ok(layout) => {
            board.replace_layout(layout, count, (settings, tuning));
            board.regenerated = regeneration.is_some();
            panel.regeneration_error = None;
            // After a regeneration the player stands on spawn; a fresh
            // layout may have moved the focused node.
            let focus = match regeneration {
                Some(_) => CameraFocus::Player,
                None => board.keep_focus(view.focus),
            };
            if view.focus != focus {
                view.focus = focus;
            }
        }
        Err(error) => panel.regeneration_error = Some(error),
    }
}

/// Rebuilds the [`TerrainSurface`] child beneath the current [`BoardRoot`].
///
/// [`surface_mesh`] uses [`Layout::heights`] for levelled ground, applies
/// appearance exaggeration, and colours it with height, hillshade, and the
/// selected [`TerrainView`]. Scheduled when board, view, or appearance changes;
/// does nothing if the board or its single root is absent.
pub(super) fn spawn_surface(
    mut commands: Commands,
    board: Option<Res<Board>>,
    settings: Res<HexPathsSettings>,
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
        Mesh3d(meshes.add(surface_mesh(
            &board,
            &settings,
            &tuning,
            view.view,
            &appearance,
        ))),
        MeshMaterial3d(materials.add(StandardMaterial {
            unlit: true,
            ..default()
        })),
        RenderLayers::layer(31),
        ChildOf(root),
    ));
}

/// Replaces billboard markers for spawn/player, POIs, and benches on the board.
///
/// Positions each [`Pad`] at its levelled centre using [`Board::pad`]. The
/// spawn/player distinction follows [`Board::regenerated`], rather than the
/// layout count. Materials start unknown and [`paint_markers`] selects their
/// displayed roles. Crossing pads are outlines drawn by [`draw_board`].
pub(super) fn spawn_markers(
    mut commands: Commands,
    board: Res<Board>,
    billboards: Res<Billboards>,
    appearance: Res<HexPathsAppearance>,
    roots: Query<Entity, With<BoardRoot>>,
    markers: Query<Entity, With<Marker>>,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    for marker in &markers {
        commands.entity(marker).despawn();
    }
    let layout = &board.layout;
    let start = if board.regenerated { Marker::Player } else { Marker::Spawn };
    let nodes = layout.nodes.iter().filter_map(|node| match node.kind {
        NodeKind::Poi => Some((node.tile, Marker::Poi(node.card))),
        NodeKind::Bench => Some((node.tile, Marker::Bench)),
        NodeKind::Crossing => None,
    });
    for (tile, marker) in std::iter::once((layout.spawn, start)).chain(nodes) {
        commands.spawn((
            Name::new("Marker"),
            marker,
            Pad(tile),
            Billboard,
            Mesh3d(billboards.quad.clone()),
            MeshMaterial3d(billboards.unknown.clone()),
            Transform::from_translation(board.pad(tile, appearance.vertical_scale)),
            RenderLayers::layer(31),
            ChildOf(root),
        ));
    }
}

/// Selects [`Billboards`] materials from each [`Marker`] and the reveal toggle.
///
/// Spawn and Player always show their own image; POIs and benches stay unknown
/// unless [`HexPathsView::reveal_nodes`] is true. Updates newly added markers
/// or markers after a view change. This is a global preview reveal, not
/// arrival-based discovery.
pub(super) fn paint_markers(
    view: Res<HexPathsView>,
    billboards: Res<Billboards>,
    mut markers: Query<(Ref<Marker>, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    for (marker, mut material) in &mut markers {
        if !view.is_changed() && !marker.is_added() {
            continue;
        }
        let handle = match *marker {
            Marker::Spawn => &billboards.spawn,
            Marker::Player => &billboards.player,
            _ if !view.reveal_nodes => &billboards.unknown,
            Marker::Bench => &billboards.bench,
            Marker::Poi(None) => &billboards.empty,
            Marker::Poi(Some(Card::Key)) => &billboards.key,
            Marker::Poi(Some(Card::Npc)) => &billboards.npc,
            Marker::Poi(Some(Card::Fear)) => &billboards.fear,
        };
        if material.0 != *handle {
            material.0 = handle.clone();
        }
    }
}

/// Chooses the nearest marker hit by a left-click ray within the board viewport.
///
/// Approximates picking by distance to each billboard's middle, using half
/// its configured height as the radius. Spawn/player markers select Player
/// focus; POIs and benches select their tile in [`CameraFocus::Node`]. This
/// sets camera focus only, not a walking destination.
pub(super) fn pick_focus(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    appearance: Res<HexPathsAppearance>,
    camera: Single<(&Camera, &GlobalTransform), With<HexPathsCamera>>,
    markers: Query<(&Marker, &Pad, &GlobalTransform)>,
    mut view: ResMut<HexPathsView>,
) {
    let (camera, camera_transform) = *camera;
    let Some(ray) = window
        .cursor_position()
        .filter(|_| mouse.just_pressed(MouseButton::Left))
        .filter(|&cursor| {
            camera
                .logical_viewport_rect()
                .is_some_and(|rect| rect.contains(cursor))
        })
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor).ok())
    else {
        return;
    };
    // Nearest billboard whose middle passes within half its height of the ray.
    let reach = appearance.billboard_height / 2.;
    let hit = markers
        .iter()
        .filter_map(|(marker, pad, transform)| {
            let middle = transform.translation() + Vec3::Y * reach;
            let along = (middle - ray.origin).dot(*ray.direction);
            let miss = middle.distance(ray.get_point(along));
            (along > 0. && miss < reach).then_some((along, marker, pad))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, marker, pad)) = hit {
        let focus = match marker {
            Marker::Spawn | Marker::Player => CameraFocus::Player,
            Marker::Poi(_) | Marker::Bench => CameraFocus::Node(pad.0),
        };
        if view.focus != focus {
            view.focus = focus;
        }
    }
}

/// Copies the board camera's rotation to every [`Billboard`] quad each frame.
///
/// Currently targets [`HexPathsCamera`], not whichever camera is globally
/// active. If moved into common code for multiple cameras, a `BillboardTarget`
/// relationship could make the intended camera explicit for each billboard.
pub(super) fn face_camera(
    camera: Single<&Transform, (With<HexPathsCamera>, Without<Billboard>)>,
    mut billboards: Query<&mut Transform, With<Billboard>>,
) {
    for mut transform in &mut billboards {
        transform.rotation = camera.rotation;
    }
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

/// Repositions the camera when the board, focus, or vertical exaggeration changes.
///
/// [`Board::focus`] resolves the target in world coordinates. Keeps the
/// current orientation and resets distance to the configured terrain or
/// node/player starting distance. The local cache includes exaggeration
/// because the same focus moves vertically when the displayed scale changes.
pub(super) fn aim_camera(
    board: Option<Res<Board>>,
    view: Res<HexPathsView>,
    appearance: Res<HexPathsAppearance>,
    mut aimed: Local<Option<(CameraFocus, f32)>>,
    mut camera: Single<&mut Transform, With<HexPathsCamera>>,
) {
    let Some(board) = board else {
        return;
    };
    // The vertical scale moves the focus, so it re-aims too.
    let aim = (view.focus, appearance.vertical_scale);
    if !board.is_changed() && *aimed == Some(aim) {
        return;
    }
    *aimed = Some(aim);
    let distance = match view.focus {
        CameraFocus::Terrain => view.initial_distance,
        CameraFocus::Player | CameraFocus::Node(_) => view.player_distance,
    };
    camera.translation =
        board.focus(view.focus, appearance.vertical_scale) - camera.forward() * distance;
}

/// Feeds orbit and zoom input to the board's editor-camera controller.
///
/// Right-drag starts an orbit only within the board viewport and ends on
/// release. Wheel input can start a zoom there when motion is not already
/// user-controlled. The anchor comes from [`Board::focus`] at preview scale;
/// zoom motion ends once its smoothed input settles. There is no panning.
pub(super) fn orbit_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    appearance: Res<HexPathsAppearance>,
    window: Single<&Window, With<PrimaryWindow>>,
    board: Option<Res<Board>>,
    view: Res<HexPathsView>,
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
        .transform_point3(board.focus(view.focus, appearance.vertical_scale))
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

/// Outlines the playable tiles on the terrain, and draws the creeks,
/// crossings and trails over it; with nodes revealed, teases too.
pub(super) fn draw_board(
    board: Option<Res<Board>>,
    view: Res<HexPathsView>,
    appearance: Res<HexPathsAppearance>,
    mut gizmos: Gizmos<HexPathsGizmos>,
) {
    let Some(board) = board else {
        return;
    };
    let (mesh, layout) = (&board.terrain.mesh, &board.layout);
    let scale = appearance.vertical_scale;
    let corner = |c: usize, lift: f32| world(mesh.positions[c], layout.heights[c] * scale + lift);
    let points = point_heights(mesh, &layout.heights);
    let centre = |t: usize, lift: f32| {
        world(mesh.centers[t], points[mesh.positions.len() + t] * scale + lift)
    };
    for (corners, _) in mesh
        .tile_corners
        .iter()
        .zip(&mesh.playable)
        .filter(|(_, playable)| **playable)
    {
        gizmos.linestrip(
            corners.iter().chain([&corners[0]]).map(|&c| corner(c, appearance.grid_lift)),
            appearance.grid,
        );
    }
    let lift = appearance.line_lift;
    for (t, below) in board.terrain.creeks.downstream.iter().enumerate() {
        if let Some(n) = below {
            gizmos.line(centre(t, lift), centre(*n, lift), appearance.creek);
        }
    }
    for node in layout.nodes.iter().filter(|n| n.kind == NodeKind::Crossing) {
        let corners = mesh.tile_corners[node.tile];
        gizmos.linestrip(
            corners.iter().chain([&corners[0]]).map(|&c| corner(c, 2. * lift)),
            appearance.crossing,
        );
    }
    for (&e, &kind) in &layout.trail {
        let [a, b] = mesh.edges[e];
        let colour = if layout.breaking.contains(&e) {
            appearance.breaking_trail
        } else {
            appearance.trail(kind)
        };
        gizmos.line(corner(a, lift), corner(b, lift), colour);
    }
    if view.reveal_nodes {
        let tiles = layout.points();
        for &(v, t) in &layout.tease_pairs {
            let colour = if board.held.contains(&(v, t)) {
                appearance.tease_held
            } else {
                appearance.tease_broken
            };
            let up = Vec3::Y * appearance.billboard_height;
            gizmos.line(
                board.pad(tiles[v], scale) + up,
                board.pad(tiles[t], scale) + up,
                colour,
            );
        }
    }
}

/// Builds the visible mesh from levelled corner heights and mean-height tile centres.
///
/// Uses [`point_heights`] and the terrain mesh's triangles,
/// corrects winding to face up, and computes area-weighted vertex normals.
/// Normals drive hillshade; [`view_tint`] blends the chosen measure over an
/// absolute metre-height palette. Scenery fades toward paper. Vertical
/// exaggeration changes positions and shading, but not the palette's heights.
fn surface_mesh(
    board: &Board,
    settings: &HexPathsSettings,
    tuning: &HexTerrainTuning,
    view: TerrainView,
    appearance: &HexPathsAppearance,
) -> Mesh {
    let terrain = &board.terrain;
    let mesh = &terrain.mesh;
    let heights = point_heights(mesh, &board.layout.heights);
    let positions: Vec<Vec3> = mesh
        .positions
        .iter()
        .chain(&mesh.centers)
        .zip(&heights)
        .map(|(&p, &h)| world(p, h * appearance.vertical_scale))
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
    let tint = view_tint(board, settings, tuning, view, appearance);
    let colours: Vec<[f32; 4]> = heights
        .iter()
        .enumerate()
        .zip(&normals)
        .zip(&inside)
        .map(|(((i, &h), &n), &inside)| {
            let shade =
                appearance.shade_floor + appearance.shade_gain * n.dot(light).max(0.) / light.y;
            let base = tint.base.unwrap_or_else(|| appearance.ramp(h * tuning.edge_m));
            let lit = (base * shade).mix(&tint.colours[i], tint.amounts[i]);
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

/// Creates per-point colour targets and weights for the selected [`TerrainView`].
///
/// Heightmap contributes no tint. Cost uses levelled-ground steepness;
/// suitability uses original terrain scores sharpened by spread; BenchScore
/// blends quiet and overlook shares over paper. Arrays follow corners, then
/// tile centres, matching [`Tint`] and the surface point order.
fn view_tint(
    board: &Board,
    settings: &HexPathsSettings,
    tuning: &HexTerrainTuning,
    view: TerrainView,
    appearance: &HexPathsAppearance,
) -> Tint {
    let terrain = &board.terrain;
    let mesh = &terrain.mesh;
    let points = mesh.positions.len() + mesh.tiles.len();
    let single = |colour: Color, amounts: Vec<f32>| Tint {
        base: None,
        colours: vec![colour.to_linear(); points],
        amounts,
    };
    match view {
        TerrainView::Heightmap => single(Color::NONE, vec![0.; points]),
        TerrainView::Cost => single(
            appearance.cost,
            corner_amounts(terrain, &steepness(board, tuning, appearance)),
        ),
        TerrainView::Suitability => single(
            appearance.score,
            tile_amounts(
                terrain,
                &terrain
                    .suitability
                    .iter()
                    .map(|s| s.powf(settings.spread))
                    .collect::<Vec<_>>(),
            ),
        ),
        TerrainView::BenchScore => {
            let quiet = tile_amounts(terrain, &terrain.bench_quiet);
            let overlook = tile_amounts(terrain, &terrain.bench_overlook);
            let (quiet_colour, overlook_colour) =
                (appearance.quiet.to_linear(), appearance.overlook.to_linear());
            Tint {
                base: Some(appearance.paper.to_linear()),
                colours: quiet
                    .iter()
                    .zip(&overlook)
                    .map(|(q, o)| {
                        let towards_overlook = o / (q + o).max(f32::EPSILON);
                        quiet_colour.mix(&overlook_colour, towards_overlook)
                    })
                    .collect(),
                amounts: quiet.iter().zip(&overlook).map(|(q, o)| (q + o).min(1.)).collect(),
            }
        }
    }
}

/// Each corner's steepest edge over the levelled ground as
/// `aversion × |rise|`, the log of the step cost, scaled so the top few
/// percent of playable edges reach 1.
fn steepness(
    board: &Board,
    tuning: &HexTerrainTuning,
    appearance: &HexPathsAppearance,
) -> Vec<f32> {
    let mesh = &board.terrain.mesh;
    let steepness: Vec<f32> = board
        .terrain
        .step_costs(&board.layout.heights, tuning.aversion)
        .into_iter()
        .map(f32::ln)
        .collect();
    let mut playable: Vec<f32> = steepness.iter().copied().filter(|s| s.is_finite()).collect();
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

/// Per-point amounts from per-corner ones: centres take the mean of their
/// corners.
fn corner_amounts(terrain: &Terrain, corners: &[f32]) -> Vec<f32> {
    let centres = terrain
        .mesh
        .tile_corners
        .iter()
        .map(|tile| tile.iter().map(|&c| corners[c]).sum::<f32>() / 6.);
    corners.iter().copied().chain(centres).collect()
}

/// Per-point amounts from per-tile ones: corners take the mean of the tiles
/// they belong to.
fn tile_amounts(terrain: &Terrain, tiles: &[f32]) -> Vec<f32> {
    let mesh = &terrain.mesh;
    let mut sums = vec![(0., 0.); mesh.positions.len()];
    for (corners, &amount) in mesh.tile_corners.iter().zip(tiles) {
        for &c in corners {
            sums[c].0 += amount;
            sums[c].1 += 1.;
        }
    }
    sums.into_iter()
        .map(|(sum, count)| sum / count)
        .chain(tiles.iter().copied())
        .collect()
}
