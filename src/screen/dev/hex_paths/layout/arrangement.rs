//! The layout definition and its placement, routing, levelling, and measurement methods.
//!
//! [`Layout::generate`] builds over fixed [`Terrain`](super::super::terrain::Terrain), with optional [`Pins`].
//! Methods below preserve that ground while constructing pads and a trail graph.
//! [`crossings`] selects viable creek pads; [`network`](super::network) supplies
//! shortest-path and contracted-graph data used by the layout methods.

use super::crossings::pad_banks;
use super::{
    Pace, crossings,
    data::{Card, Context, Node, NodeKind, Pins, TrailKind},
    network::{Contracted, Nearest, Search, path_end},
};
use crate::prelude::*;
use crate::screen::dev::hex_paths::terrain::{SQRT_3, sites};
use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

/// Ids of crossing nodes start here, after points of interest and benches.
const FIRST_CROSSING_ID: u32 = 1000;

/// Cards dealt once per fresh layout, up to the number of available POIs.
const DECK: [Card; 3] = [Card::Key, Card::Npc, Card::Fear];

/// Near-zero perimeter weight used to model pads as freely traversable plazas.
const PLAZA: f32 = 1e-6;

/// Assumed length reduction from honeycomb zigzags, used only in pace estimates.
/// The preview still draws the original edges; no spline is built here.
const SMOOTHING: f32 = SQRT_3 / 2.;

/// Maximum candidate routes tried by connection and loop passes for one addition.
const CANDIDATES: usize = 40;

/// One arrangement of landmarks, levelled pads, and trails over a [`Terrain`](super::super::terrain::Terrain).
///
/// [`Self::generate`] constructs this from ground, settings, a seed, and optional
/// [`Pins`]. [`Self::regenerate`] preserves selected node identities and trail
/// edges while rebuilding the arrangement on the same ground. Heights are a
/// copy of [`Terrain::heights`](super::super::terrain::Terrain::heights) with pad levelling; removing a pad on the next
/// build restores its original terrain rather than retaining its old grading.
///
/// Routing point indices are local to this arrangement: point zero is spawn,
/// and point `i > 0` is `nodes[i - 1]`, as returned by [`Self::points`]. These
/// indices identify tease pairs, not stable [`Node::id`] values. Tile, corner,
/// and edge indices refer to the mesh of the terrain owned alongside this
/// layout by [`Board`](super::super::data::Board).
#[derive(Debug, Clone)]
pub struct Layout {
    /// Spawn tile, or the player tile after regeneration; routing point zero.
    pub spawn: usize,
    /// Placed nodes in stable-ID order, excluding the node under the player.
    /// Point indices can still shift when a node moves into [`Self::player_node`].
    pub nodes: Vec<Node>,
    /// The node on spawn's tile (where the player stood), if any.
    pub player_node: Option<Node>,
    /// Levelled corner heights in edge lengths, copied from [`Terrain::heights`](super::super::terrain::Terrain::heights).
    pub heights: Vec<f32>,
    /// Edges trails may not use: creek sides away from crossings.
    pub blocked: Vec<bool>,
    /// Laid mesh-edge indices and the pass that first added each edge.
    pub trail: BTreeMap<usize, TrailKind>,
    /// Selected `(viewer, target)` routing-point indices, as in [`Self::points`].
    /// Visibility and surviving detour are measured separately by the Board.
    pub tease_pairs: Vec<(usize, usize)>,
    /// Tease pairs not used because a route had to break them.
    pub disqualified: BTreeSet<(usize, usize)>,
    /// Trail edges laid unguarded that broke a tease, and the teases broken.
    pub breaking: BTreeSet<usize>,
    /// Pairs broken by accepted unguarded routes, used for disqualification and retry.
    pub broke: BTreeSet<(usize, usize)>,
    /// Open ends of pinned trails, which must join the new network.
    pub ports: Vec<usize>,
}

impl Layout {
    /// Returns spawn's tile, then node tiles, in routing-point order.
    ///
    /// Tease pairs and walking-distance vectors use indices into this order.
    /// The node under the player is represented by spawn, not a second entry.
    pub fn points(&self) -> Vec<usize> {
        std::iter::once(self.spawn)
            .chain(self.nodes.iter().map(|n| n.tile))
            .collect()
    }

    /// Looks up a node by routing-point index; spawn (zero) returns `None`.
    ///
    /// Nonzero `i` must be a valid index in [`Self::points`]; an out-of-range
    /// index panics. Use [`Self::all_nodes`] to include the node under the player.
    pub fn node(&self, i: usize) -> Option<&Node> {
        i.checked_sub(1).map(|n| &self.nodes[n])
    }

