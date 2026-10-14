//! Mutable landmarks and trails over immutable [`Terrain`].
//!
//! [`Layout::generate`] is a pure-data pipeline: place typed nodes → clone and
//! level terrain heights for pads → install pinned trails → choose tease pairs
//! when walking order is known → span the nodes → join ports → attempt loops
//! → join crossing banks → repair ports again. No stage spawns Bevy entities.
//!
//! Generation first builds without teases to measure walking order from spawn.
//! It rebuilds with the same seed to choose viewer/POI pairs and guard route
//! additions against shortening their detours or reversing arrival order.
//! Connectivity repairs can break a tease; those pairs are disqualified and
//! retried, with remaining broken pairs dropped after the retry limit.
//!
//! [`Layout::regenerate`] translates a partial regeneration or Fear into [`Pins`]
//! and calls the same pipeline. Pins preserve the player, node identities,
//! crossings, and optionally nearby nodes and trails. Cut trail ends become
//! ports to reconnect. Original [`Terrain::heights`] never change: each layout
//! owns a newly levelled copy. [`Board`] owns both layers and
//! records their measurements for the preview.

use super::{
    data::*,
    terrain::{SQRT_3, Terrain, sites},
};
use crate::prelude::*;
use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use std::collections::{BTreeMap, BTreeSet};

mod crossings;
mod network;

pub use network::Pace;

/// Ids of crossing nodes start here, after points of interest and benches.
const FIRST_CROSSING_ID: u32 = 1000;

/// The role assigned to a [`Node`] during placement, before trail routing.
///
/// Crossings open creek barriers; POIs receive the deck's [`Card`] values;
/// benches use the terrain's quiet/overlook score. Regeneration preserves
/// the kind while moving nodes that are not fixed by [`Pins`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A ford or bridge over the main creek. Never moves.
    Crossing,
    /// A point of interest, possibly carrying a deck card.
    Poi,
    /// A rest location placed using quiet and overlook scores.
    Bench,
}

/// A gameplay role dealt to a POI [`Node`] by fresh layout placement.
///
/// The Key, NPC, and Fear cards are shuffled onto available POIs; additional
/// POIs have no card. [`Layout::regenerate`] retains each node's card through
/// [`Pins`]. A fresh unpinned layout deals again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    /// The key role, dealt to an available point of interest.
    Key,
    /// An NPC encounter role, dealt to an available point of interest.
    Npc,
    /// A Fear encounter role, dealt to an available point of interest.
    Fear,
}

/// Cards dealt once per fresh layout, up to the number of available POIs.
const DECK: [Card; 3] = [Card::Key, Card::Npc, Card::Fear];

/// A landmark whose identity, kind, and card survive [`Layout::regenerate`].
///
/// Placement chooses its tile using terrain scores and crossing reachability.
/// [`Pins`] carries the existing value into regeneration, retaining fixed
/// nodes and relocating movable ones. Crossing nodes always remain fixed.
/// Array position is not identity: point indices can change when a node
/// becomes [`Layout::player_node`], while [`Self::id`] stays the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    /// Stable identity within a layout lineage, independent of its current tile.
    pub id: u32,
    /// Current pad tile, indexed into the owning terrain mesh.
    pub tile: usize,
    /// Placement and routing role, preserved across regeneration.
    pub kind: NodeKind,
    /// POI card; `None` for an empty POI, bench, or crossing.
    pub card: Option<Card>,
}

/// The pass that first added an edge to [`Layout::trail`].
///
/// Used by [`HexPathsAppearance::trail`](super::data::HexPathsAppearance::trail) to colour the preview.
/// An edge reused by a later route retains its original kind; retained trail
/// edges receive [`Self::Pinned`] when a regeneration installs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrailKind {
    /// The spanner, joining nodes.
    Span,
    /// A cheap loop.
    Loop,
    /// A loop for a node still on a spur.
    Tree,
    /// The connection pass, so nothing is cut off.
    Forced,
    /// Reaches a crossing's far bank.
    Bank,
    /// Joins a pinned trail's open end.
    Port,
    /// Kept from before a regeneration.
    Pinned,
}

