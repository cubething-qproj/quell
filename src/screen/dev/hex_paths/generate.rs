use super::data::*;
use bevy::platform::collections::{HashMap, HashSet};
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
/// Passes over the network while spending the path budget, and attempts at
/// the spawn minimum, before an attempt counts as stuck.
const MAX_SWEEPS: u32 = 50;

/// Why a board could not be generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardGenError {
    /// The settings violate an invariant; see [`HexPathsSettings::validate`].
    InvalidSettings(&'static str),
    /// The nodes do not fit with the requested spacing.
    Placement,
    /// Some node could not be connected to spawn.
    Unreachable,
    /// Some node could not be given a second, far-reaching way out.
    Unloopable,
    /// Connecting the nodes alone needs more path tiles than the target allows.
    OverBudget,
    /// Growth was boxed in before reaching the target, or before spawn got
    /// its minimum spokes.
    Stuck,
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

    /// Places and connects the nodes, then branches out until the path count
    /// reaches a target drawn from the window.
    fn run(mut self) -> Result<BoardLayout, BoardGenError> {
        let window = self.settings.target_window();
        let nodes = self.place_nodes()?;
        self.backbone(&nodes)?;
        self.spawn_minimum()?;
        if self.paths() > *window.end() {
            return Err(BoardGenError::OverBudget);
        }
        let target = self.rng.random_range(window.clone());
        self.spend(target);
        if !window.contains(&self.paths()) {
            return Err(BoardGenError::Stuck);
        }
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
            let route = self
                .route(node, &costs, |_, _| true)
                .ok_or(BoardGenError::Unreachable)?;
            self.commit(&route);
        }
        if self.settings.node_loops {
            self.loop_nodes(nodes, &costs)?;
        }
        Ok(())
    }

    /// Gives every node not already on a loop, in random order, a second route
    /// out, which must rejoin the network at least `min_loop` tiles around it.
    fn loop_nodes(
        &mut self,
        nodes: &[Hex],
        costs: &HashMap<Hex, u32>,
    ) -> Result<(), BoardGenError> {
        let least = self.settings.min_loop.max(3) as usize;
        let mut order = nodes.to_vec();
        order.shuffle(self.rng);
        for node in order {
            if self.looped(node) {
                continue;
            }
            let around = self.distances(node);
            let far = |hex: Hex, steps: usize| around.get(&hex).is_none_or(|&d| d + steps >= least);
            let route = self
                .route(node, costs, far)
                .ok_or(BoardGenError::Unloopable)?;
            self.commit(&route);
        }
        Ok(())
    }

    /// Adds mutual spokes along a route of adjacent cells.
    fn commit(&mut self, route: &[Hex]) {
        for pair in route.windows(2) {
            let dir = pair[0]
                .neighbor_direction(pair[1])
                .expect("route steps are adjacent");
            self.connect(pair[0], dir);
        }
    }

    /// Cheapest route from `start` through empty cells to a joinable tile
    /// that `accept`s the route's step count. Search state includes the entry
    /// direction so bends can be priced by shape. No heuristic: boards are
    /// small, and there are many goals. Routes never revisit a cell, which
    /// would give it more than 2 spokes; that pruning can miss a route, which
    /// the caller retries.
    fn route(
        &self,
        start: Hex,
        costs: &HashMap<Hex, u32>,
        accept: impl Fn(Hex, usize) -> bool,
    ) -> Option<Vec<Hex>> {
        type State = (Hex, Option<EdgeDirection>);
        let mut states: Vec<State> = vec![(start, None)];
        let mut came_from: Vec<Option<usize>> = vec![None];
        let mut depth: Vec<usize> = vec![0];
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
                    true => next != start && self.joinable(next) && accept(next, depth[index] + 1),
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
                depth.push(depth[index] + 1);
                open.push(Reverse((next_cost, states.len() - 1)));
            }
        }
        None
    }

    /// Adds spokes to spawn until it reaches its minimum. Fails only when
    /// branches must close loops and none from spawn can.
    fn spawn_minimum(&mut self) -> Result<(), BoardGenError> {
        let spawn = self.spawn;
        for _ in 0..MAX_SWEEPS {
            let spokes = self.tiles[&spawn].spokes;
            if spokes.count() >= self.settings.min_spawn_spokes {
                return Ok(());
            }
            let free: Vec<_> = EdgeDirection::ALL_DIRECTIONS
                .into_iter()
                .filter(|&dir| !spokes.has(dir))
                .collect();
            let &dir = free
                .choose(self.rng)
                .expect("validated min_spawn_spokes <= 6");
            self.branch(spawn, dir, None);
        }
        Err(BoardGenError::Stuck)
    }

    /// Upgrades 2-spoke path tiles to junctions, in random order, each growing
    /// a branch capped to the tiles still owed. Repeats over the grown network
    /// until the path count reaches `target` or no branch fits.
    fn spend(&mut self, target: u32) {
        let s = self.settings;
        for _ in 0..MAX_SWEEPS {
            let mut candidates: Vec<_> = s
                .cells()
                .filter(|hex| {
                    self.tiles
                        .get(hex)
                        .is_some_and(|tile| tile.kind == TileKind::Path && tile.spokes.count() == 2)
                })
                .collect();
            if self.paths() >= target || candidates.is_empty() {
                return;
            }
            candidates.shuffle(self.rng);
            for hex in candidates {
                let owed = target.saturating_sub(self.paths());
                let tile = self.tiles[&hex];
                // An earlier branch may already have joined this candidate.
                if owed == 0
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
                    self.branch(hex, dir, Some(owed));
                }
            }
        }
    }

    /// Grows a random walk from `origin`, leaving it in `first`, at most `cap`
    /// tiles long. The walk ends joined into an adjacent tile or in a dangling
    /// spoke. With `no_dead_ends` it must instead close a loop of at least
    /// `min_loop` tiles, and is undone if it cannot.
    fn branch(&mut self, origin: Hex, first: EdgeDirection, cap: Option<u32>) {
        if !self.settings.no_dead_ends {
            self.walk(origin, first, cap, false);
            return;
        }
        let before = self.tiles.clone();
        if !self.walk(origin, first, cap, true) {
            self.tiles = before;
        }
    }

    /// The walk behind [`Self::branch`]. Returns false if a `closed` walk
    /// could not close its loop, leaving the board for the caller to restore.
    fn walk(&mut self, origin: Hex, first: EdgeDirection, cap: Option<u32>, closed: bool) -> bool {
        let s = self.settings;
        let drawn = self
            .rng
            .random_range(s.min_branch_length..=s.max_branch_length);
        let length = cap.map_or(drawn, |cap| drawn.min(cap));
        // Network distances from `origin` before the walk, to size its loop.
        let around = match closed && s.min_loop > 0 {
            true => self.distances(origin),
            false => HashMap::default(),
        };
        let mut walked = HashMap::from([(origin, 0)]);
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
            walked.insert(at, walked.len());
        }

        let Some(entered) = entered else {
            if !closed {
                self.spoke(origin, first);
            }
            return !closed;
        };
        let added = walked.len() - 1;
        // Tiles around the loop formed by joining `hex` from the walk's head.
        let closes = |generator: &Self, hex: Hex| {
            let loop_size = match walked.get(&hex) {
                Some(&step) => added - step + 1,
                None => around.get(&hex).map_or(usize::MAX, |&d| d + added + 1),
            };
            generator.joinable(hex) && loop_size >= s.min_loop as usize
        };
        let join = match closed || self.rng.random_bool(s.branch_join_chance as f64) {
            true => self.pick_turn(at, entered, closes),
            false => None,
        };
        match join {
            Some(exit) => self.connect(at, exit),
            None if closed => return false,
            None => {
                if let Some(exit) = self.pick_turn(at, entered, |_, _| true) {
                    self.spoke(at, exit);
                }
            }
        }
        true
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

    /// Number of path tiles, excluding spawn and nodes.
    fn paths(&self) -> u32 {
        self.tiles
            .values()
            .filter(|tile| tile.kind == TileKind::Path)
            .count() as u32
    }

    /// Neighbors reachable from `hex` across a spoke on both sides.
    fn crossings(&self, hex: Hex) -> impl Iterator<Item = Hex> + '_ {
        let spokes = self
            .tiles
            .get(&hex)
            .map(|tile| tile.spokes)
            .unwrap_or_default();
        EdgeDirection::ALL_DIRECTIONS
            .into_iter()
            .filter_map(move |dir| {
                let next = self.tiles.get(&hex.neighbor(dir))?;
                spokes
                    .crosses(next.spokes, dir)
                    .then_some(hex.neighbor(dir))
            })
    }

    /// Steps from `from` to every tile it can walk to.
    fn distances(&self, from: Hex) -> HashMap<Hex, usize> {
        let mut distances = HashMap::from([(from, 0)]);
        let mut frontier = std::collections::VecDeque::from([from]);
        while let Some(hex) = frontier.pop_front() {
            let next_distance = distances[&hex] + 1;
            for next in self.crossings(hex) {
                if !distances.contains_key(&next) {
                    distances.insert(next, next_distance);
                    frontier.push_back(next);
                }
            }
        }
        distances
    }

    /// Whether `node` sits on a loop: some crossing out of it can be returned
    /// from by another way, so it is not a bridge.
    fn looped(&self, node: Hex) -> bool {
        self.crossings(node).any(|first| {
            let mut seen = HashSet::from([node, first]);
            let mut frontier = vec![first];
            while let Some(hex) = frontier.pop() {
                for next in self.crossings(hex) {
                    // Skip only the crossing back across `node`–`first` itself.
                    if next == node && hex != first {
                        return true;
                    }
                    if seen.insert(next) {
                        frontier.push(next);
                    }
                }
            }
            false
        })
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

    /// Dead ends allowed, nodes not looped, as before the loop settings.
    fn open() -> HexPathsSettings {
        HexPathsSettings {
            no_dead_ends: false,
            node_loops: false,
            min_branch_length: 1,
            max_branch_length: 4,
            target_paths: 20,
            ..default()
        }
    }

    fn variants() -> [HexPathsSettings; 5] {
        [
            HexPathsSettings::default(),
            HexPathsSettings {
                spawn: SpawnPlacement::Random,
                noise: 0.,
                ..default()
            },
            open(),
            HexPathsSettings {
                junction_chance: 1.,
                branch_join_chance: 1.,
                ..open()
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
                ..open()
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

    /// Tiles walkable from `from` without stepping onto `avoid`.
    fn reachable(layout: &BoardLayout, from: Hex, avoid: Option<Hex>) -> HashSet<Hex> {
        let mut seen = HashSet::from([from]);
        let mut frontier = vec![from];
        while let Some(hex) = frontier.pop() {
            for dir in EdgeDirection::ALL_DIRECTIONS {
                let next = hex.neighbor(dir);
                let Some(to) = layout.tiles.get(&next) else {
                    continue;
                };
                if Some(next) != avoid
                    && layout.tiles[&hex].spokes.crosses(to.spokes, dir)
                    && seen.insert(next)
                {
                    frontier.push(next);
                }
            }
        }
        seen
    }

    #[test]
    fn spawn_reaches_every_tile() {
        for (_, layout) in boards() {
            assert_eq!(
                reachable(&layout, layout.spawn, None).len(),
                layout.tiles.len()
            );
        }
    }

    #[test]
    fn path_count_lands_in_window() {
        for (settings, layout) in boards() {
            let paths = layout
                .tiles
                .values()
                .filter(|tile| tile.kind == TileKind::Path)
                .count() as u32;
            assert!(settings.target_window().contains(&paths), "{paths}");
        }
    }

    #[test]
    fn closed_boards_have_no_dead_ends() {
        for (_, layout) in boards().filter(|(settings, _)| settings.no_dead_ends) {
            for (&hex, tile) in &layout.tiles {
                for dir in EdgeDirection::ALL_DIRECTIONS
                    .into_iter()
                    .filter(|&d| tile.spokes.has(d))
                {
                    let partner = layout.tiles.get(&hex.neighbor(dir));
                    assert!(partner.is_some_and(|to| tile.spokes.crosses(to.spokes, dir)));
                }
            }
        }
    }

    #[test]
    fn every_node_sits_on_a_loop() {
        // A node is on a loop iff two of its neighbors stay connected without it.
        for (_, layout) in boards().filter(|(settings, _)| settings.node_loops) {
            for (node, _) in nodes(&layout) {
                let next: Vec<_> = EdgeDirection::ALL_DIRECTIONS
                    .into_iter()
                    .filter(|&dir| {
                        layout
                            .tiles
                            .get(&node.neighbor(dir))
                            .is_some_and(|to| layout.tiles[&node].spokes.crosses(to.spokes, dir))
                    })
                    .map(|dir| node.neighbor(dir))
                    .collect();
                assert!(next.iter().enumerate().any(|(i, &a)| {
                    let around = reachable(&layout, a, Some(node));
                    next[i + 1..].iter().any(|b| around.contains(b))
                }));
            }
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
        let no_window = HexPathsSettings {
            target_tolerance: 1.,
            ..default()
        };
        assert!(matches!(
            BoardLayout::generate(&no_window, rng),
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
