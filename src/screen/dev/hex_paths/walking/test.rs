//! Small, hand-authored networks for routing and continuous-time movement.

use super::super::{
    data::{HexPathsSettings, HexTerrainTuning},
    layout::{Context, Layout, TrailKind},
    terrain::{Terrain, tests::flat, world},
};
use super::data::Walk;
use std::collections::{BTreeMap, BTreeSet};

/// A spawn pad with a three-edge trail leading away from it; no erosion or placement.
fn network() -> (
    HexPathsSettings,
    HexTerrainTuning,
    Terrain,
    Layout,
    Vec<usize>,
) {
    let settings = HexPathsSettings::default();
    let tuning = HexTerrainTuning::default();
    let terrain = flat(4, 4, 2);
    let mesh = &terrain.mesh;
    let spawn = mesh.tile_at(1, 1, 2).unwrap();
    let pad = mesh.tile_corners[spawn];
    let mut path = vec![pad[0]];
    for _ in 0..3 {
        let &corner = path.last().unwrap();
        let next = mesh.corner_edges[corner]
            .iter()
            .map(|&(c, _)| c)
            .find(|c| !path.contains(c) && !pad.contains(c))
            .unwrap();
        path.push(next);
    }
    let trail = path
        .windows(2)
        .map(|pair| (mesh.edge(pair[0], pair[1]).unwrap(), TrailKind::Span))
        .collect::<BTreeMap<_, _>>();
    let layout = Layout {
        spawn,
        nodes: Vec::new(),
        player_node: None,
        heights: terrain.heights.clone(),
        blocked: vec![false; mesh.edges.len()],
        trail,
        tease_pairs: Vec::new(),
        disqualified: BTreeSet::new(),
        breaking: BTreeSet::new(),
        broke: BTreeSet::new(),
        ports: Vec::new(),
    };
    (settings, tuning, terrain, layout, path)
}

#[test]
fn routes_use_only_trails_and_real_cost_pad_edges() {
    let (settings, tuning, terrain, mut layout, path) = network();
    let context = Context {
        terrain: &terrain,
        settings: &settings,
        tuning: &tuning,
    };
    let mesh = &terrain.mesh;
    let start = mesh.tile_corners[layout.spawn][3];
    let target = *path.last().unwrap();
    let route = layout.walking_route(context, start, target).unwrap();
    assert_eq!(route.last(), Some(&target));
    let pad: BTreeSet<_> = mesh.perimeter(layout.spawn).into_iter().collect();
    let mut before = start;
    for &next in &route {
        let edge = mesh
            .edge(before, next)
            .expect("consecutive route corners share an edge");
        assert!(layout.trail.contains_key(&edge) || pad.contains(&edge));
        before = next;
    }
    let costs = layout.walking_costs(context);
    assert!(pad.iter().all(|&e| costs[e] >= 1. && costs[e].is_finite()));
    let off_network = mesh
        .positions
        .iter()
        .enumerate()
        .find(|(c, _)| {
            mesh.corner_edges[*c]
                .iter()
                .all(|&(_, e)| !costs[e].is_finite())
        })
        .map(|(c, _)| c)
        .unwrap();
    assert!(layout.walking_route(context, start, off_network).is_none());
    assert_eq!(
        layout.walking_route(context, start, start),
        Some(Vec::new())
    );
    layout.blocked[mesh.edge(path[0], path[1]).unwrap()] = true;
    assert!(layout.walking_route(context, start, target).is_none());
}

