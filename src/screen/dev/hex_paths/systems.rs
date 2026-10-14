//! Bevy integration for pure terrain and layout generation, then preview rendering.
//!
//! [`spawn_board`] validates settings, obtains a cached or newly simulated
//! [`Region`], chooses [`Terrain`], builds [`Layout`], and installs [`Board`].
//! [`relayout`] keeps that terrain and attempts a new arrangement; failure
//! retains the old layout. Both repair tile-based camera focus after success.
//!
//! [`refresh_preview`] translates resource changes into rebuild events.
//! [`spawn_surface`] draws levelled layout heights, while [`spawn_markers`] and
//! [`paint_markers`] display landmark roles. Marker picking edits [`HexPathsView`];
//! surface picking requests a route from [`Walk`].
//! Camera systems resolve [`CameraFocus`] into an exaggerated world target.
//! [`draw_board`] overlays true topology as gizmos. [`screen`](super::screen) orders
//! these systems so generation precedes rendering and focus precedes aiming.
//! Visual height scaling never enters terrain costs, sight tests, or pace.

use super::walking::{
    data::{Exploration, Walk},
    systems::pick_destination,
};
use super::{
    data::*,
    layout::{Context, Layout, NodeKind, regeneration_seed},
    terrain::{
        Terrain,
        erosion::{Region, RegionKey},
        point_heights, world,
    },
};
use crate::prelude::*;
use bevy::{
    camera::Viewport,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    picking::events::Press,
    prelude::*,
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

/// Bridges resource changes to preview rebuild events after generation.
///
/// A board or appearance change replaces both surface and markers. A view
/// change rebuilds surface colours and requests [`UpdateMarkers`] for reveal.
/// [`If`] skips this bridge while no board exists. The screen's chained
/// schedule flushes these commands before camera and overlay updates.
pub(super) fn refresh_preview(
    mut commands: Commands,
    board: If<Res<Board>>,
    view: Res<HexPathsView>,
    appearance: Res<HexPathsAppearance>,
) {
    if board.is_changed() || view.is_changed() || appearance.is_changed() {
        commands.trigger(RebuildSurface);
    }
    if board.is_changed() || appearance.is_changed() {
        commands.trigger(RebuildMarkers);
    }
    if view.is_changed() {
        commands.trigger(UpdateMarkers);
    }
}

/// Rebuilds the [`TerrainSurface`] child beneath the current [`BoardRoot`].
///
/// [`Board::surface_mesh`] uses [`Layout::heights`] for levelled ground, applies
/// appearance exaggeration, and colours it with height, hillshade, and the
/// selected [`TerrainView`]. Observes [`RebuildSurface`]; parameter validation
/// skips the observer if the board or its single root is absent.
pub(super) fn spawn_surface(
    _: On<RebuildSurface>,
    mut commands: Commands,
    board: If<Res<Board>>,
    settings: Res<HexPathsSettings>,
    tuning: Res<HexTerrainTuning>,
    view: Res<HexPathsView>,
    appearance: Res<HexPathsAppearance>,
    root: Single<Entity, With<BoardRoot>>,
    surfaces: Query<Entity, With<TerrainSurface>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for surface in &surfaces {
        commands.entity(surface).despawn();
    }
    commands
        .spawn((
            TerrainSurface,
            Pickable::default(),
            Name::new("Terrain"),
            Mesh3d(meshes.add(board.surface_mesh(&settings, &tuning, view.view, &appearance))),
            MeshMaterial3d(materials.add(StandardMaterial {
                unlit: true,
                ..default()
            })),
            RenderLayers::from(RenderLayer::HEX_PREVIEW),
            ChildOf(*root),
        ))
        .observe(pick_destination);
}

/// Replaces billboard markers for spawn/player, POIs, and benches on the board.
///
/// Positions each [`Pad`] at its levelled centre using [`Board::pad`]. The
/// spawn/player distinction follows [`Board::regenerated`], rather than the
/// layout count. While walking, spawn remains a landmark and the separate
/// [`Walk`] entity displays the moving player. [`Marker::presentation`] sets
/// each initial image and visibility; [`paint_markers`] applies later discovery
/// and reveal changes. Observes [`RebuildMarkers`]; crossing pads remain
/// outlines drawn by [`draw_board`].
pub(super) fn spawn_markers(
    _: On<RebuildMarkers>,
    mut commands: Commands,
    board: If<Res<Board>>,
    view: Res<HexPathsView>,
    billboards: Res<Billboards>,
    appearance: Res<HexPathsAppearance>,
    root: Single<Entity, With<BoardRoot>>,
    markers: Query<Entity, With<Marker>>,
    walkers: Query<(), With<Walk>>,
    exploration: Option<Single<&Exploration>>,
) {
    for marker in &markers {
        commands.entity(marker).despawn();
    }
    let layout = &board.layout;
    let start = if board.regenerated && walkers.is_empty() {
        Marker::Player
    } else {
        Marker::Spawn
    };
    let exploration = exploration.as_deref().map(|s| &**s);
    let start = layout
        .player_node
        .filter(|_| exploration.is_some())
        .and_then(Marker::for_node)
        .unwrap_or(start);
    let nodes = layout
        .nodes
        .iter()
        .filter_map(|&node| Marker::for_node(node).map(|role| (node.tile, role, Some(node.id))));
    for (tile, marker, id) in
        std::iter::once((layout.spawn, start, layout.player_node.map(|n| n.id))).chain(nodes)
    {
        let (material, visibility) = marker.presentation(id, &view, &billboards, exploration);
        let mut entity = commands.spawn((
            Name::new("Marker"),
            marker,
            Pickable::default(),
            Pad(tile),
            Billboard,
            Mesh3d(billboards.quad.clone()),
            MeshMaterial3d(material),
            visibility,
            Transform::from_translation(board.pad(tile, appearance.vertical_scale)),
            RenderLayers::from(RenderLayer::HEX_PREVIEW),
            ChildOf(*root),
        ));
        entity.observe(pick_focus);
        if let Some(id) = id {
            entity.insert(NodeMarker(id));
        }
    }
}

/// Updates existing billboard images and visibility after view or discovery changes.
///
/// [`Marker::presentation`] combines local sight, visited node IDs, and the
/// Reveal override. [`spawn_markers`] assigns the same presentation at spawn.
/// Observing [`UpdateMarkers`] avoids scanning markers on unchanged frames.
pub(super) fn paint_markers(
    _: On<UpdateMarkers>,
    view: Res<HexPathsView>,
    billboards: Res<Billboards>,
    exploration: Option<Single<&Exploration>>,
    mut markers: Query<(
        &Marker,
        Option<&NodeMarker>,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
    )>,
) {
    let exploration = exploration.as_deref().map(|s| &**s);
    for (marker, id, mut material, mut visibility) in &mut markers {
        let (handle, shown) = marker.presentation(id.map(|n| n.0), &view, &billboards, exploration);
        if material.0 != handle {
            material.0 = handle;
        }
        visibility.set_if_neq(shown);
    }
}

/// Focuses a billboard on a primary pointer press from the board camera.
///
/// Attached to each marker by [`spawn_markers`]. Bevy mesh picking handles
/// hit ordering, viewport clipping, and UI occlusion. Spawn/player select
/// Player focus; POIs and benches select their tile in [`CameraFocus::Node`].
/// This sets camera focus only, not a walking destination.
pub(super) fn pick_focus(
    press: On<Pointer<Press>>,
    cameras: Query<(), With<HexPathsCamera>>,
    markers: Query<(&Marker, &Pad)>,
    mut view: ResMut<HexPathsView>,
) {
    if press.button != PointerButton::Primary || !cameras.contains(press.hit.camera) {
        return;
    }
    let Ok((marker, pad)) = markers.get(press.entity) else {
        return;
    };
    let focus = match marker {
        Marker::Spawn | Marker::Player => CameraFocus::Player,
        Marker::Poi(_) | Marker::Bench => CameraFocus::Node(pad.0),
    };
    if view.focus != focus {
        view.focus = focus;
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
    board: If<Res<Board>>,
    view: Res<HexPathsView>,
    appearance: Res<HexPathsAppearance>,
    mut aimed: Local<Option<(CameraFocus, f32)>>,
    mut camera: Single<&mut Transform, With<HexPathsCamera>>,
) {
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
    board: If<Res<Board>>,
    view: Res<HexPathsView>,
    camera: Single<(&Camera, &Transform, &mut EditorCam), With<HexPathsCamera>>,
) {
    let (camera, transform, mut controller) = camera.into_inner();
    if mouse.just_released(MouseButton::Right) {
        controller.end_move();
    }
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
    let settled = controller
        .current_motion
        .inputs()
        .is_none_or(|inputs| inputs.zoom_velocity_abs(controller.smoothing.zoom.mul_f32(2.)) == 0.);
    if controller.current_motion.is_zooming_only() && settled {
        controller.end_move();
    }
}

/// Outlines the playable tiles on the terrain, and draws the creeks,
/// crossings and trails over it; with nodes revealed, teases too.
pub(super) fn draw_board(
    board: If<Res<Board>>,
    view: Res<HexPathsView>,
    appearance: Res<HexPathsAppearance>,
    exploration: Option<Single<&Exploration>>,
    mut gizmos: Gizmos<HexPathsGizmos>,
) {
    let (mesh, layout) = (&board.terrain.mesh, &board.layout);
    let scale = appearance.vertical_scale;
    let corner = |c: usize, lift: f32| world(mesh.positions[c], layout.heights[c] * scale + lift);
    let points = point_heights(mesh, &layout.heights);
    let centre = |t: usize, lift: f32| {
        world(
            mesh.centers[t],
            points[mesh.positions.len() + t] * scale + lift,
        )
    };
    for (corners, _) in mesh
        .tile_corners
        .iter()
        .zip(&mesh.playable)
        .filter(|(_, playable)| **playable)
    {
        gizmos.linestrip(
            corners
                .iter()
                .chain([&corners[0]])
                .map(|&c| corner(c, appearance.grid_lift)),
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
        if exploration
            .as_ref()
            .is_some_and(|s| !s.nearby.contains(&node.id))
        {
            continue;
        }
        let corners = mesh.tile_corners[node.tile];
        gizmos.linestrip(
            corners
                .iter()
                .chain([&corners[0]])
                .map(|&c| corner(c, 2. * lift)),
            appearance.crossing,
        );
    }
    for (&e, &kind) in &layout.trail {
        if exploration
            .as_ref()
            .is_some_and(|s| !s.visible.contains(&e))
        {
            continue;
        }
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
