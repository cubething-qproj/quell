//! Creek barriers, crossing placement, and bank connections for [`Layout`].
//!
//! [`Terrain::creek_sides`] turns tile-centre drainage into corner-graph barriers.
//! [`corner_parts`] partitions that graph with union-find; [`trunk_sites`]
//! chooses main-creek crossing pads that reconnect substantial parts, then
//! spreads additional crossings. [`cut_off`] excludes unreachable node sites.
//!
//! After node placement, [`drop_dead_crossings`] removes pads without two useful
//! banks leading to different nodes. After trail routing, [`Layout::join_banks`]
//! attempts a trail from each missing bank, and [`Layout::one_sided`] measures
//! the result. Crossings remain fixed in [`Pins`](super::Pins) through regeneration.

use super::{Context, Layout, Node, NodeKind, TrailKind};
use crate::screen::dev::hex_paths::terrain::{HexMesh, Terrain, window::bank_span};
use std::collections::BTreeSet;

/// Lists interior tiles off the creeks, used for ordinary node placement.
///
/// Indices refer to [`Terrain::mesh`]. Crossing placement uses these tiles to
/// measure whether each disconnected corner part has room for landmarks.
pub fn dry_tiles(terrain: &Terrain) -> Vec<usize> {
    let interior = terrain.mesh.interior();
    (0..terrain.mesh.tiles.len())
        .filter(|&t| interior[t] && !terrain.creeks.on(t))
        .collect()
}

/// Builds a per-edge barrier mask, opening creek sides on crossing-pad perimeters.
///
/// Begins with [`Terrain::creek_sides`]. `crossings` holds tile indices, not
/// node or edge indices. [`Layout`] stores the resulting mask for routing.
pub fn creek_closed(terrain: &Terrain, crossings: &[usize]) -> Vec<bool> {
    let mut closed = vec![false; terrain.mesh.edges.len()];
    terrain.creek_sides().into_iter().for_each(|e| closed[e] = true);
    for &t in crossings {
        terrain.mesh.perimeter(t).into_iter().for_each(|e| closed[e] = false);
    }
    closed
}

/// Labels corners by connected part under creek barriers and extra closed edges.
///
/// Union-find joins the endpoints of every playable edge not shut by
/// [`creek_closed`] or `closed`. The latter contains edge indices;
/// `crossings` contains tile indices. Equal labels mean corners can reach
/// each other on the available ground, before trails are laid. Labels are
/// representative corner indices, not dense or stable part IDs.
pub fn corner_parts(terrain: &Terrain, crossings: &[usize], closed: &[usize]) -> Vec<usize> {
    let mesh = &terrain.mesh;
    let mut shut = creek_closed(terrain, crossings);
    closed.iter().for_each(|&e| shut[e] = true);
    let mut parent: Vec<usize> = (0..mesh.positions.len()).collect();
    /// Finds a corner's union-find representative, shortening its parent chain.
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for (e, &[a, b]) in mesh.edges.iter().enumerate() {
        if mesh.playable_edges[e] && !shut[e] {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            parent[ra] = rb;
        }
    }
    (0..parent.len()).map(|x| find(&mut parent, x)).collect()
}

