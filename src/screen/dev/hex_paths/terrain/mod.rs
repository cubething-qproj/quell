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

use height::Raster;
pub use mesh::HexMesh;

/// A generated board: the mesh and the height of every corner.
#[derive(Debug, Clone)]
pub struct Terrain {
    pub mesh: HexMesh,
    /// Height of each corner, in hex edge lengths.
    pub heights: Vec<f32>,
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
        let heights = mesh.positions.iter().map(|&p| raster.sample(p)).collect();
        Self { mesh, heights }
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
}
