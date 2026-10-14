//! Immutable board terrain cropped from a larger eroded landscape.
//!
//! [`Region::simulate`] produces metre heights, downhill receivers, and drainage
//! areas on a square grid. [`Window::choose`] ranks positions and rotations by
//! creeks, ford opportunities, natural views, rivers, and relief, then crops the
//! best into a finer [`Raster`]. [`Terrain::generate`] samples its hex corners,
//! computes buildability, and ranks quiet and overlook sites for benches.
//!
//! [`HexMesh`] supplies two connected geometries: trails use corner-to-corner
//! hex sides; [`Creeks`] run between tile centres and block the sides they cross.
//! Region and window scoring use metres; terrain heights and mesh coordinates
//! use hex edge lengths, converted by [`HexTerrainTuning::edge_m`].
//!
//! [`Layout`](super::layout::Layout) borrows this ground, clones its heights for pad
//! levelling, and places nodes and trails. A new layout therefore restores the
//! original ground where old pads disappear. [`Board`] owns both
//! the fixed terrain and the current layout for the preview.

use super::data::*;
use crate::prelude::*;
use rand::Rng;

/// √3, until `f32::consts::SQRT_3` is stable.
pub const SQRT_3: f32 = 1.732_050_8;

pub mod erosion;
mod height;
mod mesh;
pub mod sites;
pub mod window;

use erosion::Region;
use height::Raster;
pub use mesh::HexMesh;
use window::{Creeks, Features, Window};

/// The fixed ground and placement scores shared by layouts on one board.
///
/// [`Self::generate`] selects a [`Window`] from an eroded [`Region`], samples
/// its heights onto a [`HexMesh`], and computes suitability and bench scores.
/// Tile-indexed arrays follow [`HexMesh::tiles`]; corner heights follow
/// [`HexMesh::positions`] and are stored in hex edge lengths.
///
/// A [`Layout`](super::layout::Layout) adds nodes, creek restrictions, and trails,
/// levelling a copy of these heights. Regeneration replaces that layout while
/// the owning [`Board`] retains this terrain.
#[derive(Debug, Clone)]
pub struct Terrain {
    pub mesh: HexMesh,
    /// Height of each corner, in hex edge lengths.
    pub heights: Vec<f32>,
    pub creeks: Creeks,
    /// What the window was chosen for.
    pub features: Features,
    /// How buildable each tile is, 0–1.
    pub suitability: Vec<f32>,
    /// The quiet share of each tile's bench score: how enclosed it is,
    /// sharpened and weighted by `1 - view`.
    pub bench_quiet: Vec<f32>,
    /// The overlook share of each tile's bench score: how far it looks down
    /// on its surroundings, sharpened and weighted by `view`.
    pub bench_overlook: Vec<f32>,
}

