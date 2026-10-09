use crate::prelude::*;
use hexx::{EdgeDirection, Hex, HexOrientation, OffsetHexMode};
use rand::{Rng, RngExt};

const OFFSET_MODE: OffsetHexMode = OffsetHexMode::Even;

/// What occupies a tile.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum TileKind {
    Spawn,
    Node(NodeTrait),
    Path,
}

/// What a node reveals when the player reaches it. Dealt from the [`Deck`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum NodeTrait {
    KeyLocation,
    Npc,
    Fear,
    /// A regular node with no encounter underneath it.
    Empty,
}

/// Where the spawn tile goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum SpawnPlacement {
    Center,
    Random,
    /// Offset coordinates.
    Fixed {
        column: u32,
        row: u32,
    },
}

/// Edge connections of a tile, one bit per [`EdgeDirection`].
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect)]
pub struct Spokes(u8);

impl Spokes {
    /// Whether there is a spoke on the edge in `dir`.
    pub fn has(self, dir: EdgeDirection) -> bool {
        self.0 & (1 << dir.index()) != 0
    }

    /// Adds a spoke on the edge in `dir`.
    pub fn insert(&mut self, dir: EdgeDirection) {
        self.0 |= 1 << dir.index();
    }

    /// Number of spokes.
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Moving from a tile with these spokes to its neighbor in `dir` is legal
    /// iff both tiles have a spoke on the shared edge.
    pub fn crosses(self, to: Spokes, dir: EdgeDirection) -> bool {
        self.has(dir) && to.has(-dir)
    }
}

/// Shape of a 2-spoke tile, by the angle between its spokes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum PathShape {
    /// 60°
    Sharp,
    /// 120°
    Wide,
    /// 180°
    Straight,
}

impl PathShape {
    /// Shape of the tile entered moving in `entered` and left moving in
    /// `exited`. `None` if `exited` turns straight back.
    pub fn of_turn(entered: EdgeDirection, exited: EdgeDirection) -> Option<Self> {
        match (exited.index() + 6 - entered.index()) % 6 {
            0 => Some(Self::Straight),
            1 | 5 => Some(Self::Wide),
            2 | 4 => Some(Self::Sharp),
            _ => None,
        }
    }
}

/// Relative preference for each [`PathShape`]. Zero forbids a shape.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct ShapeWeights {
    pub sharp: f32,
    pub wide: f32,
    pub straight: f32,
}

impl ShapeWeights {
    /// Relative preference for `shape`.
    pub fn get(&self, shape: PathShape) -> f32 {
        match shape {
            PathShape::Sharp => self.sharp,
            PathShape::Wide => self.wide,
            PathShape::Straight => self.straight,
        }
    }

    fn max(&self) -> f32 {
        self.sharp.max(self.wide).max(self.straight)
    }

    /// Extra path cost for bending into `shape`: zero for the most preferred
    /// shape, `None` for a shape with zero weight.
    pub fn turn_cost(&self, shape: PathShape) -> Option<f32> {
        let weight = self.get(shape);
        (weight > 0.).then(|| self.max() / weight - 1.)
    }
}

/// Node trait counts. Node count is the deck size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub struct Deck {
    pub key_locations: u32,
    pub npcs: u32,
    pub fears: u32,
    pub empty: u32,
}

impl Deck {
    /// Every card in the deck, unshuffled.
    pub fn cards(&self) -> Vec<NodeTrait> {
        [
            (NodeTrait::KeyLocation, self.key_locations),
            (NodeTrait::Npc, self.npcs),
            (NodeTrait::Fear, self.fears),
            (NodeTrait::Empty, self.empty),
        ]
        .into_iter()
        .flat_map(|(card, count)| std::iter::repeat_n(card, count as usize))
        .collect()
    }
}

