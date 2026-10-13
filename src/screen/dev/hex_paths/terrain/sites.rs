use super::{HexMesh, Raster, SQRT_3};
use crate::prelude::*;
use rand::{Rng, RngExt};

/// How buildable each tile is: a low height range over it and its
/// neighbours, as `exp(-range / median playable range)`.
pub fn suitability(mesh: &HexMesh, heights: &[f32]) -> Vec<f32> {
    let ranges: Vec<f32> = (0..mesh.tiles.len())
        .map(|tile| {
            let (low, high) = std::iter::once(tile)
                .chain(mesh.neighbours(tile))
                .flat_map(|t| mesh.tile_corners[t])
                .map(|c| heights[c])
                .fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(h), hi.max(h)));
            high - low
        })
        .collect();
    let playable: Vec<f32> = ranges
        .iter()
        .zip(&mesh.playable)
        .filter(|(_, playable)| **playable)
        .map(|(range, _)| *range)
        .collect();
    let median = median(playable).max(f32::EPSILON);
    ranges.iter().map(|range| (-range / median).exp()).collect()
}

/// Per playable tile, over tiles within `radius` hexes seen from `eye` above
/// the ground: the fraction visible (open, not enclosed), and the fraction
/// visible *and* lower, up to one edge length (an overlook, not just a
/// plain). Looks past the board's edge. Zero for margin tiles.
pub fn views(mesh: &HexMesh, raster: &Raster, radius: f32, eye: f32) -> (Vec<f32>, Vec<f32>) {
    let ground: Vec<f32> = mesh.centers.iter().map(|&c| raster.sample(c)).collect();
    let mut open = vec![0.; mesh.tiles.len()];
    let mut overlook = vec![0.; mesh.tiles.len()];
    for tile in (0..mesh.tiles.len()).filter(|&t| mesh.playable[t]) {
        let (from, eye) = (mesh.centers[tile], ground[tile] + eye);
        let (mut count, mut seen_count, mut lower) = (0., 0., 0.);
        for other in (0..mesh.tiles.len()).filter(|&o| {
            o != tile && mesh.centers[o].distance(from) <= radius * SQRT_3
        }) {
            let to = mesh.centers[other];
            // Sight line to the ground at the other tile's centre.
            let seen = (1..17).map(|k| k as f32 / 17.).all(|f| {
                raster.sample(from + (to - from) * f) <= eye + (ground[other] - eye) * f + 1e-6
            });
            count += 1.;
            if seen {
                seen_count += 1.;
                lower += (ground[tile] - ground[other]).clamp(0., 1.);
            }
        }
        if count > 0. {
            open[tile] = seen_count / count;
            overlook[tile] = lower / count;
        }
    }
    (open, overlook)
}

/// Each value's rank among the tiles in `among`, scaled to 0..=1; zero
/// elsewhere. Ranking only playable tiles keeps the margin, which has no
/// views, from crowding the bottom of the scale.
pub fn rank(values: &[f32], among: &[bool]) -> Vec<f32> {
    let mut order: Vec<usize> = (0..values.len()).filter(|&i| among[i]).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let scale = (order.len().max(2) - 1) as f32;
    let mut ranks = vec![0.; values.len()];
    for (rank, index) in order.into_iter().enumerate() {
        ranks[index] = rank as f32 / scale;
    }
    ranks
}

/// Weighted farthest-point sampling: repeatedly takes the tile maximising
/// distance-to-nearest-pad × score. The playable edge counts as a pad at half
/// the distance, so sites don't crowd the border. Sites keep `spacing` hexes
/// apart while they can, then crowd in: this always places `count`.
pub fn farthest(
    mesh: &HexMesh,
    taken: &[usize],
    score: &[f32],
    count: usize,
    spacing: f32,
    rng: &mut impl Rng,
) -> Vec<usize> {
    let eligible = mesh.interior();
    let mut picks: Vec<usize> = Vec::new();
    for _ in 0..count {
        let distance: Vec<f32> = (0..mesh.tiles.len())
            .map(|t| {
                taken
                    .iter()
                    .chain(&picks)
                    .map(|&p| mesh.centers[t].distance(mesh.centers[p]) / SQRT_3)
                    .fold(f32::INFINITY, f32::min)
                    .min(2. * mesh.border_distance[t])
            })
            .collect();
        let jitter: Vec<f32> = distance.iter().map(|_| rng.random_range(0.9..1.0)).collect();
        let best = |spaced: bool| {
            (0..mesh.tiles.len())
                .filter(|&t| eligible[t] && (!spaced || distance[t] >= spacing))
                .max_by(|&a, &b| {
                    let s = |t: usize| distance[t] * score[t] * if spaced { jitter[t] } else { 1. };
                    s(a).total_cmp(&s(b))
                })
        };
        if let Some(pick) = best(true).or_else(|| best(false)) {
            picks.push(pick);
        }
    }
    picks
}

/// Middle value; the mean of the middle two for an even count.
fn median(mut values: Vec<f32>) -> f32 {
    values.sort_by(f32::total_cmp);
    match values.len() {
        0 => 0.,
        n if n % 2 == 1 => values[n / 2],
        n => (values[n / 2 - 1] + values[n / 2]) / 2.,
    }
}