#[test]
fn picking_uses_the_hovered_tile_and_inverts_rendered_z() {
    let (settings, tuning, terrain, layout, _) = network();
    let context = Context {
        terrain: &terrain,
        settings: &settings,
        tuning: &tuning,
    };
    let mesh = &terrain.mesh;
    let walk = Walk::at_spawn(context, &layout);
    let expected = mesh.tile_corners[layout.spawn][3];
    let point = mesh.centers[layout.spawn].lerp(mesh.positions[expected], 0.9);
    assert!(point.y.abs() > 1., "the hit is away from the mirror axis");
    let target = walk
        .destination(context, &layout, world(point, 0.))
        .unwrap();
    assert_eq!(target.tile, layout.spawn);
    assert_eq!(target.corner, expected);
    let costs = layout.walking_costs(context);
    let empty = (0..mesh.tiles.len())
        .find(|&t| mesh.playable[t] && mesh.perimeter(t).iter().all(|&e| !costs[e].is_finite()))
        .unwrap();
    assert!(
        walk.destination(context, &layout, world(mesh.centers[empty], 0.))
            .is_none(),
        "empty ground never snaps to a distant path"
    );
    assert!(
        walk.destination(context, &layout, world(mesh.centers[0], 0.))
            .is_none(),
        "scenery cannot select a trail destination"
    );
}

#[test]
fn elapsed_time_carries_across_edges_independently_of_frame_size() {
    let (settings, tuning, terrain, layout, path) = network();
    let context = Context {
        terrain: &terrain,
        settings: &settings,
        tuning: &tuning,
    };
    let mut coarse = Walk::at_spawn(context, &layout);
    assert!(coarse.plan(context, &layout, *path.last().unwrap()));
    let mut fine = coarse.clone();
    coarse.advance(context, &layout, 55.);
    for _ in 0..550 {
        fine.advance(context, &layout, 0.1);
    }
    assert_ne!(coarse.corner, path[0], "elapsed time spans multiple edges");
    assert!(
        !coarse.route.is_empty(),
        "the player is still on the last leg"
    );
    assert_eq!(coarse.corner, fine.corner);
    assert_eq!(coarse.route, fine.route);
    assert!((coarse.progress - fine.progress).abs() < 1e-10);
    coarse.advance(context, &layout, 1000.);
    assert_eq!(coarse.corner, *path.last().unwrap());
    assert!(coarse.route.is_empty());
    assert_eq!(coarse.progress, 0.);
}

#[test]
fn redirect_finishes_the_current_edge_and_unreachable_requests_keep_the_route() {
    let (settings, tuning, terrain, layout, path) = network();
    let context = Context {
        terrain: &terrain,
        settings: &settings,
        tuning: &tuning,
    };
    let mut walk = Walk::at_spawn(context, &layout);
    assert!(walk.plan(context, &layout, *path.last().unwrap()));
    walk.advance(context, &layout, 1.);
    let before = walk.clone();
    assert!(!walk.plan(context, &layout, usize::MAX));
    assert_eq!(walk.route, before.route);
    assert_eq!(walk.progress, before.progress);
    assert!(walk.plan(context, &layout, path[0]));
    assert_eq!(
        walk.route.iter().copied().collect::<Vec<_>>(),
        [path[1], path[0]]
    );
    assert_eq!(walk.progress, before.progress);
    walk.advance(context, &layout, 1000.);
    assert_eq!(walk.corner, path[0]);
    assert!(walk.route.is_empty());
}

#[test]
fn directional_pace_and_display_heights_use_the_unexaggerated_slope() {
    let (settings, tuning, terrain, mut layout, path) = network();
    let context = Context {
        terrain: &terrain,
        settings: &settings,
        tuning: &tuning,
    };
    let (a, b) = (path[0], path[1]);
    layout.heights[b] = 0.2;
    let mut uphill = Walk::at_spawn(context, &layout);
    assert!(uphill.plan(context, &layout, b));
    let mut downhill = uphill.clone();
    downhill.corner = b;
    downhill.route = [a].into();
    uphill.advance(context, &layout, 1.);
    downhill.advance(context, &layout, 1.);
    assert!(downhill.progress > uphill.progress);
    let actual = uphill.position(context, &layout, 1.);
    let exaggerated = uphill.position(context, &layout, 4.);
    assert_eq!(actual.x, exaggerated.x);
    assert_eq!(actual.z, exaggerated.z);
    assert!((actual.y * 4. - exaggerated.y).abs() < 1e-6);
    assert!((actual.y - 0.2 * uphill.progress as f32).abs() < 1e-6);
}