/// Marks tiles forbidden to ordinary nodes under the current creek barriers.
///
/// Creek and non-interior tiles stay forbidden. An interior dry tile is allowed
/// when its first corner shares spawn's corner-part label from [`corner_parts`].
/// The mask becomes a hard [`Constraints`](super::super::terrain::sites::Constraints)
/// exclusion during placement.
pub fn cut_off(terrain: &Terrain, crossings: &[usize], spawn: usize) -> Vec<bool> {
    let label = corner_parts(terrain, crossings, &[]);
    let corners = &terrain.mesh.tile_corners;
    let home = label[corners[spawn][0]];
    let mut cut = vec![true; terrain.mesh.tiles.len()];
    for t in dry_tiles(terrain) {
        cut[t] = label[corners[t][0]] != home;
    }
    cut
}

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
        .filter(|&t| t != avoid && interior[t] && terrain.creeks.on(t) && terrain.creeks.flow_ha[t] >= main)
        .collect();
    let bank = |t: usize| bank_span(mesh.tile_corners[t].map(|c| terrain.heights[c]));
    let mut by_bank = trunk.clone();
    by_bank.sort_by(|&a, &b| bank(a).total_cmp(&bank(b)));
    let dry = dry_tiles(terrain);
    let mut chosen: Vec<usize> = Vec::new();
    for t in by_bank {
        let label = corner_parts(terrain, &chosen, &[]);
        let mut sizes = vec![0u32; label.len()];
        dry.iter().for_each(|&d| sizes[label[mesh.tile_corners[d][0]]] += 1);
        let roots: BTreeSet<usize> = mesh.tile_corners[t].iter().map(|&c| label[c]).collect();
        if roots.iter().filter(|&&r| sizes[r] >= context.tuning.min_part).count() > 1 {
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
            let label = corner_parts(terrain, &[], &mesh.perimeter(t));
            let mut sizes = vec![0u32; label.len()];
            dry.iter().for_each(|&d| sizes[label[mesh.tile_corners[d][0]]] += 1);
            pad_banks(mesh, t, &sides)
                .iter()
                .filter(|bank| bank.iter().any(|&c| sizes[label[c]] >= context.tuning.min_part))
                .count()
                >= 2
        })
        .collect();
    let middle = trunk.iter().map(|&t| mesh.centers[t]).sum::<bevy::math::Vec2>() / trunk.len().max(1) as f32;
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
fn bank_reach(terrain: &Terrain, crossing: &Node, nodes: &[Node], spawn: usize) -> Vec<BTreeSet<usize>> {
    let mesh = &terrain.mesh;
    let others: Vec<usize> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Crossing && n.id != crossing.id)
        .map(|n| n.tile)
        .collect();
    let label = corner_parts(terrain, &others, &mesh.perimeter(crossing.tile));
    let targets: Vec<usize> = std::iter::once(spawn)
        .chain(nodes.iter().filter(|n| n.kind != NodeKind::Crossing).map(|n| n.tile))
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
        let dead = nodes.iter().filter(|n| n.kind == NodeKind::Crossing).find(|c| {
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

impl Layout {
    /// Attempts trails from crossing banks without an exit to another trail or pad.
    ///
    /// Existing trail edges and the crossing perimeter are impassable during
    /// the search, preventing a false bank connection round the pad. Candidate
    /// routes are tried cheapest first under [`Self::lay`]'s tease guard, then
    /// the cheapest route is accepted unguarded if needed. Banks with no route
    /// remain unconnected and are reported by [`Self::one_sided`].
    pub(super) fn join_banks(&mut self, context: Context) {
        let mesh = &context.terrain.mesh;
        let sides: BTreeSet<usize> = context.terrain.creek_sides().into_iter().collect();
        let costs = self.step_costs(context);
        let pads = self.pad_corners(context);
        let crossings: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Crossing)
            .map(|n| n.tile)
            .collect();
        for tile in crossings {
            let pad: BTreeSet<usize> = mesh.tile_corners[tile].into_iter().collect();
            let on_trail = self.trail_corners(context);
            for bank in pad_banks(mesh, tile, &sides) {
                if bank.iter().any(|&v| self.leaves_pad(context, v, &pad)) {
                    continue;
                }
                let mut weights = costs.clone();
                self.trail.keys().for_each(|&e| weights[e] = f32::INFINITY);
                mesh.perimeter(tile).into_iter().for_each(|e| weights[e] = f32::INFINITY);
                let targets: BTreeSet<usize> = on_trail.union(&pads).copied().filter(|c| !pad.contains(c)).collect();
                let mut paths: Vec<Vec<usize>> = bank
                    .iter()
                    .filter_map(|&v| self.corner_route(context, v, &weights, &targets))
                    .collect();
                let cost = |p: &Vec<usize>| p.iter().map(|&e| weights[e]).sum::<f32>();
                paths.sort_by(|a, b| cost(a).total_cmp(&cost(b)));
                if !paths.is_empty() && !self.lay(context, &paths, TrailKind::Bank, true) {
                    self.lay(context, &paths[..1], TrailKind::Bank, false);
                }
            }
        }
    }

    /// Whether a trail leaves corner `v` away from the pad's perimeter.
    fn leaves_pad(&self, context: Context, v: usize, pad: &BTreeSet<usize>) -> bool {
        self.trail.keys().any(|&e| {
            let [a, b] = context.terrain.mesh.edges[e];
            (a == v || b == v) && !(pad.contains(&a) && pad.contains(&b))
        })
    }

    /// Counts crossings with fewer than two banks carrying an outward trail.
    ///
    /// An exit must leave the pad, rather than follow its perimeter. This measures
    /// the laid network after [`Self::join_banks`], unlike [`bank_reach`]'s
    /// potential ground connectivity; [`Board`](super::super::data::Board) displays it.
    pub fn one_sided(&self, context: Context) -> usize {
        let mesh = &context.terrain.mesh;
        let sides: BTreeSet<usize> = context.terrain.creek_sides().into_iter().collect();
        self.nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Crossing)
            .filter(|n| {
                let pad: BTreeSet<usize> = mesh.tile_corners[n.tile].into_iter().collect();
                pad_banks(mesh, n.tile, &sides)
                    .iter()
                    .filter(|bank| bank.iter().any(|&v| self.leaves_pad(context, v, &pad)))
                    .count()
                    < 2
            })
            .count()
    }
}
