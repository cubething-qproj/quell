//! Selects and crops useful ground from an eroded [`Region`].
//!
//! [`Window::choose`] tries positions and rotations large enough for the
//! [`HexMesh`] plus scenery. Each candidate traces [`Creeks`] and ranks creek
//! coverage, main-creek fords, natural sight lines, relief, and river-sized flow
//! using [`WindowWeights`]. Only the winner is sampled into a fine [`Raster`].
//! [`Terrain::generate`](super::Terrain::generate) then samples corners and computes placement
//! scores. Ranking and creek tracing use metre heights and hectares; the crop
//! converts heights into board edge lengths. Candidate tease spots describe
//! terrain opportunities, not the actual pairs selected by a layout.
//!
//! Creek links run between tile centres. [`Terrain::creek_sides`](super::Terrain::creek_sides) maps
//! them to blocked trail edges. Gap filling normally makes links neighbouring;
//! points outside the jagged outer margin are skipped, so a link there may lack
//! a shared side. That outer scenery exception closes no trail edge.

use super::{super::data::*, HexMesh, Raster, SQRT_3, erosion::Region};
use crate::prelude::*;
use bevy::platform::collections::HashMap;
use rand::{Rng, RngExt};
use std::f32::consts::TAU;

/// Points sampled along each natural tease's sight line.
const SIGHT_SAMPLES: usize = 12;

/// The chosen crop that [`Terrain::generate`](super::Terrain::generate) turns into board ground.
///
/// [`Self::choose`] ranks frames into a [`Region`], retains the winner's
/// [`Creeks`] and [`Features`], and fills its [`Raster`] by bicubic sampling.
/// The raster has heights in edge lengths; creek drainage is in hectares.
/// Creek arrays use the tile indices of the [`HexMesh`] supplied to selection.
#[derive(Debug, Clone)]
pub struct Window {
    /// Heights in edge lengths.
    pub raster: Raster,
    /// Creeks, per tile: the most water draining through it, in hectares (0
    /// off the creeks), and the tile downstream.
    pub creeks: Creeks,
    /// The winning candidate's score and landscape opportunities, before node placement.
    pub features: Features,
}

/// Tile-centre drainage links traced from the channels of a [`Region`].
///
/// Stored in both candidate [`Window`] data and the resulting [`Terrain`](super::Terrain).
/// Each tile may link to a downstream tile; [`Terrain::creek_sides`](super::Terrain::creek_sides)
/// finds the hex side crossed by that link so layouts can block trails there.
/// [`Self::main_ha`] identifies the higher-flow tiles used for crossings and
/// ford ranking. Arrays follow the selecting [`HexMesh::tiles`] order.
#[derive(Debug, Clone, Default)]
pub struct Creeks {
    /// Largest drainage area mapped to each tile, in hectares; zero off the creeks.
    pub flow_ha: Vec<f32>,
    /// Downstream tile index, or `None` where the trace ends or no link is mapped.
    pub downstream: Vec<Option<usize>>,
}

impl Creeks {
    /// Whether `tile` is on a creek.
    pub fn on(&self, tile: usize) -> bool {
        self.flow_ha[tile] > 0.
    }

    /// The main creek's threshold: half the biggest flow in the board's
    /// interior. Crossings and fords may only be placed in this region.
    ///
    /// Here “region” means creek tiles at or above this flow threshold, rather
    /// than the full simulated [`Region`]. Returns infinity if no interior creek
    /// exists, so no tile qualifies. Uses the same mesh that indexed these arrays.
    pub fn main_ha(&self, mesh: &HexMesh) -> f32 {
        let interior = mesh.interior();
        let biggest = (0..mesh.tiles.len())
            .filter(|&t| interior[t] && self.on(t))
            .map(|t| self.flow_ha[t])
            .fold(0., f32::max);
        if biggest > 0. { biggest / 2. } else { f32::INFINITY }
    }
}

/// Landscape measurements used to rank a [`Window`] before placing nodes.
///
/// [`WindowWeights`] combines the normalised opportunities into [`Self::score`].
/// [`Terrain`](super::Terrain) retains these values for preview statistics. Ford and
/// tease counts describe candidate sites, not placed crossings or held teases.
#[derive(Debug, Clone, Default)]
pub struct Features {
    /// Weighted candidate score; the highest-scoring window wins.
    pub score: f32,
    /// Playable tiles with a traced creek, including playable edge tiles.
    pub creek_tiles: usize,
    /// Ford sites: main-creek tiles with low banks, kept apart.
    pub fords: usize,
    /// Disjoint viewer/target tile pairs with a clear view down by the required drop.
    pub tease_viewers: usize,
    /// Maximum minus minimum sampled playable corner height, in metres.
    pub relief_m: f32,
}

