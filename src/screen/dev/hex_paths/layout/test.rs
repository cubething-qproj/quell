//! Seeded properties for layout connectivity, crossings, and regeneration.

use super::{super::terrain::tests::small, *};

/// Checks seeded layout reachability, no off-pad dead ends, and crossing placement.
///
/// Also requires two trail-bearing banks per crossing and dry ordinary nodes
/// and spawn. These assertions inspect this sample, not all possible seeds.
fn assert_sound(context: Context, layout: &Layout) {
    let terrain = context.terrain;
    assert_eq!(layout.pace(context).unreachable, 0);
    assert!(layout.dead_ends(context).is_empty());
    assert_eq!(layout.one_sided(context), 0);
    let main = terrain.creeks.main_ha(&terrain.mesh);
    for node in &layout.nodes {
        let on_creek = terrain.creeks.on(node.tile);
        match node.kind {
            NodeKind::Crossing => assert!(on_creek && terrain.creeks.flow_ha[node.tile] >= main),
            NodeKind::Poi | NodeKind::Bench => assert!(!on_creek),
        }
    }
    assert!(!terrain.creeks.on(layout.spawn));
}

#[test]
fn layouts_reach_every_node_and_cross_creeks_only_at_crossings() {
    let mut crossings = 0;
    for seed in 1..=3 {
        let (settings, tuning, terrain) = small(seed);
        let context = Context {
            terrain: &terrain,
            settings: &settings,
            tuning: &tuning,
        };
        let layout = Layout::generate(context, seed, None).expect("a layout");
        assert_sound(context, &layout);
        assert!(layout.trail.keys().all(|&e| !layout.blocked[e]));
        assert!(layout.tease_pairs.iter().all(|p| !layout.broke.contains(p)));
        crossings += layout
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Crossing)
            .count();
    }
    assert!(crossings > 0, "the seeds exercise crossings");
}

#[test]
fn regeneration_keeps_crossings_and_the_deck() {
    let (settings, tuning, terrain) = small(2);
    let context = Context {
        terrain: &terrain,
        settings: &settings,
        tuning: &tuning,
    };
    let first = Layout::generate(context, 2, None).expect("a layout");
    let poi = first
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Poi)
        .expect("a point of interest");
    let crossings = |l: &Layout| -> Vec<Node> {
        l.all_nodes()
            .filter(|n| n.kind == NodeKind::Crossing)
            .copied()
            .collect()
    };
    let deck = |l: &Layout| -> Vec<(u32, Option<Card>)> {
        let mut cards: Vec<_> = l.all_nodes().map(|n| (n.id, n.card)).collect();
        cards.sort_by_key(|c| c.0);
        cards
    };
    for fear in [false, true] {
        let next = first
            .regenerate(context, regeneration_seed(2, 1), poi.tile, fear)
            .expect("a regeneration");
        assert_sound(context, &next);
        assert_eq!(next.spawn, poi.tile);
        assert_eq!(crossings(&next), crossings(&first));
        assert_eq!(deck(&next), deck(&first));
    }
    if let Some(crossing) = crossings(&first).first() {
        assert!(first.regenerate(context, 3, crossing.tile, true).is_err());
    }
}
