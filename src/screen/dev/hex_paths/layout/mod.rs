//! Mutable landmarks and trails over immutable [`Terrain`](super::terrain::Terrain).
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
//! ports to reconnect. Original [`Terrain::heights`](super::terrain::Terrain::heights) never change: each layout
//! owns a newly levelled copy. [`Board`](super::data::Board) owns both layers and
//! records their measurements for the preview.

mod arrangement;
mod crossings;
mod data;
mod network;
#[cfg(test)]
mod test;

pub use arrangement::Layout;
pub use data::{Card, Context, Node, NodeKind, Pins, TrailKind};
pub use network::Pace;
/// Derives a layout seed from the fixed terrain seed and layout count.
///
/// [`relayout`](super::systems::relayout) increments the count for a successful fresh,
/// partial, or Fear layout. This mixes the count into the terrain seed without
/// changing or re-simulating the underlying [`Terrain`](super::terrain::Terrain).
pub fn regeneration_seed(seed: u64, count: u32) -> u64 {
    seed ^ (count as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}
