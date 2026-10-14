//! Regional drainage and erosion, before a playable board is selected.
//!
//! [`Region::simulate`] begins with broad rolls and small roughness, then repeats
//! uplift → priority flood → D8 drainage → stream-power cuts → slope creep.
//! All four boundaries stay at zero as outlets. A final flood and drainage pass
//! leave heights, receivers, and accumulated areas consistent.
//!
//! [`Region`] is a square raster in metres, with one cell per hex edge length.
//! [`Window::choose`](super::window::Window::choose) ranks and crops it to build a much smaller
//! [`Terrain`](super::Terrain); [`Layout`](super::super::layout::Layout) later places trails on
//! that terrain. [`RegionKey`] lets [`RegionCache`] reuse
//! the simulation when only window or layout settings change.

use super::{
    super::data::*,
    height::{blur, standard_normal},
};
use rand::Rng;
use std::{cmp::Ordering, collections::BinaryHeap};

/// The eight D8 drainage offsets, in stable iteration order for slope ties.
const NEIGHBOURS: [(isize, isize); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];
/// Stream-power exponent on drainage area.
const AREA_EXPONENT: f64 = 0.5;
/// Minimum height increase above a flood predecessor, in metres.
const FLOOD_EPSILON: f64 = 1e-3;

/// The eroded landscape from which [`Terrain`](super::Terrain) selects its ground.
///
/// [`Self::simulate`] produces metre heights, each cell's downhill receiver,
/// and the drainage area passing through it. [`Window::choose`](super::window::Window::choose)
/// maps a rotated hex board into this raster, traces its channels, and crops
/// its heights. Several layouts can then share that board's fixed terrain.
///
/// Each array has `size × size` entries. Cell `(x, y)` is at `x * size + y`,
/// so consecutive entries vary Y within a column. Cell-coordinate sampling
/// uses this indexing; physical distances multiply by [`Self::cell`].
#[derive(Debug, Clone)]
pub struct Region {
    /// Number of cells along each side of the square region.
    pub size: usize,
    /// Cell size, in metres.
    pub cell: f64,
    /// Ground height above the fixed boundary outlets, in metres.
    pub heights: Vec<f64>,
    /// The cell water flows to; `None` at the outlets.
    pub receiver: Vec<Option<usize>>,
    /// Area draining through each cell, in m².
    pub area: Vec<f64>,
}

/// Cache key for [`Region`].
///
/// Contains every seed and physical setting used by [`Region::simulate`].
/// [`RegionCache`] compares it before reusing the last
/// region; board size, creek tracing, and node placement do not belong to this
/// key because they operate after erosion.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionKey {
    pub seed: u64,
    pub uplift_m: f32,
    pub channel_ha: f32,
    pub hexes: u32,
    pub regional_pct: f32,
    pub roll_m: f32,
    pub roll_size_m: f32,
    pub roughness_m: f32,
    pub erodibility: f32,
    pub iterations: u32,
    pub creep: f32,
    pub edge_m: f32,
}

impl RegionKey {
    /// Collects the seed and erosion inputs for [`Region::simulate`].
    ///
    /// Layout and window-only settings are omitted so changing them can reuse
    /// the simulated region in [`RegionCache`].
    pub fn new(seed: u64, settings: &HexPathsSettings, tuning: &HexTerrainTuning) -> Self {
        Self {
            seed,
            uplift_m: settings.uplift_m,
            channel_ha: settings.channel_ha,
            hexes: tuning.region_hexes,
            regional_pct: tuning.regional_pct,
            roll_m: tuning.roll_m,
            roll_size_m: tuning.roll_size_m,
            roughness_m: tuning.roughness_m,
            erodibility: tuning.erodibility,
            iterations: tuning.iterations,
            creep: tuning.creep,
            edge_m: tuning.edge_m,
        }
    }
}

/// A heap entry for the priority flood, lowest height first.
#[derive(PartialEq)]
struct Lowest(f64, usize);

impl Eq for Lowest {}

impl Ord for Lowest {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0).then(other.1.cmp(&self.1))
    }
}