/// A candidate board position and rotation in a [`Region`].
///
/// Maps board edge-length coordinates into region cell coordinates around
/// the board's middle, without scaling: one region cell is one hex edge.
/// The inverse maps channel cells back into tiles for creek tracing.
#[derive(Debug, Clone, Copy)]
struct Frame {
    middle: Vec2,
    centre: Vec2,
    rotation: Mat2,
}

impl Frame {
    /// Rotates and translates a board point into fractional region cell coordinates.
    fn to_region(self, point: Vec2) -> Vec2 {
        self.centre + self.rotation * (point - self.middle)
    }

    /// Applies the inverse frame to map a channel cell back into board coordinates.
    fn to_board(self, cell: Vec2) -> Vec2 {
        self.middle + self.rotation.transpose() * (cell - self.centre)
    }

    /// Region height at board `point`, in metres.
    fn height(&self, region: &Region, point: Vec2) -> f32 {
        let at = self.to_region(point);
        region.sample(at.x as f64, at.y as f64) as f32
    }
}

/// Reusable samples for natural-view opportunities during window ranking.
///
/// Lists interior tile pairs three to six hex spacings apart, plus positions
/// along each sight line. Frames reuse these board-plane samples against
/// different parts of the [`Region`], avoiding repeated geometric setup.
struct SightLines {
    pairs: Vec<(usize, usize)>,
    points: Vec<[Vec2; SIGHT_SAMPLES]>,
}

impl SightLines {
    /// Precomputes interior tile pairs and intermediate points for every candidate frame.
    fn new(mesh: &HexMesh) -> Self {
        let interior = mesh.interior();
        let tiles: Vec<usize> = (0..mesh.tiles.len()).filter(|&t| interior[t]).collect();
        let mut lines = Self {
            pairs: Vec::new(),
            points: Vec::new(),
        };
        for &v in &tiles {
            for &t in &tiles {
                let (a, b) = (mesh.centers[v], mesh.centers[t]);
                if (3. ..=6.).contains(&(a.distance(b) / SQRT_3)) {
                    lines.pairs.push((v, t));
                    lines.points.push(std::array::from_fn(|k| {
                        a + (b - a) * (k + 1) as f32 / (SIGHT_SAMPLES + 1) as f32
                    }));
                }
            }
        }
        lines
    }
}

impl Window {
    /// Ranks candidate board frames into `region` and crops the highest score.
    ///
    /// Positions and rotations keep the mesh plus its raster padding within
    /// region bounds. [`Creeks`] are traced identically for candidate ranking
    /// and the resulting board. Only the winning frame gets a bicubic [`Raster`],
    /// with heights converted from metres to edge lengths. Returns an error if
    /// the board cannot fit; `tuning.candidates` must be positive.
    pub fn choose(
        mesh: &HexMesh,
        region: &Region,
        settings: &HexPathsSettings,
        tuning: &HexTerrainTuning,
        rng: &mut impl Rng,
    ) -> Result<Self, &'static str> {
        let (min, max) = mesh
            .positions
            .iter()
            .fold((Vec2::MAX, Vec2::MIN), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        let (origin, extent) = (min - 2., max + 4. - min);
        let middle = origin + extent / 2.;
        let radius = extent.length() / 2. + 2.;
        let far_edge = region.size as f32 - 1. - radius;
        if far_edge <= radius {
            return Err("the board doesn't fit in the simulated region");
        }
        let lines = SightLines::new(mesh);
        let mut best: Option<(Window, Frame)> = None;
        for _ in 0..tuning.candidates {
            let frame = Frame {
                middle,
                centre: Vec2::new(rng.random_range(radius..far_edge), rng.random_range(radius..far_edge)),
                rotation: Mat2::from_angle(rng.random_range(0. ..TAU)),
            };
            let creeks = trace(mesh, region, settings.creek_ha, &frame, (origin, extent));
            let window = rank(mesh, region, creeks, &frame, tuning, &lines);
            if best.as_ref().is_none_or(|(b, _)| window.features.score > b.features.score) {
                best = Some((window, frame));
            }
        }
        let (mut window, frame) = best.expect("at least one candidate");
        let size = (extent * tuning.resolution).ceil();
        let (width, height) = (size.x as usize, size.y as usize);
        let data = (0..width * height)
            .map(|i| {
                let point = origin + Vec2::new((i / height) as f32, (i % height) as f32) / tuning.resolution;
                let at = frame.to_region(point);
                region.sample_cubic(at.x as f64, at.y as f64) as f32 / tuning.edge_m
            })
            .collect();
        window.raster = Raster {
            origin,
            res: tuning.resolution,
            width,
            height,
            data,
        };
        Ok(window)
    }
}

