use super::data::*;
use bevy::platform::collections::HashMap;
use hexx::{EdgeDirection, Hex};
use rand::{
    Rng, RngExt,
    seq::{IndexedRandom, SliceRandom},
};
use std::{cmp::Reverse, collections::BinaryHeap};

/// Whole-board retries before giving up on placement or routing.
const MAX_ATTEMPTS: u32 = 100;
/// Path costs are fixed-point so they can be ordered in a heap.
const COST_SCALE: f32 = 100.;

/// Why a board could not be generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardGenError {
    /// The settings violate an invariant; see [`HexPathsSettings::validate`].
    InvalidSettings(&'static str),
    /// The nodes do not fit with the requested spacing.
    Placement,
    /// Some node could not be connected to spawn.
    Unreachable,
}

/// Generator output. Cells without a tile are empty.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardLayout {
    pub tiles: HashMap<Hex, LayoutTile>,
    pub spawn: Hex,
    /// Generation attempts it took to produce this board.
    pub attempts: u32,
}

impl BoardLayout {
    /// Generates a board on which spawn and every node are connected.
    pub fn generate(
        settings: &HexPathsSettings,
        rng: &mut impl Rng,
    ) -> Result<Self, BoardGenError> {
        settings
            .validate()
            .map_err(BoardGenError::InvalidSettings)?;
        let mut last_error = BoardGenError::Unreachable;
        for attempts in 1..=MAX_ATTEMPTS {
            match Generator::new(settings, rng).run() {
                Ok(layout) => return Ok(Self { attempts, ..layout }),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
}

/// A generated tile, before it is spawned as components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutTile {
    pub kind: TileKind,
    pub spokes: Spokes,
}

impl From<TileKind> for LayoutTile {
    fn from(kind: TileKind) -> Self {
        Self {
            kind,
            spokes: Spokes::default(),
        }
    }
}

/// One generation attempt.
struct Generator<'a, R: Rng> {
    settings: &'a HexPathsSettings,
    rng: &'a mut R,
    spawn: Hex,
    tiles: HashMap<Hex, LayoutTile>,
}

impl<'a, R: Rng> Generator<'a, R> {
    /// Starts a board containing only spawn.
    fn new(settings: &'a HexPathsSettings, rng: &'a mut R) -> Self {
        let spawn = settings.spawn_cell(rng);
        Self {
            settings,
            rng,
            spawn,
            tiles: HashMap::from([(spawn, TileKind::Spawn.into())]),
        }
    }

    /// Places and connects the nodes, then decorates the network.
    fn run(mut self) -> Result<BoardLayout, BoardGenError> {
        let nodes = self.place_nodes()?;
        self.backbone(&nodes)?;
        self.spawn_minimum();
        self.decorate();
        Ok(BoardLayout {
            tiles: self.tiles,
            spawn: self.spawn,
            attempts: 0,
        })
    }

    /// Places nodes with the configured spacing and deals the deck onto them.
    /// Returns them in random order.
    fn place_nodes(&mut self) -> Result<Vec<Hex>, BoardGenError> {
        let s = self.settings;
        let mut cards = s.deck.cards();
        cards.shuffle(self.rng);
        let mut cells: Vec<Hex> = s.cells().collect();
        cells.shuffle(self.rng);
        let nodes = cells
            .into_iter()
            .filter(|cell| cell.unsigned_distance_to(self.spawn) >= s.min_spawn_distance.max(1))
            .fold(Vec::new(), |mut nodes, cell| {
                if nodes.len() < cards.len()
                    && nodes.iter().all(|node: &Hex| {
                        node.unsigned_distance_to(cell) >= s.min_node_distance.max(1)
                    })
                {
                    nodes.push(cell);
                }
                nodes
            });
        if nodes.len() < cards.len() {
            return Err(BoardGenError::Placement);
        }
        self.tiles.extend(
            nodes
                .iter()
                .zip(cards)
                .map(|(&node, card)| (node, TileKind::Node(card).into())),
        );
        Ok(nodes)
    }

    /// Connects each node to the network along a noisy cheapest route.
    fn backbone(&mut self, nodes: &[Hex]) -> Result<(), BoardGenError> {
        let s = self.settings;
        let costs: HashMap<Hex, u32> = s
            .cells()
            .map(|cell| (cell, to_cost(1. + s.noise * self.rng.random::<f32>())))
            .collect();
        for &node in nodes {
            let route = self.route(node, &costs).ok_or(BoardGenError::Unreachable)?;
            for pair in route.windows(2) {
                let dir = pair[0]
                    .neighbor_direction(pair[1])
                    .expect("route steps are adjacent");
                self.connect(pair[0], dir);
            }
        }
        Ok(())
    }

