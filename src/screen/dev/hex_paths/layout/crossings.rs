//! Creek barriers, crossing placement, and bank connections for [`Layout`](super::Layout).
//!
//! [`Terrain::creek_sides`] turns tile-centre drainage into corner-graph barriers.
//! [`Terrain::corner_parts`] partitions that graph with union-find; [`trunk_sites`]
//! chooses main-creek crossing pads that reconnect substantial parts, then
//! spreads additional crossings. [`Terrain::cut_off`] excludes unreachable node sites.
//!
//! After node placement, [`drop_dead_crossings`] removes pads without two useful
//! banks leading to different nodes. After trail routing, [`Layout::join_banks`](super::Layout::join_banks)
//! attempts a trail from each missing bank, and [`Layout::one_sided`](super::Layout::one_sided) measures
//! the result. Crossings remain fixed in [`Pins`](super::Pins) through regeneration.

use super::{Context, Node, NodeKind};
use crate::screen::dev::hex_paths::terrain::{HexMesh, Terrain, window::bank_span};
use std::collections::BTreeSet;

/// Chooses interior main-creek tile indices for crossing pads, avoiding `avoid`.
///
/// First opens low-bank sites that join corner parts with at least `min_part`
/// dry tiles each. These required crossings can exceed the requested count.
/// Then fills toward that count using widely separated sites with two banks
/// opening onto substantial dry parts. The first spread pick is far from
/// the trunk's middle; later picks maximise their nearest crossing distance.
/// The returned candidates are checked against placed nodes by
/// [`drop_dead_crossings`], so the final count can be smaller.
pub fn trunk_sites(context: Context, avoid: usize) -> Vec<usize> {
    let terrain = context.terrain;
    let mesh = &terrain.mesh;
    let interior = mesh.interior();
    let main = terrain.creeks.main_ha(mesh);
    let trunk: Vec<usize> = (0..mesh.tiles.len())
        .filter(|&t| {
            t != avoid && interior[t] && terrain.creeks.on(t) && terrain.creeks.flow_ha[t] >= main
        })
        .collect();
    let bank = |t: usize| bank_span(mesh.tile_corners[t].map(|c| terrain.heights[c]));
    let mut by_bank = trunk.clone();
    by_bank.sort_by(|&a, &b| bank(a).total_cmp(&bank(b)));
    let dry = terrain.dry_tiles();
    let mut chosen: Vec<usize> = Vec::new();
    for t in by_bank {
        let label = terrain.corner_parts(&chosen, &[]);
        let mut sizes = vec![0u32; label.len()];
        dry.iter()
            .for_each(|&d| sizes[label[mesh.tile_corners[d][0]]] += 1);
        let roots: BTreeSet<usize> = mesh.tile_corners[t].iter().map(|&c| label[c]).collect();
        if roots
            .iter()
            .filter(|&&r| sizes[r] >= context.tuning.min_part)
            .count()
            > 1
        {
            chosen.push(t);
        }
    }
    // Spread only over tiles that can lead somewhere: two banks or more,
    // each opening onto room for a node. The creek's head has one bank (you
    // walk round it), and a bank facing a sliver by the board's edge has no
    // room; either would be dropped later, leaving the creek uncrossed.
    let sides: BTreeSet<usize> = terrain.creek_sides().into_iter().collect();
    let trunk: Vec<usize> = trunk
        .into_iter()
        .filter(|&t| {
            let label = terrain.corner_parts(&[], &mesh.perimeter(t));
            let mut sizes = vec![0u32; label.len()];
            dry.iter()
                .for_each(|&d| sizes[label[mesh.tile_corners[d][0]]] += 1);
            pad_banks(mesh, t, &sides)
                .iter()
                .filter(|bank| {
                    bank.iter()
                        .any(|&c| sizes[label[c]] >= context.tuning.min_part)
                })
                .count()
                >= 2
        })
        .collect();
    let middle = trunk
        .iter()
        .map(|&t| mesh.centers[t])
        .sum::<bevy::math::Vec2>()
        / trunk.len().max(1) as f32;
    while chosen.len() < context.settings.crossings as usize {
        let far = |t: usize| {
            if chosen.is_empty() {
                mesh.centers[t].distance(middle)
            } else {
                chosen
                    .iter()
                    .map(|&c| mesh.centers[t].distance(mesh.centers[c]))
                    .fold(f32::INFINITY, f32::min)
            }
        };
        let Some(best) = trunk
            .iter()
            .copied()
            .filter(|t| !chosen.contains(t))
            .max_by(|&a, &b| far(a).total_cmp(&far(b)))
        else {
            break;
        };
        chosen.push(best);
    }
    chosen
}