/// Maps channels above `creek_ha` hectares onto the board's hex tiles.
///
/// Each included region cell maps to its containing tile. Following receivers
/// finds the next mapped downstream tile; intervening tiles are added for
/// links that skip a hex near a corner. The same trace supplies candidate
/// ranking and [`Terrain::creeks`](super::Terrain::creeks), keeping ford sites on the drawn creek.
///
/// Cells and gap-fill points that fall
/// outside every hex (the jagged outer edge of the margin) are skipped. A
/// link there may join tiles that aren't neighbours, and then closes no side;
/// that only happens in the outer margin, which takes no trails.
fn trace(
    mesh: &HexMesh,
    region: &Region,
    creek_ha: f32,
    frame: &Frame,
    (origin, extent): (Vec2, Vec2),
) -> Creeks {
    let mut flow_ha = vec![0f32; mesh.tiles.len()];
    let mut downstream = vec![None; mesh.tiles.len()];
    let size = region.size;
    let cells: Vec<usize> = (0..size * size)
        .filter(|&i| region.area[i] >= creek_ha as f64 * 1e4)
        .collect();
    let mut tile_of = HashMap::<usize, usize>::default();
    for &i in &cells {
        let point = frame.to_board(Vec2::new((i / size) as f32, (i % size) as f32));
        let inside = point.cmpge(origin).all() && point.cmple(origin + extent).all();
        if let Some(tile) = mesh.tile_containing(point).filter(|_| inside) {
            tile_of.insert(i, tile);
        }
    }
    for &i in cells.iter().filter(|i| tile_of.contains_key(*i)) {
        let tile = tile_of[&i];
        flow_ha[tile] = flow_ha[tile].max((region.area[i] / 1e4) as f32);
        // Follow the channel down to the next tile.
        let mut next = region.receiver[i];
        while let Some(j) = next.filter(|j| tile_of.get(j) == Some(&tile)) {
            next = region.receiver[j];
        }
        if let Some(&below) = next.and_then(|j| tile_of.get(&j))
            && downstream[tile].is_none()
        {
            downstream[tile] = Some(below);
        }
    }
    // A channel passing near a hex corner can skip a tile between two cells;
    // fill such gaps with the tiles along the straight line, so every creek
    // link joins neighbouring tiles (and closes exactly one side).
    let linked: Vec<usize> = (0..mesh.tiles.len()).filter(|&t| downstream[t].is_some()).collect();
    for t in linked {
        let Some(n) = downstream[t] else { continue };
        let (a, b) = (mesh.centers[t], mesh.centers[n]);
        if a.distance(b) <= SQRT_3 + 1e-4 {
            continue;
        }
        let mut chain = vec![t];
        for k in 1..8 {
            if let Some(between) = mesh.tile_containing(a + (b - a) * k as f32 / 8.)
                && between != n
                && chain.last() != Some(&between)
            {
                chain.push(between);
            }
        }
        chain.push(n);
        for pair in chain.windows(2) {
            downstream[pair[0]] = Some(pair[1]);
            flow_ha[pair[1]] = flow_ha[pair[1]].max(flow_ha[t]);
        }
    }
    Creeks {
        flow_ha,
        downstream,
    }
}