    /// Cheapest route from `start` through empty cells to a joinable tile.
    /// Search state includes the entry direction so bends can be priced by
    /// shape. No heuristic: boards are small, and there are many goals.
    /// Routes never revisit a cell, which would give it more than 2 spokes;
    /// that pruning can miss a route, which the caller retries.
    fn route(&self, start: Hex, costs: &HashMap<Hex, u32>) -> Option<Vec<Hex>> {
        type State = (Hex, Option<EdgeDirection>);
        let mut states: Vec<State> = vec![(start, None)];
        let mut came_from: Vec<Option<usize>> = vec![None];
        let mut best: HashMap<State, u32> = HashMap::from([((start, None), 0)]);
        let mut open = BinaryHeap::from([Reverse((0, 0))]);

        while let Some(Reverse((cost, index))) = open.pop() {
            let (hex, entered) = states[index];
            if best[&(hex, entered)] < cost {
                continue;
            }
            if hex != start && self.tiles.contains_key(&hex) {
                let route = std::iter::successors(Some(index), |&i| came_from[i])
                    .map(|i| states[i].0)
                    .collect();
                return Some(route);
            }
            for exit in EdgeDirection::ALL_DIRECTIONS {
                let next = hex.neighbor(exit);
                let enterable = match self.tiles.contains_key(&next) {
                    false => self.settings.contains(next),
                    true => next != start && self.joinable(next),
                };
                // Only path tiles have a shape; leaving the start node is free.
                let turn = match entered {
                    None => Some(0),
                    Some(entered) => PathShape::of_turn(entered, exit)
                        .and_then(|shape| self.settings.shape_weights.turn_cost(shape))
                        .map(to_cost),
                };
                let revisits = std::iter::successors(Some(index), |&i| came_from[i])
                    .any(|i| states[i].0 == next);
                let (true, Some(turn), false) = (enterable, turn, revisits) else {
                    continue;
                };
                let state = (next, Some(exit));
                // Saturate: an extreme weight ratio may produce huge turn costs.
                let next_cost = cost.saturating_add(turn).saturating_add(costs[&next]);
                if best.get(&state).is_some_and(|&known| known <= next_cost) {
                    continue;
                }
                best.insert(state, next_cost);
                states.push(state);
                came_from.push(Some(index));
                open.push(Reverse((next_cost, states.len() - 1)));
            }
        }
        None
    }

    /// Adds spokes to spawn until it reaches its minimum.
    fn spawn_minimum(&mut self) {
        let spawn = self.spawn;
        while self.tiles[&spawn].spokes.count() < self.settings.min_spawn_spokes {
            let spokes = self.tiles[&spawn].spokes;
            let free: Vec<_> = EdgeDirection::ALL_DIRECTIONS
                .into_iter()
                .filter(|&dir| !spokes.has(dir))
                .collect();
            let &dir = free
                .choose(self.rng)
                .expect("validated min_spawn_spokes <= 6");
            self.branch(spawn, dir);
        }
    }

    /// Upgrades some 2-spoke path tiles to junctions, each growing a branch.
    fn decorate(&mut self) {
        let s = self.settings;
        for hex in s.cells() {
            let Some(&tile) = self.tiles.get(&hex) else {
                continue;
            };
            if tile.kind != TileKind::Path
                || tile.spokes.count() != 2
                || !self.rng.random_bool(s.junction_chance as f64)
            {
                continue;
            }
            let open: Vec<_> = EdgeDirection::ALL_DIRECTIONS
                .into_iter()
                .filter(|&dir| !tile.spokes.has(dir) && self.is_empty(hex.neighbor(dir)))
                .collect();
            if let Some(&dir) = open.choose(self.rng) {
                self.branch(hex, dir);
            }
        }
    }

