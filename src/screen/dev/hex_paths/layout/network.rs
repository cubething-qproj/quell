//! Corner-graph routing and trail construction for [`Layout`].
//!
//! Terrain step weights plus creek barriers feed Dijkstra [`Search`] results.
//! [`Layout::span`] compares available ground routes with the existing trail
//! network, favouring reuse and penalising parallel trails. [`Layout::lay`]
//! tentatively inserts candidate edges and rolls back guarded routes that
//! increase the number of broken tease pairs. Required repairs record breaks
//! so [`Layout::generate`] can disqualify pairs and rebuild.
//!
//! [`Contracted`] collapses each pad to one vertex for loop tests: a useful loop
//! needs two internally vertex-disjoint routes to another pad, rather than a
//! cycle around its own perimeter. Loop attempts exclude the node's spur;
//! port repair reconnects cut ends of preserved trails. [`Layout::level`] works
//! on a copy of terrain heights, and [`Layout::pace`] estimates minutes using
//! unexaggerated heights and an assumed smoothing-length correction.

use super::{Context, Layout, TrailKind};
use crate::screen::dev::hex_paths::terrain::SQRT_3;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
};

/// Near-zero perimeter weight used to model pads as freely traversable plazas.
const PLAZA: f32 = 1e-6;
/// Assumed length reduction from honeycomb zigzags, used only in pace estimates.
/// The preview still draws the original edges; no spline is built here.
const SMOOTHING: f32 = SQRT_3 / 2.;
/// Maximum candidate routes tried by connection and loop passes for one addition.
const CANDIDATES: usize = 40;

/// Dijkstra distances and predecessor edges for one [`Layout::search`] call.
///
/// Entries follow the terrain mesh's corner order. Several source corners
/// can start at zero, allowing a pad to be entered anywhere. [`Self::path`]
/// reconstructs edge indices from the target back toward a source; distances
/// retain the caller's cost units, whether terrain weights, hops, or minutes.
pub struct Search {
    /// Shortest cost per corner; zero at sources and infinity at unreachable corners.
    pub distance: Vec<f32>,
    /// The corner and edge each corner was reached by.
    previous: Vec<Option<(usize, usize)>>,
}

impl Search {
    /// Follows predecessor edges from `target` back to any search source.
    ///
    /// Returns edges in target-to-source order. An unreached target has no
    /// predecessor and returns an empty path, so callers check reachability first.
    fn path(&self, mut target: usize) -> Vec<usize> {
        let mut path = Vec::new();
        while let Some((before, edge)) = self.previous[target] {
            path.push(edge);
            target = before;
        }
        path
    }

    /// The shortest distance to any of `corners`.
    fn nearest(&self, corners: impl IntoIterator<Item = usize>) -> f32 {
        corners
            .into_iter()
            .map(|c| self.distance[c])
            .fold(f32::INFINITY, f32::min)
    }
}

/// A heap entry for Dijkstra, nearest first.
#[derive(PartialEq)]
struct Nearest(f32, usize);

impl Eq for Nearest {}

impl Ord for Nearest {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0).then(other.1.cmp(&self.1))
    }
}

impl PartialOrd for Nearest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The trail graph used to distinguish meaningful alternate routes from pad cycles.
///
/// [`Layout::contracted`] maps every corner on a pad to `corners + point_index`,
/// leaves other corners unchanged, and coalesces neighbours into sets.
/// [`Self::on_loop`] finds biconnected blocks shared with another pad;
/// [`Self::spur`] identifies the existing exit run to avoid during loop routing.
struct Contracted {
    corners: usize,
    adjacent: BTreeMap<usize, BTreeSet<usize>>,
}

impl Contracted {
    /// Maps a layout routing-point index to its contracted pad vertex.
    fn pad(&self, i: usize) -> usize {
        self.corners + i
    }

    /// Distinguishes contracted pad vertices from ordinary corner vertices.
    fn is_pad(&self, v: usize) -> bool {
        v >= self.corners
    }