/// Splits a pad's corners into banks separated by creek-crossed sides.
///
/// `sides` contains edge indices from [`Terrain::creek_sides`]. Consecutive
/// corners remain on one bank until their perimeter side is crossed by the
/// creek. Used for both crossing validity and missing-bank trail repair.
pub fn pad_banks(mesh: &HexMesh, tile: usize, sides: &BTreeSet<usize>) -> Vec<Vec<usize>> {
    let (corners, perimeter) = (mesh.tile_corners[tile], mesh.perimeter(tile));
    let mut banks: Vec<Vec<usize>> = Vec::new();
    for (i, &corner) in corners.iter().enumerate() {
        match banks.last_mut() {
            Some(bank) if !sides.contains(&perimeter[i]) => bank.push(corner),
            _ => banks.push(vec![corner]),
        }
    }
    if banks.len() > 1 && !sides.contains(&perimeter[0]) {
        let last = banks.pop().expect("more than one bank");
        banks[0].splice(0..0, last);
    }
    banks
}

/// Finds non-crossing node tiles each bank could reach over available ground.
///
/// Spawn also counts as a target. Closes this pad's perimeter so reachability
/// cannot just go round it to the opposite bank, while allowing other
/// crossings. This tests potential terrain routes, not the laid trail graph.
fn bank_reach(
    terrain: &Terrain,
    crossing: &Node,
    nodes: &[Node],
    spawn: usize,
) -> Vec<BTreeSet<usize>> {
    let mesh = &terrain.mesh;
    let others: Vec<usize> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Crossing && n.id != crossing.id)
        .map(|n| n.tile)
        .collect();
    let label = terrain.corner_parts(&others, &mesh.perimeter(crossing.tile));
    let targets: Vec<usize> = std::iter::once(spawn)
        .chain(
            nodes
                .iter()
                .filter(|n| n.kind != NodeKind::Crossing)
                .map(|n| n.tile),
        )
        .collect();
    let sides: BTreeSet<usize> = terrain.creek_sides().into_iter().collect();
    pad_banks(mesh, crossing.tile, &sides)
        .into_iter()
        .map(|bank| {
            let parts: BTreeSet<usize> = bank.iter().map(|&c| label[c]).collect();
            targets
                .iter()
                .copied()
                .filter(|&t| parts.contains(&label[mesh.tile_corners[t][0]]))
                .collect()
        })
        .collect()
}

/// Drops crossing nodes that cannot serve two banks and two different targets.
///
/// [`bank_reach`] tests ground reachability with this crossing's perimeter
/// closed. Spawn counts as a target. At least two banks must reach targets,
/// and their combined target set must contain at least two tiles. Removes one
/// invalid crossing at a time and rechecks, since crossings affect each
/// other's reachability. Ordinary nodes are retained.
pub fn drop_dead_crossings(terrain: &Terrain, mut nodes: Vec<Node>, spawn: usize) -> Vec<Node> {
    loop {
        let dead = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Crossing)
            .find(|c| {
                let reach: Vec<BTreeSet<usize>> = bank_reach(terrain, c, &nodes, spawn)
                    .into_iter()
                    .filter(|r| !r.is_empty())
                    .collect();
                let all: BTreeSet<usize> = reach.iter().flatten().copied().collect();
                reach.len() < 2 || all.len() < 2
            });
        match dead.map(|d| d.id) {
            Some(id) => nodes.retain(|n| n.id != id),
            None => return nodes,
        }
    }
}
