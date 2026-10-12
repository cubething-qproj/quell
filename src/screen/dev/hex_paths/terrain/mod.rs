//! Terrain-first board generation, ported from the hex-lab prototype
//! (`misc/hex-lab/src/hexlab/terrain.py`). See
//! `.docs/designs/quell-hex-terrain-design.md`. Pure data: no ECS.

use super::data::*;
use crate::prelude::*;
use rand::Rng;

/// √3, until `f32::consts::SQRT_3` is stable.
const SQRT_3: f32 = 1.732_050_8;

mod height;
mod mesh;
mod sites;

use height::Raster;
pub use mesh::HexMesh;

/// A generated board: the ground, how it suits pads, and where they went.
#[derive(Debug, Clone)]
pub struct Terrain {
    pub mesh: HexMesh,
    /// Height of each corner, in hex edge lengths.
    pub heights: Vec<f32>,
    /// How buildable each tile is, 0–1.
    pub suitability: Vec<f32>,
    /// How good a bench each tile would make, 0–1, blending quiet and
    /// overlook by the `view` knob.
    pub bench_score: Vec<f32>,
    /// Spawn's tile.
    pub spawn: usize,
    /// Each node's tile, untyped until trails exist.
    pub nodes: Vec<usize>,
}

impl Terrain {
    /// Generates the terrain for `settings`.
    pub fn generate(
        settings: &HexPathsSettings,
        tuning: &HexTerrainTuning,
        rng: &mut impl Rng,
    ) -> Self {
        let mesh = HexMesh::build(settings.columns, settings.rows, tuning.margin);
        let raster = Raster::heightmap(&mesh, settings, tuning, rng);
        let heights: Vec<f32> = mesh.positions.iter().map(|&p| raster.sample(p)).collect();
        let suitability = sites::suitability(&mesh, &heights);
        let (open, overlook) = sites::views(&mesh, &raster, tuning.view_radius, tuning.eye_height);
        let (quiet, overlook) = (sites::rank(&open), sites::rank(&overlook));
        let bench_score = quiet
            .iter()
            .zip(&overlook)
            .map(|(q, o)| (1. - settings.view) * (1. - q) + settings.view * o)
            .collect();
        let spawn = mesh
            .tile_at(
                settings.columns as i32 / 2,
                settings.rows as i32 / 2,
                tuning.margin,
            )
            .expect("the middle of the board is a tile");
        let score: Vec<f32> = suitability.iter().map(|s| s.powf(settings.spread)).collect();
        let nodes = sites::farthest(
            &mesh,
            &[spawn],
            &score,
            settings.nodes as usize,
            tuning.min_spacing,
            rng,
        );
        Self {
            mesh,
            heights,
            suitability,
            bench_score,
            spawn,
            nodes,
        }
    }

    /// Height of a pad: the median of its tile's corners.
    pub fn pad_height(&self, tile: usize) -> f32 {
        let mut corners = self.mesh.tile_corners[tile].map(|c| self.heights[c]);
        corners.sort_by(f32::total_cmp);
        (corners[2] + corners[3]) / 2.
    }

    /// Walking cost of each edge by Tobler's hiking function,
    /// `exp(aversion × |rise|)`; infinite off the playable board.
    pub fn step_costs(&self, aversion: f32) -> Vec<f32> {
        self.mesh
            .edges
            .iter()
            .zip(&self.mesh.playable_edges)
            .map(|(&[a, b], &playable)| {
                if playable {
                    (aversion * (self.heights[a] - self.heights[b]).abs()).exp()
                } else {
                    f32::INFINITY
                }
            })
            .collect()
    }

    /// Height of every mesh point: corners, then tile centres at the mean of
    /// their corners.
    pub fn point_heights(&self) -> Vec<f32> {
        let centres = self
            .mesh
            .tile_corners
            .iter()
            .map(|corners| corners.iter().map(|&c| self.heights[c]).sum::<f32>() / 6.);
        self.heights.iter().copied().chain(centres).collect()
    }
}

/// World position of a mesh point at `height`: the hex plane is XZ, up is Y.
pub fn world(point: Vec2, height: f32) -> Vec3 {
    Vec3::new(point.x, height, -point.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    #[test]
    fn heights_span_the_relief_and_repeat_for_a_seed() {
        let settings = HexPathsSettings::default();
        let tuning = HexTerrainTuning::default();
        let generate = || Terrain::generate(&settings, &tuning, &mut StdRng::seed_from_u64(3));
        let terrain = generate();
        assert!(
            terrain
                .heights
                .iter()
                .all(|h| (0. ..=settings.relief).contains(h))
        );
        assert_eq!(terrain.heights, generate().heights);
    }

    #[test]
    fn nodes_spread_out_inside_the_board() {
        let settings = HexPathsSettings {
            columns: 24,
            rows: 20,
            ..default()
        };
        let tuning = HexTerrainTuning::default();
        let terrain = Terrain::generate(&settings, &tuning, &mut StdRng::seed_from_u64(3));
        let interior = terrain.mesh.interior();
        assert_eq!(terrain.nodes.len(), settings.nodes as usize);
        let pads: Vec<usize> = std::iter::once(terrain.spawn).chain(terrain.nodes.clone()).collect();
        for (i, &a) in pads.iter().enumerate() {
            assert!(terrain.mesh.playable[a]);
            for &b in &pads[i + 1..] {
                let apart = terrain.mesh.centers[a].distance(terrain.mesh.centers[b]) / SQRT_3;
                assert!(apart >= tuning.min_spacing);
            }
        }
        assert!(terrain.nodes.iter().all(|&n| interior[n]));
    }

    #[test]
    fn steps_cost_more_uphill_and_nothing_off_the_board() {
        let mut terrain = Terrain::generate(
            &HexPathsSettings::default(),
            &HexTerrainTuning::default(),
            &mut StdRng::seed_from_u64(3),
        );
        let costs = terrain.step_costs(3.5);
        terrain.heights.iter_mut().for_each(|h| *h = 0.);
        for ((cost, flat_cost), &playable) in costs
            .iter()
            .zip(terrain.step_costs(3.5))
            .zip(&terrain.mesh.playable_edges)
        {
            assert_eq!(cost.is_finite(), playable);
            assert!(!playable || (*cost >= 1. && flat_cost == 1.));
        }
    }
}
