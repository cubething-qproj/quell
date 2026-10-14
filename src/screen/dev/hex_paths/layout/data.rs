//! Node identities, trail roles, pins, and borrowed generation context.
//!
//! [`Layout`](super::Layout) consumes these values while building arrangements over [`Terrain`].
//! Pins carry existing node identities and trail edges into regeneration;
//! Context supplies the ground and knobs without depending on a Bevy world.

use crate::prelude::*;
use crate::screen::dev::hex_paths::{data::*, terrain::Terrain};
use std::collections::BTreeSet;

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
/// POIs have no card. [`Layout::regenerate`](super::Layout::regenerate) retains each node's card through
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

/// A landmark whose identity, kind, and card survive [`Layout::regenerate`](super::Layout::regenerate).
///
/// Placement chooses its tile using terrain scores and crossing reachability.
/// [`Pins`] carries the existing value into regeneration, retaining fixed
/// nodes and relocating movable ones. Crossing nodes always remain fixed.
/// Array position is not identity: point indices can change when a node
/// becomes [`Layout::player_node`](super::Layout::player_node), while [`Self::id`] stays the same.
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

/// The pass that first added an edge to [`Layout::trail`](super::Layout::trail).
///
/// Used by [`HexPathsAppearance::trail`](super::super::data::HexPathsAppearance::trail) to colour the preview.
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

/// Inputs that preserve part of a [`Layout`](super::Layout) while rebuilding its surroundings.
///
/// [`Layout::regenerate`](super::Layout::regenerate) constructs these from the old layout and player tile,
/// then passes them to [`Layout::generate`](super::Layout::generate). The player becomes spawn; the node
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

/// Borrowed ground and settings for the pure [`Layout`](super::Layout) algorithms.
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
    pub(super) fn creeks_matter(&self) -> bool {
        self.settings.crossings > 0
    }
}