    /// Every node, including the one the player stands on.
    pub fn all_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().chain(&self.player_node)
    }

    /// Builds a reproducible arrangement over [`Context::terrain`], preserving `pins`.
    ///
    /// An unpinned call places new nodes and deals cards. A pinned call retains
    /// identities and preserved edges, as prepared by [`Self::regenerate`]. The
    /// first build omits teases and measures walking order from spawn; subsequent
    /// builds use that order and the same seed to select and protect tease pairs.
    ///
    /// If a required connection breaks a pair, add it to the disqualified set and
    /// retry so another pair can take its place. After at least one guarded build
    /// and at most `tease_retries` when positive, remaining recorded broken pairs
    /// are dropped. Returns placement or connectivity errors; neither visibility
    /// nor a loop at every node is guaranteed by a successful build.
    pub fn generate(
        context: Context,
        seed: u64,
        pins: Option<&Pins>,
    ) -> Result<Self, &'static str> {
        let first = Self::build(context, seed, pins, None, &BTreeSet::new())?;
        if context.settings.teases == 0 {
            return Ok(first);
        }
        let walked = first.walked_from_spawn(context);
        let mut disqualified = BTreeSet::new();
        let mut layout = first;
        for _ in 0..context.tuning.tease_retries.max(1) {
            layout = Self::build(context, seed, pins, Some(&walked), &disqualified)?;
            if layout.broke.is_empty() {
                return Ok(layout);
            }
            disqualified.extend(layout.broke.iter().copied());
        }
        let broke = layout.broke.clone();
        layout.tease_pairs.retain(|p| !broke.contains(p));
        Ok(layout)
    }

    /// Runs one placement and trail-building pass over the fixed terrain.
    ///
    /// `walked` supplies the tease-free first pass's arrival distances; `None`
    /// skips tease selection. Resets pad heights from the terrain, installs pins,
    /// then spans nodes, repairs ports, attempts loops, joins banks, and repairs
    /// ports again. [`Self::generate`] retries this pass with broken pairs banned.
    fn build(
        context: Context,
        seed: u64,
        pins: Option<&Pins>,
        walked: Option<&[f32]>,
        disqualified: &BTreeSet<(usize, usize)>,
    ) -> Result<Self, &'static str> {
        let Context {
            terrain, settings, ..
        } = context;
        let mesh = &terrain.mesh;
        let mut rng = StdRng::seed_from_u64(seed);
        let mut spawn = match pins {
            Some(pins) => pins.player,
            None => mesh
                .tile_at(
                    settings.columns as i32 / 2,
                    settings.rows as i32 / 2,
                    context.tuning.margin,
                )
                .ok_or("the middle of the board is not a tile")?,
        };
        if pins.is_none() && context.creeks_matter() && terrain.creeks.on(spawn) {
            // Spawn stands beside the creek, not in it.
            spawn = terrain
                .dry_tiles()
                .into_iter()
                .min_by(|&a, &b| {
                    let d = |t: usize| mesh.centers[t].distance(mesh.centers[spawn]);
                    d(a).total_cmp(&d(b))
                })
                .ok_or("no dry ground for spawn")?;
        }
        let mut nodes = place_nodes(context, spawn, pins, &mut rng)?;
        nodes.sort_by_key(|n| n.id);
        let crossing_tiles: Vec<usize> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Crossing)
            .map(|n| n.tile)
            .collect();
        let mut layout = Self {
            spawn,
            nodes,
            player_node: pins.and_then(|p| p.here),
            heights: terrain.heights.clone(),
            blocked: if context.creeks_matter() {
                terrain.creek_closed(&crossing_tiles)
            } else {
                vec![false; mesh.edges.len()]
            },
            trail: BTreeMap::new(),
            tease_pairs: Vec::new(),
            disqualified: disqualified.clone(),
            breaking: BTreeSet::new(),
            broke: BTreeSet::new(),
            ports: Vec::new(),
        };
        layout.level(context);
        if let Some(pins) = pins {
            layout.trail = pins
                .trails
                .iter()
                .map(|&e| (e, TrailKind::Pinned))
                .collect();
            layout.ports = layout.dead_ends(context);
        }
        if let Some(walked) = walked {
            layout.tease_pairs = layout.choose_teases(context, walked);
        }
        layout.span(context)?;
        layout.join_ports(context);
        layout.close_loops(
            context,
            Some(context.tuning.max_connector),
            &BTreeSet::new(),
            TrailKind::Loop,
        );
        let targets = layout.tease_pairs.iter().map(|&(_, t)| t).collect();
        layout.close_loops(context, None, &targets, TrailKind::Tree);
        if context.creeks_matter() {
            layout.join_banks(context);
        }
        layout.join_ports(context);
        Ok(layout)
    }

    /// Rebuilds on the same [`Terrain`](super::super::terrain::Terrain) around the supplied player tile.
    ///
    /// Partial regeneration pins nearby nodes and edges whose endpoints are
    /// both within `pin_radius`; Fear drops those proximity pins. Both preserve
    /// the player, the node underfoot, every crossing, and each other node's ID
    /// and card. Movable nodes prefer the displacement annulus, which placement
    /// can relax. Cut ends of preserved trails become ports for reconnection.
    ///
    /// Constructs [`Pins`] and calls [`Self::generate`]. `player` must be a valid
    /// tile on this terrain. Standing on a crossing returns an error; the caller
    /// decides whether to replace the old layout after successful generation.
    pub fn regenerate(
        &self,
        context: Context,
        seed: u64,
        player: usize,
        fear: bool,
    ) -> Result<Self, &'static str> {
        let mesh = &context.terrain.mesh;
        let near = |p: Vec2| p.distance(mesh.centers[player]) <= context.tuning.pin_radius * SQRT_3;
        let here = self.all_nodes().find(|n| n.tile == player).copied();
        if here.is_some_and(|n| n.kind == NodeKind::Crossing) {
            return Err("no regeneration on a crossing");
        }
        // Crossings are bridges and fords: physical, so they never move.
        let crossings = self
            .all_nodes()
            .filter(|n| n.kind == NodeKind::Crossing)
            .copied();
        let others = self
            .all_nodes()
            .filter(|n| Some(**n) != here && n.kind != NodeKind::Crossing)
            .copied();
        let pins = if fear {
            Pins {
                player,
                here,
                fixed: crossings.collect(),
                movable: others.collect(),
                trails: BTreeSet::new(),
            }
        } else {
            let (stay, go): (Vec<Node>, Vec<Node>) =
                others.partition(|n| near(mesh.centers[n.tile]));
            Pins {
                player,
                here,
                fixed: crossings.chain(stay).collect(),
                movable: go,
                trails: self
                    .trail
                    .keys()
                    .copied()
                    .filter(|&e| mesh.edges[e].iter().all(|&c| near(mesh.positions[c])))
                    .collect(),
            }
        };
        Self::generate(context, seed, Some(&pins))
    }

    /// Ranks viewer/POI pairs within sight range using first-pass arrival order.
    ///
    /// The target must be a POI reached after the viewer. Visible pairs rank
    /// first, then larger drops capped at `tease_top_m`, then shorter distance.
    /// Hidden pairs and non-dropping pairs can still be selected if they rank
    /// within the requested count. No point is used twice and disqualified pairs
    /// are excluded. Detour protection belongs to routing; held visible pairs
    /// are measured by [`Board`](super::super::data::Board).
    fn choose_teases(&self, context: Context, walked: &[f32]) -> Vec<(usize, usize)> {
        let tuning = context.tuning;
        let n = self.nodes.len() + 1;
        let top = tuning.edges(tuning.tease_top_m);
        let mut candidates: Vec<(bool, f32, f32, usize, usize)> = (1..n)
            .flat_map(|t| (1..n).map(move |v| (v, t)))
            .filter(|&(v, t)| {
                v != t
                    && !self.disqualified.contains(&(v, t))
                    && self.node(t).is_some_and(|n| n.kind == NodeKind::Poi)
                    && self.crow(context, v, t) <= tuning.sight * SQRT_3
                    && walked[v] < walked[t]
            })
            .map(|(v, t)| {
                let drop = self.pad_height(context, v) - self.pad_height(context, t);
                (
                    !self.visible(context, v, t),
                    -drop.min(top),
                    self.crow(context, v, t),
                    v,
                    t,
                )
            })
            .collect();
        candidates.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
                .then((a.3, a.4).cmp(&(b.3, b.4)))
        });
        let mut used = BTreeSet::new();
        let mut pairs = Vec::new();
        for (.., v, t) in candidates {
            if pairs.len() == context.settings.teases as usize {
                break;
            }
            if !used.contains(&v) && !used.contains(&t) {
                pairs.push((v, t));
                used.extend([v, t]);
            }
        }
        pairs
    }

    /// Measures centre-to-centre straight-line distance in edge lengths.
    ///
    /// `a` and `b` are valid routing-point indices from [`Self::points`], not tile
    /// indices. [`Self::broken_teases`] uses this to normalise detours.
    pub fn crow(&self, context: Context, a: usize, b: usize) -> f32 {
        let centers = &context.terrain.mesh.centers;
        let points = self.points();
        centers[points[a]].distance(centers[points[b]])
    }

    /// Returns the median of point `i`'s six levelled corner heights, in edge lengths.
    ///
    /// `i` follows [`Self::points`]. For six values the median averages the middle
    /// two. Used for eye and landmark heights in [`Self::visible`].
    pub fn pad_height(&self, context: Context, i: usize) -> f32 {
        median_corner(context.terrain.mesh.tile_corners[self.points()[i]].map(|c| self.heights[c]))
    }

    /// Tests ground occlusion from point `a`'s eye to point `b`'s landmark.
    ///
    /// `a` and `b` follow [`Self::points`]. Eye and landmark metre settings are
    /// converted to edge lengths, added to pad heights, and joined by a straight
    /// line. Intermediate samples are compared with the levelled triangular
    /// surface. Samples off the mesh do not block sight. This tests terrain only,
    /// not trees or sight range; pair selection applies the range separately.
    pub fn visible(&self, context: Context, a: usize, b: usize) -> bool {
        let (mesh, tuning) = (&context.terrain.mesh, context.tuning);
        let points = self.points();
        let (pa, pb) = (mesh.centers[points[a]], mesh.centers[points[b]]);
        let ha = self.pad_height(context, a) + tuning.edges(tuning.eye_m);
        let hb = self.pad_height(context, b) + tuning.edges(tuning.landmark_m);
        (3..57).all(|k| {
            let f = k as f32 / 59.;
            self.surface(context, pa + (pb - pa) * f)
                .is_none_or(|ground| ground <= ha + (hb - ha) * f + 1e-6)
        })
    }

    /// Height anywhere on the board, linear over the hex triangles (centres
    /// at the mean of their corners); `None` off the mesh.
    fn surface(&self, context: Context, point: Vec2) -> Option<f32> {
        let mesh = &context.terrain.mesh;
        let tile = mesh.tile_containing(point)?;
        let corners = mesh.tile_corners[tile];
        let centre = (
            mesh.centers[tile],
            corners.iter().map(|&c| self.heights[c]).sum::<f32>() / 6.,
        );
        (0..6).find_map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 6]);
            let weights = barycentric(point, centre.0, mesh.positions[a], mesh.positions[b])?;
            Some(weights.x * centre.1 + weights.y * self.heights[a] + weights.z * self.heights[b])
        })
    }

    /// Walking cost of each edge over the levelled heights; infinite off the
    /// board and across closed creek sides.
    pub(super) fn step_costs(&self, context: Context) -> Vec<f32> {
        context
            .terrain
            .step_costs(&self.heights, context.tuning.aversion)
            .into_iter()
            .zip(&self.blocked)
            .map(|(cost, &blocked)| if blocked { f32::INFINITY } else { cost })
            .collect()
    }

    /// Every pad's corners.
    pub(super) fn pad_corners(&self, context: Context) -> BTreeSet<usize> {
        let corners = &context.terrain.mesh.tile_corners;
        self.points().into_iter().flat_map(|t| corners[t]).collect()
    }

    /// Corners on a trail.
    pub(super) fn trail_corners(&self, context: Context) -> BTreeSet<usize> {
        let edges = &context.terrain.mesh.edges;
        self.trail.keys().flat_map(|&e| edges[e]).collect()
    }

    /// Runs multi-source Dijkstra over the terrain mesh's corner adjacency.
    ///
    /// `weights` must match mesh edges and contain nonnegative costs or infinity
    /// for impassable edges. Sources and distances use corner indices, not tiles
    /// or routing points. Searches the whole reachable graph; returned costs
    /// keep the weight units supplied by the calling routing or measurement pass.
    pub(super) fn search(&self, context: Context, weights: &[f32], sources: &[usize]) -> Search {
        let mesh = &context.terrain.mesh;
        let mut search = Search {
            distance: vec![f32::INFINITY; mesh.positions.len()],
            previous: vec![None; mesh.positions.len()],
        };
        let mut heap = BinaryHeap::new();
        for &s in sources {
            search.distance[s] = 0.;
            heap.push(Nearest(0., s));
        }
        while let Some(Nearest(d, v)) = heap.pop() {
            if d > search.distance[v] {
                continue;
            }
            for &(w, e) in &mesh.corner_edges[v] {
                let next = d + weights[e];
                if next < search.distance[w] {
                    search.distance[w] = next;
                    search.previous[w] = Some((v, e));
                    heap.push(Nearest(next, w));
                }
            }
        }
        search
    }

    /// The corners of point `i`'s pad.
    fn pad(&self, context: Context, i: usize) -> [usize; 6] {
        context.terrain.mesh.tile_corners[self.points()[i]]
    }

    /// Finds one shortest route from pad `a` to each reachable target corner.
    ///
    /// Starts at all six corners of routing point `a` and sorts targets by cost.
    /// Returned paths contain edge indices in target-to-source order. It does
    /// not enumerate alternative paths to the same target.
    fn routes(
        &self,
        context: Context,
        a: usize,
        weights: &[f32],
        targets: &[usize],
    ) -> Vec<Vec<usize>> {
        let search = self.search(context, weights, &self.pad(context, a));
        let mut reached: Vec<usize> = targets
            .iter()
            .copied()
            .filter(|&t| search.distance[t].is_finite())
            .collect();
        reached.sort_by(|&x, &y| search.distance[x].total_cmp(&search.distance[y]));
        reached.into_iter().map(|t| search.path(t)).collect()
    }

    /// Finds the cheapest route from one corner to any reachable target corner.
    ///
    /// Returns edge indices in target-to-source order, or `None` if no target is
    /// reachable. A start that is already a target gives an empty path. Bank and
    /// port repair use this with pass-specific forbidden edges.
    pub(super) fn corner_route(
        &self,
        context: Context,
        start: usize,
        weights: &[f32],
        targets: &BTreeSet<usize>,
    ) -> Option<Vec<usize>> {
        let search = self.search(context, weights, &[start]);
        targets
            .iter()
            .copied()
            .filter(|&t| search.distance[t].is_finite())
            .min_by(|&x, &y| search.distance[x].total_cmp(&search.distance[y]))
            .map(|t| search.path(t))
    }

    /// Penalises unused edges near a trail, encouraging new routes to merge into it.
    ///
    /// Breadth-first hops over corner adjacency define the `radius` neighbourhood.
    /// Edges with both endpoints off the trail receive `alongside` when either
    /// endpoint is nearby; edges stepping onto the trail keep multiplier one.
    /// This discourages shadowing a trail rather than prohibiting it.
    fn alongside(&self, context: Context, radius: u32) -> Vec<f32> {
        let mesh = &context.terrain.mesh;
        let on = self.trail_corners(context);
        if on.is_empty() {
            return vec![1.; mesh.edges.len()];
        }
        let mut hops = vec![u32::MAX; mesh.positions.len()];
        let mut queue: VecDeque<usize> = on.iter().copied().collect();
        on.iter().for_each(|&c| hops[c] = 0);
        while let Some(v) = queue.pop_front() {
            if hops[v] == radius {
                continue;
            }
            for &(w, _) in &mesh.corner_edges[v] {
                if hops[w] == u32::MAX {
                    hops[w] = hops[v] + 1;
                    queue.push_back(w);
                }
            }
        }
        mesh.edges
            .iter()
            .map(|&[u, v]| {
                let near = |c: usize| hops[c] != u32::MAX && !on.contains(&c);
                if (near(u) || near(v)) && !on.contains(&u) && !on.contains(&v) {
                    context.tuning.alongside
                } else {
                    1.
                }
            })
            .collect()
    }

    /// Builds weights for walking the laid network, including nearly free pad perimeters.
    ///
    /// Unused edges are impassable. Laid edges retain terrain cost and creek
    /// restrictions, then [`Self::plazas`] opens every pad perimeter. This differs
    /// from the full available-ground graph used to propose new trails.
    fn network(&self, context: Context) -> Vec<f32> {
        let costs = self.step_costs(context);
        let mut weights = vec![f32::INFINITY; costs.len()];
        self.trail.keys().for_each(|&e| weights[e] = costs[e]);
        self.plazas(context, &mut weights);
        weights
    }

    /// Returns physical walking weights for laid trails and pad perimeters.
    ///
    /// Other edges are impassable. Unlike [`Self::network`], pads retain their
    /// terrain costs: a player must actually walk between their entrances.
    /// The symmetric weights choose a route; directional travel time is
    /// calculated by [`Walk`](super::super::walking::data::Walk).
    pub fn walking_costs(&self, context: Context) -> Vec<f32> {
        let ground = context
            .terrain
            .step_costs(&self.heights, context.tuning.aversion);
        let mut weights = self.step_costs(context);
        for (e, weight) in weights.iter_mut().enumerate() {
            if !self.trail.contains_key(&e) {
                *weight = f32::INFINITY;
            }
        }
        for tile in self.points() {
            for edge in context.terrain.mesh.perimeter(tile) {
                weights[edge] = ground[edge];
            }
        }
        weights
    }

    /// Routes from corner `start` to `target` across trails and pads.
    ///
    /// Returns corners in walking order, excluding `start`. A request for the
    /// current corner succeeds with an empty route. When `known` is supplied,
    /// only those edges can be used, including on pads. Invalid or unreachable
    /// corners return `None`; Dijkstra's internal predecessor data stays private.
    pub fn walking_route(
        &self,
        context: Context,
        start: usize,
        target: usize,
        known: Option<&BTreeSet<usize>>,
    ) -> Option<Vec<usize>> {
        let mesh = &context.terrain.mesh;
        if start >= mesh.positions.len() || target >= mesh.positions.len() {
            return None;
        }
        let mut weights = self.walking_costs(context);
        if let Some(known) = known {
            for (e, weight) in weights.iter_mut().enumerate() {
                if !known.contains(&e) {
                    *weight = f32::INFINITY;
                }
            }
        }
        let search = self.search(context, &weights, &[start]);
        if !search.distance[target].is_finite() {
            return None;
        }
        let mut corner = start;
        Some(
            search
                .path(target)
                .into_iter()
                .rev()
                .map(|edge| {
                    let [a, b] = mesh.edges[edge];
                    corner = if a == corner { b } else { a };
                    corner
                })
                .collect(),
        )
    }

    /// Sets every pad's perimeter to the plaza weight.
    fn plazas(&self, context: Context, weights: &mut [f32]) {
        for tile in self.points() {
            context
                .terrain
                .mesh
                .perimeter(tile)
                .into_iter()
                .for_each(|e| weights[e] = PLAZA);
        }
    }

    /// Counts network edges from pad `a`, treating plaza-weight edges as nearly free.
    ///
    /// Converts finite network weights to one and tiny weights to [`PLAZA`]. Used
    /// for tease detours and arrival order, independent of terrain travel time.
    fn hops_from(&self, context: Context, a: usize) -> Search {
        let hops: Vec<f32> = self
            .network(context)
            .into_iter()
            .map(|w| {
                if w < 1e-3 {
                    PLAZA
                } else if w.is_finite() {
                    1.
                } else {
                    f32::INFINITY
                }
            })
            .collect();
        self.search(context, &hops, &self.pad(context, a))
    }

    /// Normalises shortest network hops by the open gap between two pad centres.
    ///
    /// The denominator is centre distance minus two pad radii, floored at one
    /// edge length. This detour ratio feeds [`Self::broken_teases`]; it is not the
    /// ratio of Tobler walking times.
    fn detour(&self, context: Context, a: usize, b: usize) -> f32 {
        let walked = self.hops_from(context, a).nearest(self.pad(context, b));
        walked / (self.crow(context, a, b) - 2.).max(1.)
    }

    /// Measures shortest trail-edge hops from spawn to every routing point.
    ///
    /// Entries follow [`Self::points`], including spawn at zero; unreachable pads
    /// get infinity. Plaza edges are nearly free. [`Self::generate`] uses the
    /// first pass's result to order tease viewers before targets.
    pub fn walked_from_spawn(&self, context: Context) -> Vec<f32> {
        let search = self.hops_from(context, 0);
        (0..self.nodes.len() + 1)
            .map(|i| search.nearest(self.pad(context, i)))
            .collect()
    }

    /// Finds selected tease pairs whose route detour or arrival order no longer holds.
    ///
    /// A pair breaks when its detour falls below `tease_detour`, or its target
    /// is reachable at no greater hop distance from spawn than its viewer.
    /// Pairs use [`Self::points`] indices. Visibility is a separate test in
    /// [`Self::visible`] and in the Board's held-tease measurement.
    pub fn broken_teases(&self, context: Context) -> BTreeSet<(usize, usize)> {
        if self.tease_pairs.is_empty() {
            return BTreeSet::new();
        }
        let walked = self.walked_from_spawn(context);
        self.tease_pairs
            .iter()
            .copied()
            .filter(|&(v, t)| {
                self.detour(context, v, t) < context.tuning.tease_detour
                    || (walked[t] <= walked[v] && walked[t].is_finite())
            })
            .collect()
    }

    /// Accepts the first candidate that does not increase the count of broken teases.
    ///
    /// Adds only previously absent edges, checks [`Self::broken_teases`], and
    /// rolls those edges back if the guarded candidate fails. It compares counts,
    /// not pair-set equality. Existing edges and their kinds are retained.
    /// With `guard = false`, accepts the first route and records newly broken
    /// pairs in [`Self::broke`] and responsible new edges in [`Self::breaking`].
    /// Returns false if every candidate is rejected or the list is empty.
    pub(super) fn lay(
        &mut self,
        context: Context,
        candidates: &[Vec<usize>],
        kind: TrailKind,
        guard: bool,
    ) -> bool {
        let before = self.broken_teases(context);
        for path in candidates {
            let new: Vec<usize> = path
                .iter()
                .copied()
                .filter(|e| !self.trail.contains_key(e))
                .collect();
            new.iter().for_each(|&e| {
                self.trail.insert(e, kind);
            });
            let after = self.broken_teases(context);
            let breaks = after.len() > before.len();
            if !guard || !breaks {
                if breaks {
                    self.breaking.extend(new);
                    self.broke.extend(after.difference(&before));
                }
                return true;
            }
            new.iter().for_each(|e| {
                self.trail.remove(e);
            });
        }
        false
    }

    /// Builds a greedy spanner, then connects any nodes still unreachable from spawn.
    ///
    /// Scans node pairs twice in direct terrain-cost order, proposing a route
    /// when the network walk exceeds `stretch` times that cost. Route weights
    /// discount existing trails and penalise alongside edges. Direct tease-pair
    /// connections are skipped and proposals must pass [`Self::lay`]'s guard.
    ///
    /// The connection pass tries guarded routes for unreachable nodes, falling
    /// back to an unguarded route if necessary. Such breaks trigger retries in
    /// [`Self::generate`]. Returns an error when no connecting ground route exists
    /// or the pass fails to connect all nodes.
    pub(super) fn span(&mut self, context: Context) -> Result<(), &'static str> {
        let tuning = context.tuning;
        let costs = self.step_costs(context);
        let n = self.nodes.len() + 1;
        let direct: Vec<Vec<f32>> = (0..n)
            .map(|i| {
                let search = self.search(context, &costs, &self.pad(context, i));
                (0..n)
                    .map(|j| search.nearest(self.pad(context, j)))
                    .collect()
            })
            .collect();
        let teases: BTreeSet<(usize, usize)> = self
            .tease_pairs
            .iter()
            .flat_map(|&(v, t)| [(v, t), (t, v)])
            .collect();
        let mut pairs: Vec<(usize, usize)> = (0..n)
            .flat_map(|a| (a + 1..n).map(move |b| (a, b)))
            .collect();
        pairs.sort_by(|x, y| direct[x.0][x.1].total_cmp(&direct[y.0][y.1]).then(x.cmp(y)));
        // Twice: a pair the guard rejected early (say, a tease target before
        // its viewer) may pass later.
        for &(a, b) in pairs.iter().chain(&pairs) {
            if teases.contains(&(a, b)) {
                continue;
            }
            let walked = self.search(context, &self.network(context), &self.pad(context, a));
            if walked.nearest(self.pad(context, b)) <= tuning.stretch * direct[a][b] {
                continue;
            }
            let along = self.alongside(context, 1);
            let weights: Vec<f32> = (0..costs.len())
                .map(|e| {
                    costs[e]
                        * along[e]
                        * if self.trail.contains_key(&e) {
                            tuning.reuse
                        } else {
                            1.
                        }
                })
                .collect();
            let route = self.routes(context, a, &weights, &self.pad(context, b));
            self.lay(context, &route[..route.len().min(1)], TrailKind::Span, true);
        }
        // Each pass joins at least one node, so n passes are enough.
        for _ in 0..n {
            let walked = self.hops_from(context, 0);
            let cut: Vec<usize> = (0..n)
                .filter(|&i| !walked.nearest(self.pad(context, i)).is_finite())
                .collect();
            if cut.is_empty() {
                return Ok(());
            }
            let nearest_reached = |i: usize| {
                (0..n)
                    .filter(|j| !cut.contains(j))
                    .map(|j| direct[i][j])
                    .fold(f32::INFINITY, f32::min)
            };
            let a = cut
                .iter()
                .copied()
                .min_by(|&x, &y| nearest_reached(x).total_cmp(&nearest_reached(y)))
                .expect("something is cut off");
            let reached: Vec<usize> = (0..n)
                .filter(|j| !cut.contains(j))
                .flat_map(|j| self.pad(context, j))
                .collect();
            let candidates = self.routes(context, a, &costs, &reached);
            if candidates.is_empty() {
                return Err("a node is cut off: no route to the rest");
            }
            if !self.lay(
                context,
                &candidates[..candidates.len().min(CANDIDATES)],
                TrailKind::Forced,
                true,
            ) {
                self.lay(context, &candidates[..1], TrailKind::Forced, false);
            }
        }
        Err("the connection pass made no progress")
    }

    /// Collapses pad corners to routing-point vertices and removes pad-internal trail edges.
    fn contracted(&self, context: Context) -> Contracted {
        let mesh = &context.terrain.mesh;
        let corners = mesh.positions.len();
        let label: BTreeMap<usize, usize> = self
            .points()
            .into_iter()
            .enumerate()
            .flat_map(|(i, t)| mesh.tile_corners[t].map(|c| (c, corners + i)))
            .collect();
        let mut adjacent: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for &e in self.trail.keys() {
            let [u, v] = mesh.edges[e].map(|c| label.get(&c).copied().unwrap_or(c));
            if u != v {
                adjacent.entry(u).or_default().insert(v);
                adjacent.entry(v).or_default().insert(u);
            }
        }
        Contracted { corners, adjacent }
    }

    /// Attempts an alternate trail for each unspared pad not already on a useful loop.
    ///
    /// [`Contracted::on_loop`] requires two internally vertex-disjoint paths to
    /// another pad. Candidate routes cannot use existing trail edges or the
    /// node's own spur, and keep a two-corner clearance from that spur. Targets
    /// are other trail corners or pads. Searches do not explicitly stop at the
    /// first trail corner encountered on the way to a selected target.
    ///
    /// With a cap, candidates must fit both its edge limit and `cheap_loop` times
    /// the nearest existing link's cost; spawn has no cost limit. Without a cap,
    /// the no-tree pass removes both limits. Both passes require minimum round
    /// length and the tease guard, so a second exit is attempted, not guaranteed.
    /// Points listed in `spare` are skipped.
    pub(super) fn close_loops(
        &mut self,
        context: Context,
        cap: Option<u32>,
        spare: &BTreeSet<usize>,
        kind: TrailKind,
    ) {
        let (mesh, tuning) = (&context.terrain.mesh, context.tuning);
        let costs = self.step_costs(context);
        let pads = self.pad_corners(context);
        let corners = mesh.positions.len();
        let unit = vec![1.; mesh.edges.len()];
        for i in 0..self.nodes.len() + 1 {
            let graph = self.contracted(context);
            if spare.contains(&i) || graph.on_loop(i) {
                continue;
            }
            let mut budget = f32::INFINITY;
            if i != 0 && cap.is_some() {
                let walked = self.search(context, &self.network(context), &self.pad(context, i));
                let link = (0..self.nodes.len() + 1)
                    .filter(|&j| j != i)
                    .map(|j| walked.nearest(self.pad(context, j)))
                    .fold(f32::INFINITY, f32::min);
                budget = tuning.cheap_loop * link;
            }
            let back = graph.hops(graph.pad(i));
            let occupied: BTreeSet<usize> = self
                .trail_corners(context)
                .difference(&pads)
                .copied()
                .collect();
            let along = self.alongside(context, 2);
            let mut weights: Vec<f32> = costs.iter().zip(&along).map(|(c, a)| c * a).collect();
            self.trail.keys().for_each(|&e| weights[e] = f32::INFINITY);
            // Stop at the first trail met, anywhere but this node's own spur.
            // Keep 2 corners clear of the spur, so the loop isn't a thin
            // finger beside it.
            let spur = graph.spur(i);
            let mut clear = spur.clone();
            if !spur.is_empty() {
                let sources: Vec<usize> = spur.iter().copied().collect();
                let hops = self.search(context, &unit, &sources);
                let own = self.pad(context, i);
                clear.extend((0..corners).filter(|&c| hops.distance[c] <= 2. && !own.contains(&c)));
            }
            for (e, &[u, v]) in mesh.edges.iter().enumerate() {
                if clear.contains(&u) || clear.contains(&v) {
                    weights[e] = f32::INFINITY;
                }
            }
            let label: BTreeMap<usize, usize> = (0..self.nodes.len() + 1)
                .filter(|&j| j != i)
                .flat_map(|j| self.pad(context, j).map(|c| (c, graph.pad(j))))
                .collect();
            let ends: Vec<usize> = occupied
                .iter()
                .chain(label.keys())
                .copied()
                .filter(|c| !clear.contains(c))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let own = self.pad(context, i);
            let options: Vec<Vec<usize>> = self
                .routes(context, i, &weights, &ends)
                .into_iter()
                .filter(|path| {
                    let Some(&first) = path.first() else {
                        return false;
                    };
                    let end =
                        path_end(mesh.edges[first], path.get(1).map(|&e| mesh.edges[e]), &own);
                    let round = back
                        .get(&label.get(&end).copied().unwrap_or(end))
                        .map_or(f32::INFINITY, |&h| h as f32);
                    path.iter().map(|&e| weights[e]).sum::<f32>() <= budget
                        && cap.is_none_or(|cap| path.len() as u32 <= cap)
                        && round + path.len() as f32 >= tuning.min_loop as f32
                })
                .take(CANDIDATES)
                .collect();
            self.lay(context, &options, kind, true);
        }
    }

    /// Each trail corner's number of trail edges.
    fn degrees(&self, context: Context) -> BTreeMap<usize, usize> {
        let mut degrees = BTreeMap::new();
        for &e in self.trail.keys() {
            for c in context.terrain.mesh.edges[e] {
                *degrees.entry(c).or_default() += 1;
            }
        }
        degrees
    }

    /// Lists trail corners with degree one that lie outside every pad.
    ///
    /// Pad corners can legitimately end at a landmark. Pinned trails are scanned
    /// with this rule to identify [`Self::ports`] before the new network is built.
    pub fn dead_ends(&self, context: Context) -> Vec<usize> {
        let pads = self.pad_corners(context);
        self.degrees(context)
            .into_iter()
            .filter(|&(c, degree)| degree == 1 && !pads.contains(&c))
            .map(|(c, _)| c)
            .collect()
    }

    /// Attempts to reconnect each preserved-trail port that is still a dead end.
    ///
    /// Excludes existing edges and the port's own stub back toward a junction or
    /// pad. Searches for the cheapest other trail corner or pad, first guarded
    /// and then unguarded if a tease blocks the repair. Ports with no route stay
    /// dead ends; this pass does not return a connectivity error.
    pub(super) fn join_ports(&mut self, context: Context) {
        let mesh = &context.terrain.mesh;
        let costs = self.step_costs(context);
        let pads = self.pad_corners(context);
        for port in self.ports.clone() {
            if !self.dead_ends(context).contains(&port) {
                continue;
            }
            let mut neighbours: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for &e in self.trail.keys() {
                let [u, v] = mesh.edges[e];
                neighbours.entry(u).or_default().push(v);
                neighbours.entry(v).or_default().push(u);
            }
            let mut stub = BTreeSet::from([port]);
            let mut frontier = vec![port];
            while let Some(v) = frontier.pop() {
                for &w in neighbours.get(&v).into_iter().flatten() {
                    if !stub.contains(&w) && !pads.contains(&w) && neighbours[&w].len() <= 2 {
                        stub.insert(w);
                        frontier.push(w);
                    }
                }
            }
            stub.remove(&port);
            let along = self.alongside(context, 2);
            let mut weights: Vec<f32> = costs.iter().zip(&along).map(|(c, a)| c * a).collect();
            self.trail.keys().for_each(|&e| weights[e] = f32::INFINITY);
            for (e, &[u, v]) in mesh.edges.iter().enumerate() {
                if stub.contains(&u) || stub.contains(&v) {
                    weights[e] = f32::INFINITY;
                }
            }
            let targets: BTreeSet<usize> = self
                .trail_corners(context)
                .union(&pads)
                .copied()
                .filter(|c| !stub.contains(c) && *c != port)
                .collect();
            if let Some(path) = self.corner_route(context, port, &weights, &targets) {
                let path = [path];
                if !self.lay(context, &path, TrailKind::Port, true) {
                    self.lay(context, &path, TrailKind::Port, false);
                }
            }
        }
    }

    /// Levels pad ground in the model-layer copy of terrain corner heights.
    ///
    /// For each point, chooses the median of its current six corners, clamps
    /// nearby heights to `target ± grade × distance`, and fades the adjustment
    /// between one and two edge lengths beyond the unit pad radius. Pads are
    /// processed in point order. Only [`Self::heights`] changes; a later build
    /// starts from original terrain heights so removed pads leave no permanent cut.
    pub(super) fn level(&mut self, context: Context) {
        let mesh = &context.terrain.mesh;
        let grade = context.tuning.grade;
        for tile in self.points() {
            let target = median_corner(mesh.tile_corners[tile].map(|c| self.heights[c]));
            for (h, &p) in self.heights.iter_mut().zip(&mesh.positions) {
                let d = (p.distance(mesh.centers[tile]) - 1.).max(0.);
                let graded = h.clamp(target - grade * d, target + grade * d);
                let x = (d - 1.).clamp(0., 1.);
                let w = 1. - x * x * (3. - 2. * x);
                *h += w * (graded - *h);
            }
        }
    }

    /// Estimates walk time from true-scale heights using the full Tobler speed curve.
    ///
    /// Assumes trail lengths shorten by √3/2 when smoothed; no geometry is
    /// actually smoothed here. Edge slope uses that shortened horizontal length,
    /// and time averages walking the edge in both directions. Walk-all sums
    /// each laid edge once; shortest-path minutes to nodes use zero-cost pad
    /// perimeters. This is a network estimate, not a tour that includes retracing.
    pub fn pace(&self, context: Context) -> Pace {
        let (mesh, tuning) = (&context.terrain.mesh, context.tuning);
        let km = tuning.edge_m * SMOOTHING / 1000.;
        let tobler = |slope: f32| 6. * (-3.5 * (slope + 0.05).abs()).exp();
        let mut minutes = vec![f32::INFINITY; mesh.edges.len()];
        let mut walk_all = 0.;
        for &e in self.trail.keys() {
            let [a, b] = mesh.edges[e];
            let slope = (self.heights[b] - self.heights[a]) / SMOOTHING;
            minutes[e] = km * 60. * (1. / tobler(slope) + 1. / tobler(-slope)) / 2.;
            walk_all += minutes[e];
        }
        for tile in self.points() {
            mesh.perimeter(tile)
                .into_iter()
                .for_each(|e| minutes[e] = 0.);
        }
        let search = self.search(context, &minutes, &self.pad(context, 0));
        let times: Vec<f32> = (1..self.nodes.len() + 1)
            .map(|i| search.nearest(self.pad(context, i)))
            .collect();
        Pace {
            walk_all_min: walk_all,
            trail_km: self.trail.len() as f32 * km,
            farthest_min: times
                .iter()
                .copied()
                .filter(|t| t.is_finite())
                .fold(0., f32::max),
            unreachable: times.iter().filter(|t| !t.is_finite()).count(),
        }
    }

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
                mesh.perimeter(tile)
                    .into_iter()
                    .for_each(|e| weights[e] = f32::INFINITY);
                let targets: BTreeSet<usize> = on_trail
                    .union(&pads)
                    .copied()
                    .filter(|c| !pad.contains(c))
                    .collect();
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
    /// the laid network after [`Self::join_banks`], rather than the potential
    /// ground connectivity used to validate crossing sites; [`Board`](super::super::data::Board) displays it.
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