impl Terrain {
    /// Crops a board from `region` and computes its node-placement scores.
    ///
    /// The caller supplies a [`Region`] simulated with matching physical settings.
    /// Builds a [`HexMesh`], chooses a ranked [`Window`], samples its corner heights,
    /// then turns [`views`](sites::views) into sharpened quiet and overlook ranks.
    /// Returns an error if the board and scenery margin do not fit in the region.
    pub fn generate(
        settings: &HexPathsSettings,
        tuning: &HexTerrainTuning,
        region: &Region,
        rng: &mut impl Rng,
    ) -> Result<Self, &'static str> {
        let mesh = HexMesh::build(settings.columns, settings.rows, tuning.margin);
        let Window {
            raster,
            creeks,
            features,
        } = Window::choose(&mesh, region, settings, tuning, rng)?;
        let heights: Vec<f32> = mesh.positions.iter().map(|&p| raster.sample(p)).collect();
        let suitability = sites::suitability(&mesh, &heights);
        let (open, overlook) = sites::views(
            &mesh,
            &raster,
            tuning.view_radius,
            tuning.edges(tuning.eye_m),
            tuning.edges(tuning.overlook_m),
        );
        let enclosed: Vec<f32> = open.iter().map(|o| 1. - o).collect();
        let quiet = sites::rank(&enclosed, &mesh.playable);
        let overlook = sites::rank(&overlook, &mesh.playable);
        // Sharpened so only the most enclosed spots and the best overlooks
        // score well, not ground that is middling at both.
        let share = |ranks: Vec<f32>, weight: f32| -> Vec<f32> {
            ranks
                .into_iter()
                .map(|r| weight * r.powf(tuning.bench_sharpness))
                .collect()
        };
        Ok(Self {
            bench_quiet: share(quiet, 1. - settings.view),
            bench_overlook: share(overlook, settings.view),
            mesh,
            heights,
            creeks,
            features,
            suitability,
        })
    }

    /// Combines each tile's weighted quiet and overlook scores for bench placement.
    ///
    /// The two terms were ranked among playable tiles and sharpened during
    /// [`Self::generate`]; margin tiles have zero score.
    pub fn bench_score(&self) -> Vec<f32> {
        self.bench_quiet
            .iter()
            .zip(&self.bench_overlook)
            .map(|(q, o)| q + o)
            .collect()
    }

    /// Assigns a symmetric routing weight to each [`HexMesh::edges`] entry.
    ///
    /// Playable edges cost `exp(aversion × |rise|)`, with rise measured in edge
    /// lengths; edges outside the playable corner graph cost infinity. `heights`
    /// must match [`HexMesh::positions`], and may be the levelled copy from a
    /// [`Layout`](super::layout::Layout). Layout routing also masks closed creek sides.
    /// These weights penalise steep routes; actual minutes are estimated by
    /// [`Layout::pace`](super::layout::Layout::pace) using the full Tobler speed curve.
    pub fn step_costs(&self, heights: &[f32], aversion: f32) -> Vec<f32> {
        self.mesh
            .edges
            .iter()
            .zip(&self.mesh.playable_edges)
            .map(|(&[a, b], &playable)| {
                if playable {
                    (aversion * (heights[a] - heights[b]).abs()).exp()
                } else {
                    f32::INFINITY
                }
            })
            .collect()
    }

    /// Finds the honeycomb edge crossed by each centre-to-centre creek link.
    ///
    /// The returned indices refer to [`HexMesh::edges`]. Layout generation closes
    /// these edges except on crossing-pad perimeters. Links without a shared side
    /// are skipped; see the outer-margin limitation in [`window`].
    pub fn creek_sides(&self) -> Vec<usize> {
        self.creeks
            .downstream
            .iter()
            .enumerate()
            .filter_map(|(t, below)| below.and_then(|n| self.mesh.shared_side(t, n)))
            .collect()
    }

    /// Per-point amounts from per-corner ones: centres take the mean of their
    /// corners.
    pub fn corner_amounts(&self, corners: &[f32]) -> Vec<f32> {
        let centres = self
            .mesh
            .tile_corners
            .iter()
            .map(|tile| tile.iter().map(|&c| corners[c]).sum::<f32>() / 6.);
        corners.iter().copied().chain(centres).collect()
    }

    /// Per-point amounts from per-tile ones: corners take the mean of the tiles
    /// they belong to.
    pub fn tile_amounts(&self, tiles: &[f32]) -> Vec<f32> {
        let mesh = &self.mesh;
        let mut sums = vec![(0., 0.); mesh.positions.len()];
        for (corners, &amount) in mesh.tile_corners.iter().zip(tiles) {
            for &c in corners {
                sums[c].0 += amount;
                sums[c].1 += 1.;
            }
        }
        sums.into_iter()
            .map(|(sum, count)| sum / count)
            .chain(tiles.iter().copied())
            .collect()
    }

    /// Lists interior tiles off the creeks, used for ordinary node placement.
    ///
    /// Indices refer to [`Terrain::mesh`]. Crossing placement uses these tiles to
    /// measure whether each disconnected corner part has room for landmarks.
    pub fn dry_tiles(&self) -> Vec<usize> {
        let interior = self.mesh.interior();
        (0..self.mesh.tiles.len())
            .filter(|&t| interior[t] && !self.creeks.on(t))
            .collect()
    }

    /// Builds a per-edge barrier mask, opening creek sides on crossing-pad perimeters.
    ///
    /// Begins with [`Terrain::creek_sides`]. `crossings` holds tile indices, not
    /// node or edge indices. [`Layout`](super::layout::Layout) stores the resulting mask for routing.
    pub fn creek_closed(&self, crossings: &[usize]) -> Vec<bool> {
        let mut closed = vec![false; self.mesh.edges.len()];
        self.creek_sides()
            .into_iter()
            .for_each(|e| closed[e] = true);
        for &t in crossings {
            self.mesh
                .perimeter(t)
                .into_iter()
                .for_each(|e| closed[e] = false);
        }
        closed
    }

    /// Labels corners by connected part under creek barriers and extra closed edges.
    ///
    /// Union-find joins the endpoints of every playable edge not shut by
    /// [`Self::creek_closed`] or `closed`. The latter contains edge indices;
    /// `crossings` contains tile indices. Equal labels mean corners can reach
    /// each other on the available ground, before trails are laid. Labels are
    /// representative corner indices, not dense or stable part IDs.
    pub fn corner_parts(&self, crossings: &[usize], closed: &[usize]) -> Vec<usize> {
        let mesh = &self.mesh;
        let mut shut = self.creek_closed(crossings);
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
    /// when its first corner shares spawn's corner-part label from [`Self::corner_parts`].
    /// The mask becomes a hard [`Constraints`](sites::Constraints)
    /// exclusion during placement.
    pub fn cut_off(&self, crossings: &[usize], spawn: usize) -> Vec<bool> {
        let label = self.corner_parts(crossings, &[]);
        let corners = &self.mesh.tile_corners;
        let home = label[corners[spawn][0]];
        let mut cut = vec![true; self.mesh.tiles.len()];
        for t in self.dry_tiles() {
            cut[t] = label[corners[t][0]] != home;
        }
        cut
    }
}

