//! Session creation, terrain picking, and movement for the preview walker.
//!
//! [`sync_player`] replaces or removes the walking entity when mode or board
//! ownership changes. [`pick_destination`] accepts surface hits from the preview
//! camera. [`advance_player`] changes only the walker and its transform; marker
//! rebuilds and view changes therefore leave an active route intact.

use super::{
    super::{data::*, layout::Context, terrain::world},
    data::{Walk, WalkingSettings},
};
use crate::prelude::*;
use bevy::{
    picking::{events::Press, hover::HoverMap, pointer::PointerId},
    prelude::*,
};

/// Applies walking-mode changes and ends the session when its board is replaced.
///
/// The player is a separate child of [`BoardRoot`], without [`Marker`], so
/// landmark rebuilds leave it alone. Rebuilding those landmarks on mode changes
/// shows a spawn image there instead of a second, stationary player image.
pub fn sync_player(
    mut commands: Commands,
    board: Option<Res<Board>>,
    mut settings: ResMut<WalkingSettings>,
    generation: Res<HexPathsSettings>,
    tuning: Res<HexTerrainTuning>,
    appearance: Res<HexPathsAppearance>,
    billboards: Res<Billboards>,
    root: Option<Single<Entity, With<BoardRoot>>>,
    walkers: Query<Entity, With<Walk>>,
) {
    let replaced = board.as_ref().is_none_or(|b| b.is_changed());
    if replaced && settings.enabled {
        settings.enabled = false;
    }
    if !settings.enabled {
        for entity in &walkers {
            commands.entity(entity).despawn();
            commands.trigger(RebuildMarkers);
        }
    } else if walkers.is_empty()
        && let (Some(board), Some(root)) = (board, root)
    {
        let context = Context {
            terrain: &board.terrain,
            settings: &generation,
            tuning: &tuning,
        };
        let walk = Walk::at_spawn(context, &board.layout);
        let transform = Transform::from_translation(walk.position(
            context,
            &board.layout,
            appearance.vertical_scale,
        ));
        commands.spawn((
            Name::new("Walking player"),
            ScreenScoped,
            walk,
            Billboard,
            Pickable::IGNORE,
            Mesh3d(billboards.quad.clone()),
            MeshMaterial3d(billboards.player.clone()),
            transform,
            RenderLayers::from(RenderLayer::HEX_PREVIEW),
            ChildOf(*root),
        ));
        commands.trigger(RebuildMarkers);
    }
}

/// Plans a walk to the previewed path corner on a primary surface press.
///
/// Bevy picking handles viewport bounds and UI occlusion. Marker presses still
/// focus landmarks. [`Walk::destination`] handles board coordinates and rejects
/// empty or disconnected ground. [`draw_destination`] uses the same rule.
pub fn pick_destination(
    press: On<Pointer<Press>>,
    board: If<Res<Board>>,
    settings: Res<HexPathsSettings>,
    tuning: Res<HexTerrainTuning>,
    cameras: Query<(), With<HexPathsCamera>>,
    mut walker: Single<&mut Walk>,
) {
    if press.button != PointerButton::Primary || !cameras.contains(press.hit.camera) {
        return;
    }
    let Some(hit) = press.hit.position else {
        return;
    };
    let context = Context {
        terrain: &board.terrain,
        settings: &settings,
        tuning: &tuning,
    };
    if let Some(target) = walker.destination(context, &board.layout, hit) {
        walker.plan(context, &board.layout, target.corner);
    }
}

/// Outlines the hovered path tile and marks the corner a click would select.
///
/// Reads Bevy's resolved hover map so the preview disappears over blocking UI,
/// landmarks, and empty ground. Heights follow the same exaggerated surface
/// as the walker. Only hits from [`HexPathsCamera`] qualify.
pub fn draw_destination(
    board: If<Res<Board>>,
    settings: Res<HexPathsSettings>,
    tuning: Res<HexTerrainTuning>,
    appearance: Res<HexPathsAppearance>,
    walking: Res<WalkingSettings>,
    hover: Res<HoverMap>,
    surfaces: Query<(), With<TerrainSurface>>,
    cameras: Query<(), With<HexPathsCamera>>,
    walker: Single<&Walk>,
    mut gizmos: Gizmos<HexPathsGizmos>,
) {
    let hit = hover.get(&PointerId::Mouse).and_then(|hits| {
        hits.iter()
            .find(|(entity, hit)| surfaces.contains(**entity) && cameras.contains(hit.camera))
            .and_then(|(_, hit)| hit.position)
    });
    let context = Context {
        terrain: &board.terrain,
        settings: &settings,
        tuning: &tuning,
    };
    let Some(target) = hit.and_then(|hit| walker.destination(context, &board.layout, hit)) else {
        return;
    };
    let mesh = &board.terrain.mesh;
    let corner = |c: usize| {
        world(
            mesh.positions[c],
            board.layout.heights[c] * appearance.vertical_scale + 2. * appearance.line_lift,
        )
    };
    let corners = mesh.tile_corners[target.tile];
    gizmos.linestrip(
        corners.into_iter().chain([corners[0]]).map(corner),
        walking.hover_colour,
    );
    gizmos.sphere(
        Isometry3d::from_translation(corner(target.corner)),
        walking.target_radius,
        walking.hover_colour,
    );
}

/// Advances the player without changing [`Board`] or its render-change ticks.
///
/// Reprojects even while paused so appearance edits move the billboard with
/// the exaggerated surface. The camera retains its existing inspection focus.
pub fn advance_player(
    time: Res<Time>,
    board: If<Res<Board>>,
    settings: Res<HexPathsSettings>,
    tuning: Res<HexTerrainTuning>,
    walking: Res<WalkingSettings>,
    appearance: Res<HexPathsAppearance>,
    walker: Single<(&mut Walk, &mut Transform)>,
) {
    let (mut walk, mut transform) = walker.into_inner();
    let context = Context {
        terrain: &board.terrain,
        settings: &settings,
        tuning: &tuning,
    };
    walk.advance(
        context,
        &board.layout,
        time.delta_secs_f64() * walking.speed as f64,
    );
    transform.translation = walk.position(context, &board.layout, appearance.vertical_scale);
}