/// Board generation settings. Changing these regenerates the board.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexPathsSettings {
    pub columns: u32,
    pub rows: u32,
    /// `None` draws a fresh seed every generation.
    pub seed: Option<u64>,
    pub spawn: SpawnPlacement,
    pub deck: Deck,
    pub min_node_distance: u32,
    pub min_spawn_distance: u32,
    pub min_spawn_spokes: u32,
    pub max_spawn_spokes: u32,
    /// Amplitude of the per-cell random path cost. 0 gives shortest paths.
    pub noise: f32,
    pub shape_weights: ShapeWeights,
    /// Chance a 2-spoke path tile is upgraded to 3 spokes.
    pub junction_chance: f32,
    pub min_branch_length: u32,
    pub max_branch_length: u32,
    /// Chance a branch connects into an adjacent tile instead of dead-ending.
    pub branch_join_chance: f32,
}

impl Default for HexPathsSettings {
    fn default() -> Self {
        Self {
            columns: 10,
            rows: 8,
            seed: None,
            spawn: SpawnPlacement::Center,
            deck: Deck {
                key_locations: 1,
                npcs: 1,
                fears: 1,
                empty: 3,
            },
            min_node_distance: 2,
            min_spawn_distance: 3,
            min_spawn_spokes: 2,
            max_spawn_spokes: 3,
            noise: 2.,
            shape_weights: ShapeWeights {
                sharp: 1.,
                wide: 1.,
                straight: 1.,
            },
            junction_chance: 0.25,
            min_branch_length: 1,
            max_branch_length: 4,
            branch_join_chance: 0.3,
        }
    }
}

impl HexPathsSettings {
    /// The cell at offset coordinates `column`, `row`.
    pub fn cell(&self, column: u32, row: u32) -> Hex {
        Hex::from_offset_coordinates(
            [column as i32, row as i32],
            OFFSET_MODE,
            HexOrientation::Flat,
        )
    }

    /// Whether `hex` is inside the grid.
    pub fn contains(&self, hex: Hex) -> bool {
        let [column, row] = hex.to_offset_coordinates(OFFSET_MODE, HexOrientation::Flat);
        (0..self.columns as i32).contains(&column) && (0..self.rows as i32).contains(&row)
    }

    /// All grid cells, column by column.
    pub fn cells(&self) -> impl Iterator<Item = Hex> + '_ {
        (0..self.columns)
            .flat_map(move |column| (0..self.rows).map(move |row| self.cell(column, row)))
    }

    /// The spawn cell according to [`Self::spawn`].
    pub fn spawn_cell(&self, rng: &mut impl Rng) -> Hex {
        match self.spawn {
            SpawnPlacement::Center => self.cell(self.columns / 2, self.rows / 2),
            SpawnPlacement::Random => self.cell(
                rng.random_range(0..self.columns),
                rng.random_range(0..self.rows),
            ),
            SpawnPlacement::Fixed { column, row } => self.cell(column, row),
        }
    }

    /// Validates invariants on HexPathsSettings. Returns the first violated
    /// invariant.
    pub fn validate(&self) -> Result<(), &'static str> {
        let weights = self.shape_weights;
        let chance = 0. ..=1.;
        [
            (self.columns > 0 && self.rows > 0, "grid must not be empty"),
            (
                !matches!(self.spawn, SpawnPlacement::Fixed { column, row }
                    if column >= self.columns || row >= self.rows),
                "fixed spawn must be inside the grid",
            ),
            (
                self.min_spawn_spokes <= self.max_spawn_spokes && self.max_spawn_spokes <= 6,
                "spawn spokes must satisfy min <= max <= 6",
            ),
            (
                self.min_branch_length <= self.max_branch_length,
                "branch length must satisfy min <= max",
            ),
            (
                (0. ..=10.).contains(&self.noise),
                "noise must be within 0..=10",
            ),
            (
                [weights.sharp, weights.wide, weights.straight]
                    .iter()
                    .all(|w| (0. ..=10.).contains(w))
                    && weights.max() > 0.,
                "shape weights must be within 0..=10 with at least one positive",
            ),
            (
                chance.contains(&self.junction_chance) && chance.contains(&self.branch_join_chance),
                "chances must be within 0..=1",
            ),
        ]
        .into_iter()
        .find(|(ok, _)| !ok)
        .map_or(Ok(()), |(_, reason)| Err(reason))
    }
}