impl PartialOrd for Lowest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Region {
    /// Simulates a landscape for later cropping by [`Window::choose`](super::window::Window::choose).
    ///
    /// The square grid has `1.5 × key.hexes` cells on each side, accounting for
    /// hex-column spacing; each cell is `key.edge_m` metres across. Starting
    /// rolls and roughness come from `rng`. Each pass lifts the interior against
    /// fixed outlets on every side, fills depressions for downhill drainage,
    /// accumulates upstream area, cuts channels, and smooths slopes by creep.
    /// The final flood and routing pass populate the returned [`Region`].
    pub fn simulate(key: &RegionKey, rng: &mut impl Rng) -> Self {
        let size = (key.hexes as f64 * 1.5) as usize;
        let cell = key.edge_m as f64;
        let white = |rng: &mut _| {
            (0..size * size)
                .map(|_| standard_normal(rng))
                .collect::<Vec<_>>()
        };
        let rolls = blur(white(rng), size, size, key.roll_size_m / key.edge_m / 2.);
        let deviation = std_dev(&rolls);
        let rough = white(rng);
        let mut heights: Vec<f64> = (0..size * size)
            .map(|i| {
                let x = (i / size) as f64 * cell;
                x * key.regional_pct as f64 / 100.
                    + key.roll_m as f64 * rolls[i] as f64 / deviation
                    + key.roughness_m as f64 * rough[i] as f64
            })
            .collect();
        // Drains off every side: rivers all round.
        let outlet: Vec<bool> = (0..size * size)
            .map(|i| {
                let (x, y) = (i / size, i % size);
                x == 0 || y == 0 || x == size - 1 || y == size - 1
            })
            .collect();
        let mut region = Self {
            size,
            cell,
            heights: Vec::new(),
            receiver: Vec::new(),
            area: Vec::new(),
        };
        for _ in 0..key.iterations {
            for (h, &out) in heights.iter_mut().zip(&outlet) {
                *h = if out { 0. } else { *h + key.uplift_m as f64 };
            }
            region.heights = region.flood(&heights, &outlet);
            let slope = region.route(&outlet);
            let mut carved = region.heights.clone();
            for i in 0..size * size {
                if outlet[i] || region.area[i] < key.channel_ha as f64 * 1e4 {
                    continue;
                }
                let cut = key.erodibility as f64
                    * (region.area[i] / 1e6).powf(AREA_EXPONENT)
                    * (slope[i] / 0.1);
                let floor = region.receiver[i].map_or(region.heights[i], |r| region.heights[r]);
                carved[i] = (region.heights[i] - cut).max(floor + FLOOD_EPSILON);
            }
            heights = region.creep(&carved, key.creep as f64);
            outlet
                .iter()
                .zip(&mut heights)
                .filter(|(out, _)| **out)
                .for_each(|(_, h)| *h = 0.);
        }
        region.heights = region.flood(&heights, &outlet);
        region.route(&outlet);
        region
    }

    /// Samples bilinear height in metres at fractional region cell coordinates.
    ///
    /// Coordinates are clamped to the region's edge. Window ranking uses this
    /// cheaper interpolation before [`Self::sample_cubic`] builds the chosen crop.
    pub fn sample(&self, x: f64, y: f64) -> f64 {
        let max = (self.size - 1) as f64;
        let (x, y) = (x.clamp(0., max), y.clamp(0., max));
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.size - 1), (y0 + 1).min(self.size - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let h = |x: usize, y: usize| self.heights[x * self.size + y];
        let near = h(x0, y0) * (1. - fy) + h(x0, y1) * fy;
        let far = h(x1, y0) * (1. - fy) + h(x1, y1) * fy;
        near * (1. - fx) + far * fx
    }

    /// Bicubic ([`Catmull-Rom`](https://en.wikipedia.org/wiki/Catmull%E2%80%93Rom_spline)) height at region cell coordinates, clamped to
    /// the region's edge for each contributing sample.
    ///
    /// Returns metres from a 4 × 4 neighbourhood. [`Window::choose`](super::window::Window::choose)
    /// uses this smoother interpolation for the selected board raster; unlike
    /// [`Self::sample`], the interpolation may overshoot neighbouring heights.
    pub fn sample_cubic(&self, x: f64, y: f64) -> f64 {
        let max = (self.size - 1) as isize;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let h = |x: isize, y: isize| {
            self.heights[x.clamp(0, max) as usize * self.size + y.clamp(0, max) as usize]
        };
        let weights = |t: f64| {
            [
                ((-t + 2.) * t - 1.) * t / 2.,
                ((3. * t - 5.) * t * t + 2.) / 2.,
                ((-3. * t + 4.) * t + 1.) * t / 2.,
                (t - 1.) * t * t / 2.,
            ]
        };
        let (wx, wy) = (weights(fx), weights(fy));
        (0..4)
            .map(|i| {
                let row: f64 = (0..4)
                    .map(|j| wy[j] * h(x0 as isize - 1 + i as isize, y0 as isize - 1 + j as isize))
                    .sum();
                wx[i] * row
            })
            .sum()
    }

    /// Fills depressions enough to give every non-outlet a downhill path out.
    ///
    /// The min-heap expands from all outlets, assigning each newly visited cell
    /// at least its original height and its predecessor's height plus epsilon.
    /// This raises pit floors; it does not cut a breach through higher ground.
    /// [`Self::route`] can then choose strictly lower receivers without cycles.
    fn flood(&self, heights: &[f64], outlet: &[bool]) -> Vec<f64> {
        let mut out = vec![f64::INFINITY; heights.len()];
        let mut heap = BinaryHeap::new();
        for (i, _) in outlet.iter().enumerate().filter(|(_, o)| **o) {
            out[i] = heights[i];
            heap.push(Lowest(heights[i], i));
        }
        while let Some(Lowest(h, i)) = heap.pop() {
            for n in self.neighbours(i).map(|(n, _)| n) {
                if out[n] == f64::INFINITY {
                    out[n] = heights[n].max(h + FLOOD_EPSILON);
                    heap.push(Lowest(out[n], n));
                }
            }
        }
        out
    }