/// Places typed nodes, or relocates existing ones supplied through [`Pins`].
///
/// Fresh placement chooses crossings, computes reachable dry tiles, spreads
/// POIs by suitability and water proximity and benches by bench score, then
/// drops unusable crossings and deals the POI deck. Pinned placement retains
/// fixed nodes and relocates movable values without redealing their cards.
/// [`place`](sites::place) relaxes spacing and displacement preferences if needed.
/// Creek exclusions apply when crossings are enabled; zero disables them.
fn place_nodes(
    context: Context,
    spawn: usize,
    pins: Option<&Pins>,
    rng: &mut StdRng,
) -> Result<Vec<Node>, &'static str> {
    let Context {
        terrain,
        settings,
        tuning,
    } = context;
    let mesh = &terrain.mesh;
    let mut buildable: Vec<f32> = terrain
        .suitability
        .iter()
        .map(|s| s.powf(settings.spread))
        .collect();
    let mut bench = terrain.bench_score();
    let mut crossings = Vec::new();
    let mut forbid = None;
    if context.creeks_matter() {
        // Points of interest gather near water; nothing but a crossing sits
        // on a creek.
        let creek: Vec<usize> = (0..mesh.tiles.len())
            .filter(|&t| terrain.creeks.on(t))
            .collect();
        for (t, score) in buildable.iter_mut().enumerate() {
            let near = creek
                .iter()
                .map(|&c| mesh.centers[t].distance(mesh.centers[c]) / SQRT_3)
                .fold(f32::INFINITY, f32::min);
            let dry = if terrain.creeks.on(t) { 0. } else { 1. };
            *score *= (1. + settings.water_boost * (-near / 2.).exp()) * dry;
        }
        crossings = match pins {
            None => crossings::trunk_sites(context, spawn)
                .into_iter()
                .zip(FIRST_CROSSING_ID..)
                .map(|(tile, id)| Node {
                    id,
                    tile,
                    kind: NodeKind::Crossing,
                    card: None,
                })
                .collect(),
            Some(pins) => pins
                .fixed
                .iter()
                .filter(|n| n.kind == NodeKind::Crossing)
                .copied()
                .collect(),
        };
        if !crossings.is_empty() {
            bench
                .iter_mut()
                .enumerate()
                .filter(|(t, _)| terrain.creeks.on(*t))
                .for_each(|(_, s)| *s = 0.);
        }
        let tiles: Vec<usize> = crossings.iter().map(|n: &Node| n.tile).collect();
        forbid = Some(terrain.cut_off(&tiles, spawn));
    }
    let bench_room = |nodes: &[Node]| -> Vec<bool> {
        (0..mesh.tiles.len())
            .map(|t| {
                nodes.iter().filter(|n| n.kind == NodeKind::Bench).all(|n| {
                    mesh.centers[t].distance(mesh.centers[n.tile]) / SQRT_3 >= tuning.min_spacing
                })
            })
            .collect()
    };
    let score = |kind: NodeKind| {
        if kind == NodeKind::Bench {
            &bench
        } else {
            &buildable
        }
    };
    let place = |nodes: &[Node], node: Node, near: Option<(usize, Vec2)>, rng: &mut StdRng| {
        let room = (node.kind == NodeKind::Bench).then(|| bench_room(nodes));
        let taken: Vec<usize> = std::iter::once(spawn)
            .chain(nodes.iter().map(|n| n.tile))
            .collect();
        let constraints = sites::Constraints {
            forbid: forbid.as_deref(),
            room: room.as_deref(),
            near,
        };
        sites::place(
            mesh,
            &taken,
            score(node.kind),
            tuning.min_spacing,
            &constraints,
            rng,
        )
        .map(|tile| Node { tile, ..node })
        .ok_or("no room left for a node")
    };
    let Some(pins) = pins else {
        let mut nodes = crossings;
        let wanted = (0..settings.pois + settings.benches).map(|id| Node {
            id,
            tile: 0,
            kind: if id < settings.pois {
                NodeKind::Poi
            } else {
                NodeKind::Bench
            },
            card: None,
        });
        for node in wanted {
            let placed = place(&nodes, node, None, rng)?;
            nodes.push(placed);
        }
        if nodes.iter().any(|n| n.kind == NodeKind::Crossing) {
            nodes = crossings::drop_dead_crossings(terrain, nodes, spawn);
        }
        let mut pois: Vec<usize> = (0..nodes.len())
            .filter(|&i| nodes[i].kind == NodeKind::Poi)
            .collect();
        pois.shuffle(rng);
        for (&i, card) in pois.iter().zip(DECK) {
            nodes[i].card = Some(card);
        }
        return Ok(nodes);
    };
    let mut nodes = pins.fixed.clone();
    let mut order: Vec<usize> = (0..pins.movable.len()).collect();
    order.shuffle(rng);
    for i in order {
        let node = pins.movable[i];
        let placed = place(&nodes, node, Some((node.tile, tuning.displacement)), rng)?;
        nodes.push(placed);
    }
    Ok(nodes)
}

/// The median of six corner heights: the mean of the middle two.
fn median_corner(mut corners: [f32; 6]) -> f32 {
    corners.sort_by(f32::total_cmp);
    (corners[2] + corners[3]) / 2.
}

/// Barycentric weights of `p` in triangle `a`, `b`, `c`, if it's inside.
fn barycentric(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Option<Vec3> {
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let denominator = v0.perp_dot(v1);
    if denominator.abs() < f32::EPSILON {
        return None;
    }
    let v = v2.perp_dot(v1) / denominator;
    let w = v0.perp_dot(v2) / denominator;
    let u = 1. - v - w;
    (u >= -1e-4 && v >= -1e-4 && w >= -1e-4).then_some(Vec3::new(u, v, w))
}
