//! Growing tree, network first: grows a tree tile by tile from spawn, then
//! places the nodes on it, preferring its dead ends.
//!
//! Each step either extends a dead end ("head"), choosing the turn by shape
//! weight, or with `junction_chance` sprouts from a 2-spoke tile.
//! `newest_bias` picks between the newest head and a random one. The tree is
//! connected by construction; dead ends left after placing nodes are then
//! joined, capped, or pruned, depending on `no_dead_ends`.

use super::*;

impl<R: Rng> Generator<'_, R> {
    /// One growing-tree attempt. Grows `target + nodes` tiles, or only
    /// `loop_growth` of the target when node loops will add tiles afterwards
    /// and the budget tops up the rest.
    pub(super) fn run_growing_tree(mut self) -> Result<BoardLayout, BoardGenError> {
        let s = self.settings;
        let window = s.target_window();
        let target = self.rng.random_range(window.clone());
        let cards = s.deck.cards().len() as u32;
        let size = match s.node_loops {
            true => (target as f32 * s.loop_growth) as u32,
            false => target,
        } + cards;
        let grown = self.grow(size as usize)?;
        let nodes = self.claim_nodes(&grown)?;
        match s.no_dead_ends {
            true => self.close_dead_ends(),
            false => self.cap_dead_ends(),
        }
        if s.node_loops {
            let costs = self.cost_field();
            self.loop_nodes(&nodes, &costs)?;
            self.spend(target);
        }
        let spawn_spokes = self.tiles[&self.spawn].spokes.count();
        // Pruning can take the count out of the window, spawn below its
        // minimum, or a junction back to a forbidden 2-spoke bend.
        if !window.contains(&self.paths())
            || spawn_spokes < s.min_spawn_spokes
            || !self.bends_allowed()
        {
            return Err(BoardGenError::Stuck);
        }
        Ok(BoardLayout {
            tiles: self.tiles,
            spawn: self.spawn,
            attempts: 0,
        })
    }

    /// Grows `size` path tiles from spawn. Returns them in creation order.
    fn grow(&mut self, size: usize) -> Result<Vec<Hex>, BoardGenError> {
        let s = self.settings;
        let spawn = self.spawn;
        let mut free: Vec<_> = EdgeDirection::ALL_DIRECTIONS
            .into_iter()
            .filter(|&dir| self.is_empty(spawn.neighbor(dir)))
            .collect();
        if free.len() < s.min_spawn_spokes as usize {
            return Err(BoardGenError::Stuck);
        }
        free.shuffle(self.rng);
        let first = self
            .rng
            .random_range(s.min_spawn_spokes..=s.max_spawn_spokes);
        let mut order = Vec::new();
        for dir in free.into_iter().take(first as usize) {
            self.connect(spawn, dir);
            order.push(spawn.neighbor(dir));
        }
        // Heads that cannot turn anywhere; they stay dead ends.
        let mut dead = HashSet::new();
        while order.len() < size {
            let heads: Vec<_> = order
                .iter()
                .copied()
                .filter(|hex| self.tiles[hex].spokes.count() == 1 && !dead.contains(hex))
                .collect();
            let sprouts: Vec<_> = order
                .iter()
                .filter(|&&hex| self.tiles[&hex].spokes.count() == 2)
                .flat_map(|&hex| {
                    let spokes = self.tiles[&hex].spokes;
                    EdgeDirection::ALL_DIRECTIONS
                        .into_iter()
                        .filter(move |&dir| !spokes.has(dir))
                        .map(move |dir| (hex, dir))
                })
                .filter(|&(hex, dir)| self.is_empty(hex.neighbor(dir)))
                .collect();
            let sprout = !sprouts.is_empty()
                && (heads.is_empty() || self.rng.random_bool(s.junction_chance as f64));
            let (source, exit) = if sprout {
                *sprouts.choose(self.rng).expect("checked non-empty")
            } else if let Some(&newest) = heads.last() {
                let head = match self.rng.random_bool(s.newest_bias as f64) {
                    true => newest,
                    false => *heads.choose(self.rng).expect("checked non-empty"),
                };
                let entered = -self.tiles[&head]
                    .spokes
                    .directions()
                    .next()
                    .expect("a head");
                match self.pick_turn(head, entered, Self::is_empty) {
                    Some(exit) => (head, exit),
                    None => {
                        dead.insert(head);
                        continue;
                    }
                }
            } else {
                return Err(BoardGenError::Stuck);
            };
            self.connect(source, exit);
            order.push(source.neighbor(exit));
        }
        Ok(order)
    }

    /// Turns grown tiles into nodes, dead ends first, with the usual spacing,
    /// and deals the deck onto them. Returns them in random order.
    fn claim_nodes(&mut self, grown: &[Hex]) -> Result<Vec<Hex>, BoardGenError> {
        let s = self.settings;
        let mut cards = s.deck.cards();
        cards.shuffle(self.rng);
        let (mut ends, mut rest): (Vec<Hex>, Vec<Hex>) = grown
            .iter()
            .partition(|&&hex| self.tiles[&hex].spokes.count() == 1);
        ends.shuffle(self.rng);
        rest.shuffle(self.rng);
        let nodes = self.spaced(ends.into_iter().chain(rest), cards.len());
        if nodes.len() < cards.len() {
            return Err(BoardGenError::Placement);
        }
        for (node, card) in nodes.iter().zip(cards) {
            self.tiles.get_mut(node).expect("grown tile").kind = TileKind::Node(card);
        }
        Ok(nodes)
    }

    /// 1-spoke path tiles, in grid order.
    fn dead_ends(&self) -> Vec<Hex> {
        self.settings
            .cells()
            .filter(|hex| {
                self.tiles
                    .get(hex)
                    .is_some_and(|tile| tile.kind == TileKind::Path && tile.spokes.count() == 1)
            })
            .collect()
    }

    /// Ends every dead end as a branch would: joined into a neighbor with
    /// `branch_join_chance`, otherwise with a dangling spoke.
    fn cap_dead_ends(&mut self) {
        let s = self.settings;
        for hex in self.dead_ends() {
            let entered = -self.tiles[&hex]
                .spokes
                .directions()
                .next()
                .expect("a dead end");
            // Joining another uncapped dead end would bend it at an unweighted angle.
            let joinable = |generator: &Self, other: Hex| {
                generator.joinable(other)
                    && !(generator.tiles[&other].kind == TileKind::Path
                        && generator.tiles[&other].spokes.count() == 1)
            };
            let join = match self.rng.random_bool(s.branch_join_chance as f64) {
                true => self.pick_turn(hex, entered, joinable),
                false => None,
            };
            match join {
                Some(exit) => self.connect(hex, exit),
                None => {
                    if let Some(exit) = self.pick_turn(hex, entered, |_, _| true) {
                        self.spoke(hex, exit);
                    }
                }
            }
        }
    }

    /// Joins every dead end into a neighbor, closing a loop of at least
    /// `min_loop` tiles. Dead ends that cannot are pruned back until they meet
    /// a junction, node, or spawn.
    fn close_dead_ends(&mut self) {
        let s = self.settings;
        for hex in self.dead_ends() {
            let Some(tile) = self.tiles.get(&hex).filter(|tile| tile.spokes.count() == 1) else {
                continue; // An earlier dead end joined into this one.
            };
            let entered = -tile.spokes.directions().next().expect("a dead end");
            let around = self.distances(hex);
            let closes = |generator: &Self, other: Hex| {
                let far = around
                    .get(&other)
                    .is_none_or(|&d| d + 1 >= s.min_loop as usize);
                if !generator.joinable(other) || !far {
                    return false;
                }
                // Joining another dead end bends it; that bend must be allowed too.
                let target = generator.tiles[&other];
                if target.kind != TileKind::Path || target.spokes.count() != 1 {
                    return true;
                }
                let back = target.spokes.directions().next().expect("a dead end");
                let toward = other.neighbor_direction(hex).expect("adjacent");
                PathShape::of_turn(-back, toward)
                    .is_some_and(|shape| s.shape_weights.get(shape) > 0.)
            };
            if let Some(exit) = self.pick_turn(hex, entered, closes) {
                self.connect(hex, exit);
            }
        }
        loop {
            let dead = self.dead_ends();
            if dead.is_empty() {
                return;
            }
            for hex in dead {
                let tile = self.tiles.remove(&hex).expect("a dead end");
                let back = tile.spokes.directions().next().expect("a dead end");
                if let Some(parent) = self.tiles.get_mut(&hex.neighbor(back)) {
                    parent.spokes.remove(-back);
                }
            }
        }
    }

    /// Whether every 2-spoke path tile bends at an allowed (positive-weight)
    /// shape.
    fn bends_allowed(&self) -> bool {
        self.tiles
            .values()
            .filter(|tile| tile.kind == TileKind::Path && tile.spokes.count() == 2)
            .all(|tile| {
                let mut dirs = tile.spokes.directions();
                let (a, b) = (dirs.next().expect("two"), dirs.next().expect("two"));
                PathShape::of_turn(-a, b)
                    .is_some_and(|shape| self.settings.shape_weights.get(shape) > 0.)
            })
    }
}