/// Expands corner heights into the point order used by [`HexMesh::triangles`].
///
/// Keeps the input corners first, then appends one height per tile centre: the
/// mean of that tile's six corners. Input heights use the same edge-length
/// units as [`Terrain::heights`] or the levelled layout copy.
pub fn point_heights(mesh: &HexMesh, heights: &[f32]) -> Vec<f32> {
    let centres = mesh
        .tile_corners
        .iter()
        .map(|corners| corners.iter().map(|&c| heights[c]).sum::<f32>() / 6.);
    heights.iter().copied().chain(centres).collect()
}

/// Maps a hex-plane point to Bevy world coordinates: `(x, y)` becomes `(x, z)`.
///
/// World Y is `height` and world Z is the negated hex-plane Y. The caller
/// applies any preview height exaggeration before this conversion.
pub fn world(point: Vec2, height: f32) -> Vec3 {
    Vec3::new(point.x, height, -point.y)
}

#[cfg(test)]
pub(super) mod tests {
    use super::{erosion::RegionKey, *};
    use rand::{SeedableRng, rngs::StdRng};

    /// Builds a small eroded terrain and matching knobs for layout property tests.
    ///
    /// Uses lower creek thresholds because the smaller region collects less
    /// drainage area. This checks seeded examples, not the default region scale.
    pub fn small(seed: u64) -> (HexPathsSettings, HexTerrainTuning, Terrain) {
        // A smaller region grows smaller creeks, so the threshold is lower.
        let settings = HexPathsSettings {
            columns: 12,
            rows: 12,
            creek_ha: 15.,
            ..default()
        };
        let tuning = HexTerrainTuning {
            region_hexes: 60,
            iterations: 150,
            candidates: 12,
            margin: 4,
            ..default()
        };
        let region = Region::simulate(
            &RegionKey::new(seed, &settings, &tuning),
            &mut StdRng::seed_from_u64(seed),
        );
        let terrain = Terrain::generate(
            &settings,
            &tuning,
            &region,
            &mut StdRng::seed_from_u64(seed),
        )
        .expect("the board fits the region");
        (settings, tuning, terrain)
    }

    #[test]
    fn steps_cost_more_uphill_and_nothing_off_the_board() {
        let (_, tuning, terrain) = small(3);
        let costs = terrain.step_costs(&terrain.heights, tuning.aversion);
        let flat = terrain.step_costs(&vec![0.; terrain.heights.len()], tuning.aversion);
        for ((cost, flat_cost), &playable) in
            costs.iter().zip(flat).zip(&terrain.mesh.playable_edges)
        {
            assert_eq!(cost.is_finite(), playable);
            assert!(!playable || (*cost >= 1. && flat_cost == 1.));
        }
    }
}