/// Inputs that preserve part of a [`Layout`] while rebuilding its surroundings.
///
/// [`Layout::regenerate`] constructs these from the old layout and player tile,
/// then passes them to [`Layout::generate`]. The player becomes spawn; the node
/// underfoot remains there; fixed nodes retain their tiles; movable nodes keep
/// their IDs and cards but prefer new tiles near their old ones. Preserved
/// trail edges are installed before routing, and their open ends become ports.
/// All tile and edge indices must refer to the same [`Terrain`] as the old layout.
#[derive(Debug, Clone, Default)]
pub struct Pins {
    /// Player tile that becomes the new layout's spawn.
    pub player: usize,
    /// The node the player stands on, if any: it stays, as spawn.
    pub here: Option<Node>,
    /// Nodes whose tiles stay unchanged, including every crossing.
    pub fixed: Vec<Node>,
    /// Existing nodes to relocate, retaining identity, kind, and card.
    pub movable: Vec<Node>,
    /// Mesh edge indices preserved from the previous trail network.
    pub trails: BTreeSet<usize>,
}

/// One arrangement of landmarks, levelled pads, and trails over a [`Terrain`].
///
/// [`Self::generate`] constructs this from ground, settings, a seed, and optional
/// [`Pins`]. [`Self::regenerate`] preserves selected node identities and trail
/// edges while rebuilding the arrangement on the same ground. Heights are a
/// copy of [`Terrain::heights`] with pad levelling; removing a pad on the next
/// build restores its original terrain rather than retaining its old grading.
///
/// Routing point indices are local to this arrangement: point zero is spawn,
/// and point `i > 0` is `nodes[i - 1]`, as returned by [`Self::points`]. These
/// indices identify tease pairs, not stable [`Node::id`] values. Tile, corner,
/// and edge indices refer to the mesh of the terrain owned alongside this
/// layout by [`Board`].
#[derive(Debug, Clone)]
pub struct Layout {
    /// Spawn tile, or the player tile after regeneration; routing point zero.
    pub spawn: usize,
    /// Placed nodes in stable-ID order, excluding the node under the player.
    /// Point indices can still shift when a node moves into [`Self::player_node`].
    pub nodes: Vec<Node>,
    /// The node on spawn's tile (where the player stood), if any.
    pub player_node: Option<Node>,
    /// Levelled corner heights in edge lengths, copied from [`Terrain::heights`].
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

/// Borrowed ground and settings for the pure [`Layout`] algorithms.
///
/// The same context is passed through placement, routing, levelling, and
/// measurement so every stage uses the same [`Terrain`] mesh and units.
/// It does not own a layout, cache, or Bevy world; [`Board`]
/// creates it when measuring its current arrangement.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub terrain: &'a Terrain,
    pub settings: &'a HexPathsSettings,
    pub tuning: &'a HexTerrainTuning,
}