    /// Routes each non-outlet to its steepest lower D8 neighbour and accumulates area.
    ///
    /// Uses the current flooded heights. Every cell contributes its own square
    /// area; processing from highest to lowest forwards the complete upstream
    /// total to each receiver. Updates [`Self::receiver`] and [`Self::area`] and
    /// returns receiver slopes in metres per metre for stream-power erosion.
    fn route(&mut self, outlet: &[bool]) -> Vec<f64> {
        let cells = self.size * self.size;
        let mut slope = vec![0.; cells];
        let mut receiver = vec![None; cells];
        for i in (0..cells).filter(|&i| !outlet[i]) {
            for (n, distance) in self.neighbours(i) {
                let drop = (self.heights[i] - self.heights[n]) / (self.cell * distance);
                if drop > slope[i] {
                    slope[i] = drop;
                    receiver[i] = Some(n);
                }
            }
        }
        self.receiver = receiver;
        // Highest first, so each cell's area is complete before it passes on.
        let mut order: Vec<usize> = (0..cells).collect();
        order.sort_by(|&a, &b| self.heights[b].total_cmp(&self.heights[a]));
        self.area = vec![self.cell * self.cell; cells];
        for i in order {
            if let Some(r) = self.receiver[i] {
                self.area[r] += self.area[i];
            }
        }
        slope
    }

    /// Smooths slopes by adding `rate` times the four-neighbour Laplacian.
    ///
    /// Reads all neighbours from the input heights so updates are simultaneous.
    /// Samples beyond the region repeat the edge height; [`Self::simulate`]
    /// resets fixed outlets after this pass.
    fn creep(&self, heights: &[f64], rate: f64) -> Vec<f64> {
        let size = self.size as isize;
        let h = |x: isize, y: isize| {
            heights[x.clamp(0, size - 1) as usize * self.size + y.clamp(0, size - 1) as usize]
        };
        (0..heights.len())
            .map(|i| {
                let (x, y) = ((i / self.size) as isize, (i % self.size) as isize);
                let laplace = h(x - 1, y) + h(x + 1, y) + h(x, y - 1) + h(x, y + 1) - 4. * h(x, y);
                heights[i] + rate * laplace
            })
            .collect()
    }

    /// Cells around `i` inside the region, with their distance in cells.
    fn neighbours(&self, i: usize) -> impl Iterator<Item = (usize, f64)> + '_ {
        let (x, y) = ((i / self.size) as isize, (i % self.size) as isize);
        NEIGHBOURS.iter().filter_map(move |&(dx, dy)| {
            let (a, b) = (x + dx, y + dy);
            let inside =
                (0..self.size as isize).contains(&a) && (0..self.size as isize).contains(&b);
            inside.then(|| {
                let distance = if dx != 0 && dy != 0 {
                    std::f64::consts::SQRT_2
                } else {
                    1.
                };
                (a as usize * self.size + b as usize, distance)
            })
        })
    }
}

/// Population standard deviation.
fn std_dev(data: &[f32]) -> f64 {
    let mean = data.iter().map(|&x| x as f64).sum::<f64>() / data.len() as f64;
    (data.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / data.len() as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use rand::{SeedableRng, rngs::StdRng};

    /// Builds a small seeded region for drainage properties without simulating a full board.
    fn small() -> Region {
        let settings = HexPathsSettings::default();
        let tuning = HexTerrainTuning {
            region_hexes: 24,
            iterations: 40,
            ..default()
        };
        Region::simulate(
            &RegionKey::new(1, &settings, &tuning),
            &mut StdRng::seed_from_u64(1),
        )
    }

    #[test]
    fn every_cell_drains_downhill_to_an_outlet() {
        let region = small();
        for start in 0..region.heights.len() {
            let (mut cell, mut steps) = (start, 0);
            while let Some(next) = region.receiver[cell] {
                assert!(region.heights[next] < region.heights[cell]);
                cell = next;
                steps += 1;
                assert!(steps <= region.heights.len());
            }
            let (x, y) = (cell / region.size, cell % region.size);
            assert!(x == 0 || y == 0 || x == region.size - 1 || y == region.size - 1);
        }
    }

    #[test]
    fn drainage_area_grows_downstream() {
        let region = small();
        for (cell, receiver) in region.receiver.iter().enumerate() {
            if let Some(r) = receiver {
                assert!(region.area[*r] > region.area[cell]);
            }
        }
    }
}
