//! Shortest-path results, graph contraction, and pace data used by layout methods.
//!
//! [`Search`] stores Dijkstra distances and predecessor edges built by
//! [`Layout::search`](super::Layout::search). [`Contracted`] collapses each pad
//! to one vertex to test independent onward paths and find spur clearance.
//! [`Pace`] holds travel estimates computed by [`Layout::pace`](super::Layout::pace).
//! Routing and network construction remain methods beside the layout definition.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, VecDeque},
};

/// Dijkstra distances and predecessor edges for one [`Layout::search`](super::Layout::search) call.
///
/// Entries follow the terrain mesh's corner order. Several source corners
/// can start at zero, allowing a pad to be entered anywhere. [`Self::path`]
/// reconstructs edge indices from the target back toward a source; distances
/// retain the caller's cost units, whether terrain weights, hops, or minutes.
pub(super) struct Search {
    /// Shortest cost per corner; zero at sources and infinity at unreachable corners.
    pub distance: Vec<f32>,
    /// The corner and edge each corner was reached by.
    pub(super) previous: Vec<Option<(usize, usize)>>,
}

impl Search {
    /// Follows predecessor edges from `target` back to any search source.
    ///
    /// Returns edges in target-to-source order. An unreached target has no
    /// predecessor and returns an empty path, so callers check reachability first.
    pub(super) fn path(&self, mut target: usize) -> Vec<usize> {
        let mut path = Vec::new();
        while let Some((before, edge)) = self.previous[target] {
            path.push(edge);
            target = before;
        }
        path
    }

    /// The shortest distance to any of `corners`.
    pub(super) fn nearest(&self, corners: impl IntoIterator<Item = usize>) -> f32 {
        corners
            .into_iter()
            .map(|c| self.distance[c])
            .fold(f32::INFINITY, f32::min)
    }
}

/// A heap entry for Dijkstra, nearest first.
#[derive(PartialEq)]
pub(super) struct Nearest(pub(super) f32, pub(super) usize);

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
/// [`Layout::contracted`](super::Layout::contracted) maps every corner on a pad to `corners + point_index`,
/// leaves other corners unchanged, and coalesces neighbours into sets.
/// [`Self::on_loop`] finds biconnected blocks shared with another pad;
/// [`Self::spur`] identifies the existing exit run to avoid during loop routing.
pub(super) struct Contracted {
    pub(super) corners: usize,
    pub(super) adjacent: BTreeMap<usize, BTreeSet<usize>>,
}

impl Contracted {
    /// Maps a layout routing-point index to its contracted pad vertex.
    pub(super) fn pad(&self, i: usize) -> usize {
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
    pub(super) fn hops(&self, start: usize) -> BTreeMap<usize, u32> {
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
    /// [`Layout::close_loops`](super::Layout::close_loops) to decide which nodes need an alternate route.
    pub(super) fn on_loop(&self, i: usize) -> bool {
        let me = self.pad(i);
        if !self.adjacent.contains_key(&me) {
            return false;
        }
        self.blocks().iter().any(|block| {
            block.len() >= 3
                && block.contains(&me)
                && block.iter().any(|&v| v != me && self.is_pad(v))
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
    pub(super) fn spur(&self, i: usize) -> BTreeSet<usize> {
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

/// Travel estimates for a [`Layout`](super::Layout), computed by [`Layout::pace`](super::Layout::pace).
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
pub(super) fn path_end(first: [usize; 2], second: Option<[usize; 2]>, pad: &[usize; 6]) -> usize {
    match second {
        Some(next) => first.into_iter().find(|c| !next.contains(c)),
        None => first.into_iter().find(|c| !pad.contains(c)),
    }
    .unwrap_or(first[0])
}