impl Context<'_> {
    /// Enables creek placement exclusions and route barriers when crossings are requested.
    ///
    /// Zero crossings is a debug mode that ignores the creeks, rather than a
    /// board on which creeks are uncrossable.
    fn creeks_matter(&self) -> bool {
        self.settings.crossings > 0
    }
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
    pub fn generate(context: Context, seed: u64, pins: Option<&Pins>) -> Result<Self, &'static str> {
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
                .tile_at(settings.columns as i32 / 2, settings.rows as i32 / 2, context.tuning.margin)
                .ok_or("the middle of the board is not a tile")?,
        };
        if pins.is_none() && context.creeks_matter() && terrain.creeks.on(spawn) {
            // Spawn stands beside the creek, not in it.
            spawn = crossings::dry_tiles(terrain)
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
                crossings::creek_closed(terrain, &crossing_tiles)
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
            layout.trail = pins.trails.iter().map(|&e| (e, TrailKind::Pinned)).collect();
            layout.ports = layout.dead_ends(context);
        }
        if let Some(walked) = walked {
            layout.tease_pairs = layout.choose_teases(context, walked);
        }
        layout.span(context)?;
        layout.join_ports(context);
        layout.close_loops(context, Some(context.tuning.max_connector), &BTreeSet::new(), TrailKind::Loop);
        let targets = layout.tease_pairs.iter().map(|&(_, t)| t).collect();
        layout.close_loops(context, None, &targets, TrailKind::Tree);
        if context.creeks_matter() {
            layout.join_banks(context);
        }
        layout.join_ports(context);
        Ok(layout)
    }

    /// Rebuilds on the same [`Terrain`] around the supplied player tile.
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
        let crossings = self.all_nodes().filter(|n| n.kind == NodeKind::Crossing).copied();
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
            let (stay, go): (Vec<Node>, Vec<Node>) = others.partition(|n| near(mesh.centers[n.tile]));
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
    /// are measured by [`Board`].
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
                (!self.visible(context, v, t), -drop.min(top), self.crow(context, v, t), v, t)
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
        let centre = (mesh.centers[tile], corners.iter().map(|&c| self.heights[c]).sum::<f32>() / 6.);
        (0..6).find_map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 6]);
            let weights = barycentric(point, centre.0, mesh.positions[a], mesh.positions[b])?;
            Some(weights.x * centre.1 + weights.y * self.heights[a] + weights.z * self.heights[b])
        })
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
    let mut buildable: Vec<f32> = terrain.suitability.iter().map(|s| s.powf(settings.spread)).collect();
    let mut bench = terrain.bench_score();
    let mut crossings = Vec::new();
    let mut forbid = None;
    if context.creeks_matter() {
        // Points of interest gather near water; nothing but a crossing sits
        // on a creek.
        let creek: Vec<usize> = (0..mesh.tiles.len()).filter(|&t| terrain.creeks.on(t)).collect();
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
            Some(pins) => pins.fixed.iter().filter(|n| n.kind == NodeKind::Crossing).copied().collect(),
        };
        if !crossings.is_empty() {
            bench
                .iter_mut()
                .enumerate()
                .filter(|(t, _)| terrain.creeks.on(*t))
                .for_each(|(_, s)| *s = 0.);
        }
        let tiles: Vec<usize> = crossings.iter().map(|n: &Node| n.tile).collect();
        forbid = Some(crossings::cut_off(terrain, &tiles, spawn));
    }
    let bench_room = |nodes: &[Node]| -> Vec<bool> {
        (0..mesh.tiles.len())
            .map(|t| {
                nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Bench)
                    .all(|n| mesh.centers[t].distance(mesh.centers[n.tile]) / SQRT_3 >= tuning.min_spacing)
            })
            .collect()
    };
    let score = |kind: NodeKind| if kind == NodeKind::Bench { &bench } else { &buildable };
    let place = |nodes: &[Node], node: Node, near: Option<(usize, Vec2)>, rng: &mut StdRng| {
        let room = (node.kind == NodeKind::Bench).then(|| bench_room(nodes));
        let taken: Vec<usize> = std::iter::once(spawn).chain(nodes.iter().map(|n| n.tile)).collect();
        let constraints = sites::Constraints {
            forbid: forbid.as_deref(),
            room: room.as_deref(),
            near,
        };
        sites::place(mesh, &taken, score(node.kind), tuning.min_spacing, &constraints, rng)
            .map(|tile| Node { tile, ..node })
            .ok_or("no room left for a node")
    };
    let Some(pins) = pins else {
        let mut nodes = crossings;
        let wanted = (0..settings.pois + settings.benches).map(|id| Node {
            id,
            tile: 0,
            kind: if id < settings.pois { NodeKind::Poi } else { NodeKind::Bench },
            card: None,
        });
        for node in wanted {
            let placed = place(&nodes, node, None, rng)?;
            nodes.push(placed);
        }
        if nodes.iter().any(|n| n.kind == NodeKind::Crossing) {
            nodes = crossings::drop_dead_crossings(terrain, nodes, spawn);
        }
        let mut pois: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].kind == NodeKind::Poi).collect();
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

/// Derives a layout seed from the fixed terrain seed and layout count.
///
/// [`relayout`](super::systems::relayout) increments the count for a successful fresh,
/// partial, or Fear layout. This mixes the count into the terrain seed without
/// changing or re-simulating the underlying [`Terrain`].
pub fn regeneration_seed(seed: u64, count: u32) -> u64 {
    seed ^ (count as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

#[cfg(test)]
mod tests {
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
            crossings += layout.nodes.iter().filter(|n| n.kind == NodeKind::Crossing).count();
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
        let poi = first.nodes.iter().find(|n| n.kind == NodeKind::Poi).expect("a point of interest");
        let crossings = |l: &Layout| -> Vec<Node> {
            l.all_nodes().filter(|n| n.kind == NodeKind::Crossing).copied().collect()
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
}