    /// Grows a random walk from `origin`, leaving it in `first`. The walk ends
    /// joined into an adjacent tile or in a dangling spoke.
    fn branch(&mut self, origin: Hex, first: EdgeDirection) {
        let s = self.settings;
        let length = self
            .rng
            .random_range(s.min_branch_length..=s.max_branch_length);
        let (mut at, mut entered) = (origin, None);
        for _ in 0..length {
            let exit = match entered {
                None => first,
                Some(entered) => match self.pick_turn(at, entered, Self::is_empty) {
                    Some(exit) => exit,
                    None => break,
                },
            };
            if !self.is_empty(at.neighbor(exit)) {
                break;
            }
            self.connect(at, exit);
            (at, entered) = (at.neighbor(exit), Some(exit));
        }

        let Some(entered) = entered else {
            self.spoke(origin, first);
            return;
        };
        let join = match self.rng.random_bool(s.branch_join_chance as f64) {
            true => self.pick_turn(at, entered, Self::joinable),
            false => None,
        };
        match join {
            Some(exit) => self.connect(at, exit),
            None => {
                if let Some(exit) = self.pick_turn(at, entered, |_, _| true) {
                    self.spoke(at, exit);
                }
            }
        }
    }

    /// Picks an exit from `at`, weighted by the resulting path shape, among
    /// neighbors that pass `allowed`.
    fn pick_turn(
        &mut self,
        at: Hex,
        entered: EdgeDirection,
        allowed: impl Fn(&Self, Hex) -> bool,
    ) -> Option<EdgeDirection> {
        let options: Vec<(EdgeDirection, f32)> = EdgeDirection::ALL_DIRECTIONS
            .into_iter()
            .filter(|&exit| allowed(self, at.neighbor(exit)))
            .filter_map(|exit| {
                PathShape::of_turn(entered, exit)
                    .map(|shape| (exit, self.settings.shape_weights.get(shape)))
            })
            .collect();
        options
            .choose_weighted(self.rng, |(_, weight)| *weight)
            .ok()
            .map(|&(exit, _)| exit)
    }

    fn is_empty(&self, hex: Hex) -> bool {
        self.settings.contains(hex) && !self.tiles.contains_key(&hex)
    }

    /// Whether a path may connect into this tile without exceeding its cap.
    fn joinable(&self, hex: Hex) -> bool {
        self.tiles.get(&hex).is_some_and(|tile| match tile.kind {
            TileKind::Spawn => tile.spokes.count() < self.settings.max_spawn_spokes,
            // Unconnected nodes are not part of the network yet.
            TileKind::Node(_) => tile.spokes.count() > 0,
            TileKind::Path => tile.spokes.count() < 3,
        })
    }

    /// Adds matching spokes between `from` and its neighbor in `dir`.
    fn connect(&mut self, from: Hex, dir: EdgeDirection) {
        self.spoke(from, dir);
        self.spoke(from.neighbor(dir), -dir);
    }

    /// Adds one spoke, creating a path tile if the cell was empty.
    fn spoke(&mut self, hex: Hex, dir: EdgeDirection) {
        self.tiles
            .entry(hex)
            .or_insert(TileKind::Path.into())
            .spokes
            .insert(dir);
    }
}

fn to_cost(cost: f32) -> u32 {
    (cost * COST_SCALE).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use bevy::platform::collections::HashSet;
    use rand::{SeedableRng, rngs::StdRng};

    fn variants() -> [HexPathsSettings; 4] {
        [
            HexPathsSettings::default(),
            HexPathsSettings {
                junction_chance: 1.,
                branch_join_chance: 1.,
                ..default()
            },
            HexPathsSettings {
                spawn: SpawnPlacement::Random,
                noise: 0.,
                ..default()
            },
            HexPathsSettings {
                shape_weights: ShapeWeights {
                    sharp: 1.,
                    wide: 0.,
                    straight: 0.,
                },
                min_branch_length: 0,
                min_spawn_spokes: 4,
                max_spawn_spokes: 6,
                ..default()
            },
        ]
    }

    fn boards() -> impl Iterator<Item = (HexPathsSettings, BoardLayout)> {
        variants().into_iter().flat_map(|settings| {
            (0..100).map(move |seed| {
                let layout = BoardLayout::generate(&settings, &mut StdRng::seed_from_u64(seed))
                    .unwrap_or_else(|error| panic!("seed {seed}: {error:?}"));
                (settings.clone(), layout)
            })
        })
    }

    fn nodes(layout: &BoardLayout) -> impl Iterator<Item = (Hex, NodeTrait)> + '_ {
        layout
            .tiles
            .iter()
            .filter_map(|(&hex, tile)| match tile.kind {
                TileKind::Node(card) => Some((hex, card)),
                _ => None,
            })
    }