/// Scores one candidate frame using its traced creeks and sampled metre heights.
///
/// Ford candidates are low-bank main-creek tiles kept apart. Natural tease
/// opportunities are disjoint tile pairs with enough drop and a sampled clear
/// sight line. Creek, ford, and tease counts saturate at their target counts;
/// relief adds a range bonus and river-sized flow subtracts a penalty.
/// Leaves the raster empty until [`Window::choose`] selects the winner.
fn rank(
    mesh: &HexMesh,
    region: &Region,
    creeks: Creeks,
    frame: &Frame,
    tuning: &HexTerrainTuning,
    lines: &SightLines,
) -> Window {
    let w = &tuning.window;
    let centre_m: Vec<f32> = mesh.centers.iter().map(|&c| frame.height(region, c)).collect();
    let corner_m: Vec<f32> = mesh.positions.iter().map(|&p| frame.height(region, p)).collect();
    let interior = mesh.interior();
    let creek: Vec<bool> = (0..mesh.tiles.len()).map(|t| creeks.on(t) && mesh.playable[t]).collect();
    let (low, high) = mesh
        .tile_corners
        .iter()
        .zip(&mesh.playable)
        .filter(|(_, playable)| **playable)
        .flat_map(|(corners, _)| corners)
        .fold((f32::MAX, f32::MIN), |(lo, hi), &c| (lo.min(corner_m[c]), hi.max(corner_m[c])));
    let relief = high - low;
    let bank: Vec<f32> = mesh.tile_corners.iter().map(|c| bank_span(c.map(|c| corner_m[c]))).collect();
    let main = creeks.main_ha(mesh);
    let mut sites: Vec<usize> = (0..mesh.tiles.len())
        .filter(|&t| creek[t] && interior[t] && creeks.flow_ha[t] >= main && bank[t] <= w.ford_bank_m)
        .collect();
    sites.sort_by(|&a, &b| bank[a].total_cmp(&bank[b]));
    let mut fords: Vec<usize> = Vec::new();
    for t in sites {
        let spaced = |f: &usize| mesh.centers[t].distance(mesh.centers[*f]) >= w.ford_spacing * SQRT_3;
        if fords.iter().all(spaced) {
            fords.push(t);
        }
    }
    // Natural teases: viewer sees target, at least `tease_drop_m` lower.
    let mut drops: Vec<(f32, usize, usize)> = lines
        .pairs
        .iter()
        .zip(&lines.points)
        .filter_map(|(&(v, t), points)| {
            let drop = centre_m[v] - centre_m[t];
            let (eye, goal) = (centre_m[v] + tuning.eye_m, centre_m[t] + tuning.landmark_m);
            let clear = points.iter().enumerate().all(|(k, &p)| {
                let f = (k + 1) as f32 / (SIGHT_SAMPLES + 1) as f32;
                frame.height(region, p) <= eye + (goal - eye) * f + 1e-6
            });
            (drop >= w.tease_drop_m && clear).then_some((drop, v, t))
        })
        .collect();
    drops.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut teases = Vec::new();
    let mut used = Vec::new();
    for (_, v, t) in drops {
        if !used.contains(&v) && !used.contains(&t) {
            teases.push((v, t));
            used.extend([v, t]);
        }
    }
    let creek_tiles = creek.iter().filter(|&&c| c).count();
    let biggest = (0..mesh.tiles.len())
        .filter(|&t| creek[t])
        .map(|t| creeks.flow_ha[t])
        .fold(0., f32::max);
    let full = |count: f32, wanted: f32| (count / wanted).min(1.);
    let score = w.creek * full(creek_tiles as f32, w.creek_tiles)
        + w.fords * full(fords.len() as f32, w.fords_wanted)
        + w.teases * full(teases.len() as f32, w.viewers)
        + w.relief * if (w.relief_m.x..=w.relief_m.y).contains(&relief) { 1. } else { 0. }
        - w.river * if biggest > w.river_ha { 1. } else { 0. };
    Window {
        raster: Raster {
            origin: Vec2::ZERO,
            res: 1.,
            width: 0,
            height: 0,
            data: Vec::new(),
        },
        creeks,
        features: Features {
            score,
            creek_tiles,
            fords: fords.len(),
            tease_viewers: teases.len(),
            relief_m: relief,
        },
    }
}

/// Measures a creek tile's bank-height range from its six corners.
///
/// Returns highest minus lowest, in the input height unit. Window ford ranking
/// uses metres; layout crossing selection uses edge lengths. This is a height
/// span across the pad, rather than a gradient or a classification as a ford.
pub fn bank_span(corners: [f32; 6]) -> f32 {
    let (low, high) = corners
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
    high - low
}

#[cfg(test)]
mod tests {
    use super::super::tests::small;

    #[test]
    fn creek_links_join_neighbouring_tiles() {
        let (_, _, terrain) = small(2);
        let (mesh, creeks) = (&terrain.mesh, &terrain.creeks);
        let mut links = 0;
        for (t, below) in creeks.downstream.iter().enumerate() {
            if let Some(n) = below {
                links += 1;
                assert!(mesh.neighbours(t).any(|x| x == *n));
                assert!(creeks.on(t) && creeks.on(*n));
            }
        }
        assert!(links > 0, "a ranked window has creeks");
    }
}