    /// Iterates the unique trail neighbours of a vertex, or none if it is absent.
    fn neighbours(&self, v: usize) -> impl Iterator<Item = usize> + '_ {
        self.adjacent.get(&v).into_iter().flatten().copied()
    }

    /// Hops from `start` to every vertex it reaches.
    fn hops(&self, start: usize) -> BTreeMap<usize, u32> {
        let mut hops = BTreeMap::from([(start, 0)]);
        let mut queue = VecDeque::from([start]);
        while let Some(v) = queue.pop_front() {
            for w in self.neighbours(v) {
                if !hops.contains_key(&w) {
                    hops.insert(w, hops[&v] + 1);
                    queue.push_back(w);
                }
            }
        }
        hops
    }

    /// Tests whether pad `i` shares a biconnected block with another pad.
    ///
    /// A block of at least three vertices supplies two connecting paths with no
    /// shared intermediate vertex. Two pad exits that merge at a chokepoint do
    /// not suffice, nor does a cycle containing only one pad. Used by
    /// [`Layout::close_loops`] to decide which nodes need an alternate route.
    fn on_loop(&self, i: usize) -> bool {
        let me = self.pad(i);
        if !self.adjacent.contains_key(&me) {
            return false;
        }
        self.blocks().iter().any(|block| {
            block.len() >= 3 && block.contains(&me) && block.iter().any(|&v| v != me && self.is_pad(v))
        })
    }

    /// Finds biconnected vertex blocks with Tarjan's depth-first edge-stack algorithm.
    ///
    /// Discovery depth and the lowest ancestor reachable below each vertex expose
    /// articulation boundaries. At each boundary, pop the traversed edges into
    /// one vertex set. Runs over all graph components; bridge blocks can contain
    /// only two vertices and do not qualify as useful loops.
    fn blocks(&self) -> Vec<BTreeSet<usize>> {
        /// Depth-first traversal state and unassigned edges for biconnected blocks.
        struct Walk<'a> {
            graph: &'a Contracted,
            depth: BTreeMap<usize, u32>,
            low: BTreeMap<usize, u32>,
            edges: Vec<(usize, usize)>,
            blocks: Vec<BTreeSet<usize>>,
        }
        impl Walk<'_> {
            /// Visits a vertex, updates ancestor reachability, and emits blocks at articulation boundaries.
            fn visit(&mut self, v: usize, parent: Option<usize>, depth: u32) {
                self.depth.insert(v, depth);
                self.low.insert(v, depth);
                for w in self.graph.neighbours(v).collect::<Vec<_>>() {
                    if Some(w) == parent {
                        continue;
                    }
                    match self.depth.get(&w).copied() {
                        None => {
                            self.edges.push((v, w));
                            self.visit(w, Some(v), depth + 1);
                            let low = self.low[&w].min(self.low[&v]);
                            self.low.insert(v, low);
                            if self.low[&w] >= depth {
                                let mut block = BTreeSet::new();
                                while let Some((a, b)) = self.edges.pop() {
                                    block.extend([a, b]);
                                    if (a, b) == (v, w) {
                                        break;
                                    }
                                }
                                self.blocks.push(block);
                            }
                        }
                        Some(d) if d < depth => {
                            self.edges.push((v, w));
                            let low = self.low[&v].min(d);
                            self.low.insert(v, low);
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        let mut walk = Walk {
            graph: self,
            depth: BTreeMap::new(),
            low: BTreeMap::new(),
            edges: Vec::new(),
            blocks: Vec::new(),
        };
        for &v in self.adjacent.keys() {
            if !walk.depth.contains_key(&v) {
                walk.visit(v, None, 0);
            }
        }
        walk.blocks
    }

    /// Trail corners on pad `i`'s way out, up to (not including) the first
    /// junction or other pad.
    fn spur(&self, i: usize) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        let mut frontier: Vec<usize> = self.neighbours(self.pad(i)).collect();
        while let Some(v) = frontier.pop() {
            if out.contains(&v) || self.is_pad(v) || self.neighbours(v).count() > 2 {
                continue;
            }
            out.insert(v);
            frontier.extend(self.neighbours(v));
        }
        out
    }
}

impl Layout {
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
    fn routes(&self, context: Context, a: usize, weights: &[f32], targets: &[usize]) -> Vec<Vec<usize>> {
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

    /// Sets every pad's perimeter to the plaza weight.
    fn plazas(&self, context: Context, weights: &mut [f32]) {
        for tile in self.points() {
            context.terrain.mesh.perimeter(tile).into_iter().for_each(|e| weights[e] = PLAZA);
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
            .map(|w| if w < 1e-3 { PLAZA } else if w.is_finite() { 1. } else { f32::INFINITY })
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
    pub(super) fn lay(&mut self, context: Context, candidates: &[Vec<usize>], kind: TrailKind, guard: bool) -> bool {
        let before = self.broken_teases(context);
        for path in candidates {
            let new: Vec<usize> = path.iter().copied().filter(|e| !self.trail.contains_key(e)).collect();
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
                (0..n).map(|j| search.nearest(self.pad(context, j))).collect()
            })
            .collect();
        let teases: BTreeSet<(usize, usize)> = self
            .tease_pairs
            .iter()
            .flat_map(|&(v, t)| [(v, t), (t, v)])
            .collect();
        let mut pairs: Vec<(usize, usize)> = (0..n).flat_map(|a| (a + 1..n).map(move |b| (a, b))).collect();
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
                .map(|e| costs[e] * along[e] * if self.trail.contains_key(&e) { tuning.reuse } else { 1. })
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
            if !self.lay(context, &candidates[..candidates.len().min(CANDIDATES)], TrailKind::Forced, true) {
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
            let occupied: BTreeSet<usize> = self.trail_corners(context).difference(&pads).copied().collect();
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
                    let end = path_end(mesh.edges[first], path.get(1).map(|&e| mesh.edges[e]), &own);
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
            let target = super::median_corner(mesh.tile_corners[tile].map(|c| self.heights[c]));
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
            mesh.perimeter(tile).into_iter().for_each(|e| minutes[e] = 0.);
        }
        let search = self.search(context, &minutes, &self.pad(context, 0));
        let times: Vec<f32> = (1..self.nodes.len() + 1)
            .map(|i| search.nearest(self.pad(context, i)))
            .collect();
        Pace {
            walk_all_min: walk_all,
            trail_km: self.trail.len() as f32 * km,
            farthest_min: times.iter().copied().filter(|t| t.is_finite()).fold(0., f32::max),
            unreachable: times.iter().filter(|t| !t.is_finite()).count(),
        }
    }
}

/// Travel estimates for a [`Layout`], computed by [`Layout::pace`].
///
/// Uses unexaggerated heights, an assumed smoothing-length reduction, and
/// the mean Tobler travel time in both edge directions. [`Board`](super::super::data::Board)
/// stores these statistics for display; no player movement is simulated.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pace {
    /// Sum of times for every laid trail edge once, without any retracing.
    pub walk_all_min: f32,
    /// Total assumed smoothed length of all laid trail edges, in kilometres.
    pub trail_km: f32,
    /// Greatest finite shortest-path time from spawn to another node, in minutes.
    pub farthest_min: f32,
    /// Other nodes with no finite network route from spawn; spawn is excluded.
    pub unreachable: usize,
}

/// The corner a route from a pad ends on (routes run from the target end):
/// the end of its first edge not shared with the next, or not on the pad.
fn path_end(first: [usize; 2], second: Option<[usize; 2]>, pad: &[usize; 6]) -> usize {
    match second {
        Some(next) => first.into_iter().find(|c| !next.contains(c)),
        None => first.into_iter().find(|c| !pad.contains(c)),
    }
    .unwrap_or(first[0])
}