    fn reachable(layout: &BoardLayout) -> HashSet<Hex> {
        let mut seen = HashSet::from([layout.spawn]);
        let mut frontier = vec![layout.spawn];
        while let Some(hex) = frontier.pop() {
            for dir in EdgeDirection::ALL_DIRECTIONS {
                let next = hex.neighbor(dir);
                let Some(to) = layout.tiles.get(&next) else {
                    continue;
                };
                if layout.tiles[&hex].spokes.crosses(to.spokes, dir) && seen.insert(next) {
                    frontier.push(next);
                }
            }
        }
        seen
    }

    #[test]
    fn spawn_reaches_every_node() {
        for (_, layout) in boards() {
            let reachable = reachable(&layout);
            assert!(nodes(&layout).all(|(node, _)| reachable.contains(&node)));
        }
    }

    #[test]
    fn spoke_counts_within_caps() {
        for (settings, layout) in boards() {
            for tile in layout.tiles.values() {
                let count = tile.spokes.count();
                match tile.kind {
                    TileKind::Path => assert!((2..=3).contains(&count)),
                    TileKind::Spawn => assert!(
                        (settings.min_spawn_spokes..=settings.max_spawn_spokes).contains(&count)
                    ),
                    TileKind::Node(_) => {}
                }
            }
        }
    }

    #[test]
    fn nodes_respect_spacing() {
        for (settings, layout) in boards() {
            let nodes: Vec<_> = nodes(&layout).map(|(hex, _)| hex).collect();
            for (i, a) in nodes.iter().enumerate() {
                assert!(a.unsigned_distance_to(layout.spawn) >= settings.min_spawn_distance);
                for b in &nodes[i + 1..] {
                    assert!(a.unsigned_distance_to(*b) >= settings.min_node_distance);
                }
            }
        }
    }

    #[test]
    fn deck_is_dealt_exactly() {
        for (settings, layout) in boards() {
            let cards = settings.deck.cards();
            let dealt: Vec<_> = nodes(&layout).map(|(_, card)| card).collect();
            for card in [
                NodeTrait::KeyLocation,
                NodeTrait::Npc,
                NodeTrait::Fear,
                NodeTrait::Empty,
            ] {
                let count = |cards: &[NodeTrait]| cards.iter().filter(|c| **c == card).count();
                assert_eq!(count(&dealt), count(&cards));
            }
        }
    }

    #[test]
    fn same_seed_same_board() {
        let settings = HexPathsSettings::default();
        let generate = || BoardLayout::generate(&settings, &mut StdRng::seed_from_u64(7));
        assert_eq!(generate(), generate());
    }

    #[test]
    fn decoration_adds_a_third_spoke() {
        let settings = HexPathsSettings {
            columns: 3,
            rows: 3,
            junction_chance: 1.,
            min_branch_length: 0,
            max_branch_length: 0,
            ..default()
        };
        for seed in 0..32 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut generator = Generator::new(&settings, &mut rng);
            let mut tile = LayoutTile::from(TileKind::Path);
            for dir in EdgeDirection::ALL_DIRECTIONS.into_iter().take(2) {
                tile.spokes.insert(dir);
            }
            generator.tiles.insert(generator.spawn, tile);

            generator.decorate();

            assert_eq!(
                generator.tiles[&generator.spawn].spokes.count(),
                3,
                "seed {seed}"
            );
        }
    }

    #[test]
    fn impossible_settings_fail() {
        let crowded = HexPathsSettings {
            columns: 3,
            rows: 3,
            min_spawn_distance: 3,
            ..default()
        };
        let inverted = HexPathsSettings {
            min_branch_length: 5,
            max_branch_length: 4,
            ..default()
        };
        let rng = &mut StdRng::seed_from_u64(0);
        assert_eq!(
            BoardLayout::generate(&crowded, rng),
            Err(BoardGenError::Placement)
        );
        assert!(matches!(
            BoardLayout::generate(&inverted, rng),
            Err(BoardGenError::InvalidSettings(_))
        ));
        let excessive_spawn_spokes = HexPathsSettings {
            max_spawn_spokes: 7,
            ..default()
        };
        assert!(matches!(
            BoardLayout::generate(&excessive_spawn_spokes, rng),
            Err(BoardGenError::InvalidSettings(_))
        ));
    }
}
