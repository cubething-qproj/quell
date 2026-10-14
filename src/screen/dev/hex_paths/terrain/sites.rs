//! Scores landscape sites and chooses tiles for layout nodes.
//!
//! [`Terrain::generate`](super::Terrain::generate) uses [`suitability`] for buildability and
//! [`views`] for openness and downhill views. [`rank`] compares playable tiles
//! so the terrain can sharpen and blend quiet and overlook bench scores.
//!
//! [`Layout`](super::super::layout::Layout) combines those scores with water proximity
//! and uses [`place`] to spread nodes among allowed interior tiles.
//! [`Constraints`] separates hard reachability exclusions from spacing and
//! regeneration-distance preferences, which relax when no preferred tile fits.

use super::{HexMesh, Raster, SQRT_3};
use crate::prelude::*;
use rand::{Rng, RngExt};

/// Scores buildability from height variation over each tile and its neighbours.
///
/// Returns `exp(-range / median playable range)` per tile: flatter ground
/// scores higher. `heights` follows [`HexMesh::positions`]. The output includes
/// margin tiles, but only playable ranges set the normalising median.
/// [`Terrain::generate`](super::Terrain::generate) stores it for POI placement and the preview.
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

/// Measures how open each playable tile is, and how much lower ground it sees.
///
/// Returns `(open, overlook)`, with one entry per [`HexMesh::tiles`] index.
/// For a viewer at a playable tile centre, consider other tile centres within
/// `radius` hex spacings, including centres in the scenery margin. A centre
/// is visible if sampled ground stays below the line from the viewer's eye
/// to the ground at that centre. `open` is the visible fraction of all those
/// centres. `overlook` averages their visible downward drops: hidden or higher
/// centres contribute zero, and a visible centre at least `drop` lower contributes
/// one. A tile seeing only level ground can have high openness but no overlook.
///
/// `radius` is converted to planar edge lengths by multiplying by √3; `eye`
/// and positive `drop` are already in edge lengths. Both scores are zero for
/// margin viewers or viewers with no neighbours in range. [`Terrain`](super::Terrain)
/// ranks `1 - open` as quiet and `overlook` as a view for bench placement.
/// This uses the original [`Raster`]; layout landmark sight uses levelled pads.
pub fn views(mesh: &HexMesh, raster: &Raster, radius: f32, eye: f32, drop: f32) -> (Vec<f32>, Vec<f32>) {
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
                lower += ((ground[tile] - ground[other]) / drop).clamp(0., 1.);
            }
        }
        if count > 0. {
            open[tile] = seen_count / count;
            overlook[tile] = lower / count;
        }
    }
    (open, overlook)
}

/// Ranks selected values from lowest to highest, scaling their positions to 0–1.
///
/// `among` selects the tiles to compare; unselected entries stay zero. Equal
/// values retain input order and receive distinct ordinal ranks. A single
/// selected tile receives zero. [`Terrain::generate`](super::Terrain::generate) selects playable
/// tiles so zero-valued scenery viewers do not distort bench ranks.
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

/// Placement restrictions passed to [`place`] by layout node generation.
///
/// [`Self::forbid`] is a hard per-tile exclusion, typically from creek
/// reachability. [`Self::room`] and [`Self::near`] express preferred bench
/// spacing and movement distance; they can relax when no preferred tile fits.
/// All tile indices refer to the same [`HexMesh`] used by the layout.
#[derive(Default)]
pub struct Constraints<'a> {
    /// Tiles never used.
    pub forbid: Option<&'a [bool]>,
    /// Tiles used only while possible (bench spacing).
    pub room: Option<&'a [bool]>,
    /// Only tiles in this annulus round a tile (min, max hexes), while
    /// possible.
    pub near: Option<(usize, Vec2)>,
}

/// Chooses an unoccupied interior tile by weighted farthest-point sampling.
///
/// Maximises separation × site score × seeded jitter. Separation is the
/// nearest occupied-pad distance capped at twice the border distance, all in
/// hex spacings. This makes the border repel nodes less strongly than a pad.
///
/// Tries spacing, [`Constraints::room`], and [`Constraints::near`] first; then
/// relaxes spacing and room, and finally the movement annulus. Interior and
/// [`Constraints::forbid`] exclusions never relax. Returns `None` when no
/// unoccupied allowed tile remains. `score`, masks, and `taken` indices must
/// match `mesh`; zero scores do not by themselves forbid a tile.
pub fn place(
    mesh: &HexMesh,
    taken: &[usize],
    score: &[f32],
    spacing: f32,
    constraints: &Constraints,
    rng: &mut impl Rng,
) -> Option<usize> {
    let interior = mesh.interior();
    let allowed = |t: usize| interior[t] && !constraints.forbid.is_some_and(|f| f[t]);
    let near = |t: usize| {
        constraints.near.is_none_or(|(tile, range)| {
            let apart = mesh.centers[t].distance(mesh.centers[tile]) / SQRT_3;
            (range.x..=range.y).contains(&apart)
        })
    };
    let distance: Vec<f32> = (0..mesh.tiles.len())
        .map(|t| {
            taken
                .iter()
                .map(|&p| mesh.centers[t].distance(mesh.centers[p]) / SQRT_3)
                .fold(f32::INFINITY, f32::min)
                .min(2. * mesh.border_distance[t])
        })
        .collect();
    let jitter: Vec<f32> = distance.iter().map(|_| rng.random_range(0.9..1.0)).collect();
    let spaced = |t: usize| distance[t] >= spacing && constraints.room.is_none_or(|r| r[t]);
    let free = |t: usize| distance[t] > 0.;
    let tiers: [&dyn Fn(usize) -> bool; 3] = [
        &|t| allowed(t) && near(t) && spaced(t) && free(t),
        &|t| allowed(t) && near(t) && free(t),
        &|t| allowed(t) && free(t),
    ];
    tiers.iter().find_map(|tier| {
        (0..mesh.tiles.len())
            .filter(|&t| tier(t))
            .max_by(|&a, &b| {
                let s = |t: usize| distance[t] * score[t] * jitter[t];
                s(a).total_cmp(&s(b))
            })
    })
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
