//! Walking controls and the player's route through one layout.
//!
//! [`WalkingSettings`] enables a session; [`Walk`] lives on its billboard entity.
//! [`Exploration`] accompanies the walker and remembers trails and visited
//! landmarks. Routing borrows [`Layout`], and movement borrows its levelled heights. Neither
//! operation changes the generated board. Corner indices are valid only for
//! that session; [`sync_player`](super::systems::sync_player) ends it when the
//! board changes.

use super::super::{
    layout::{Context, Layout},
    terrain::world,
};
use bevy::prelude::*;
use std::collections::{BTreeSet, VecDeque};

pub use super::exploration::Exploration;

/// Inspector controls for walking and discovering the generated network.
///
/// The panel toggles `enabled`. Entering starts at spawn; leaving discards
/// the route. A new board or layout also disables walking. `speed` multiplies
/// elapsed walking time without changing route selection or preview geometry.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct WalkingSettings {
    pub enabled: bool,
    /// Simulation multiplier; zero pauses. Negative or non-finite values pause too.
    pub speed: f32,
    /// Horizontal sight radius in edge lengths; only visible edges can be routed over.
    pub sight_radius: f32,
    /// Fixed camera distance while Player focus follows the walker; zoom is disabled.
    pub follow_distance: f32,
    /// Steepest downward camera pitch while following, in degrees below the horizon.
    pub follow_max_pitch: f32,
    /// Outline and destination colour for a reachable tile under the pointer.
    pub hover_colour: Color,
    /// Radius of the destination marker, in edge lengths.
    pub target_radius: f32,
}

impl Default for WalkingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            speed: 100.,
            sight_radius: 3.,
            follow_distance: 6.,
            follow_max_pitch: 60.,
            hover_colour: Color::srgb(1., 0.85, 0.15),
            target_radius: 0.15,
        }
    }
}

/// A player walking corner-to-corner across one [`Layout`].
///
/// `corner` is the last corner reached. The front of `route` is the next
/// corner, and `progress` is the fraction of that edge already walked.
/// Heights and horizontal coordinates interpolate along the same edge;
/// exaggeration affects only [`Self::position`], never elapsed travel time.
#[derive(Component, Debug, Clone)]
pub struct Walk {
    pub corner: usize,
    pub route: VecDeque<usize>,
    pub progress: f64,
}

impl Walk {
    /// Starts on a spawn-pad corner with a laid trail, or any spawn corner if isolated.
    pub fn at_spawn(context: Context, layout: &Layout) -> Self {
        let mesh = &context.terrain.mesh;
        let corners = mesh.tile_corners[layout.spawn];
        let corner = corners
            .into_iter()
            .find(|&c| {
                mesh.corner_edges[c]
                    .iter()
                    .any(|(_, e)| layout.trail.contains_key(e))
            })
            .unwrap_or(corners[0]);
        Self {
            corner,
            route: VecDeque::new(),
            progress: 0.,
        }
    }

    /// Resolves a world-space surface hit to a reachable corner on that tile.
    ///
    /// Inverts [`world`]'s negated Z axis. Only playable tiles with a trail or
    /// pad perimeter accept clicks; empty ground never snaps to a distant path.
    /// Both hover preview and pressing use this rule. A request during an edge
    /// must be reachable from its far corner, where redirection takes effect.
    /// `known` limits eligible edges and the connecting route; `None` exposes
    /// the whole network for inspection.
    pub fn destination(
        &self,
        context: Context,
        layout: &Layout,
        hit: Vec3,
        known: Option<&BTreeSet<usize>>,
    ) -> Option<Destination> {
        let mesh = &context.terrain.mesh;
        let point = Vec2::new(hit.x, -hit.z);
        let tile = mesh.tile_containing(point)?;
        if !mesh.playable[tile] {
            return None;
        }
        let costs = layout.walking_costs(context);
        let mut eligible = mesh
            .perimeter(tile)
            .into_iter()
            .filter(|&e| costs[e].is_finite() && known.is_none_or(|known| known.contains(&e)))
            .flat_map(|e| mesh.edges[e])
            .collect::<Vec<_>>();
        eligible.sort_unstable();
        eligible.dedup();
        let corner = eligible.into_iter().min_by(|&a, &b| {
            mesh.positions[a]
                .distance_squared(point)
                .total_cmp(&mesh.positions[b].distance_squared(point))
                .then(a.cmp(&b))
        })?;
        let start = if self.progress > 0. {
            self.route[0]
        } else {
            self.corner
        };
        layout.walking_route(context, start, corner, known)?;
        Some(Destination { tile, corner })
    }

    /// Accepts a destination only if it is reachable across trails and pads.
    ///
    /// During an edge, replans from its far corner and keeps that edge's
    /// progress. An unreachable request leaves the existing walk intact.
    /// `known` restricts the route to discovered edges; `None` exposes the
    /// whole network for inspection.
    pub fn plan(
        &mut self,
        context: Context,
        layout: &Layout,
        target: usize,
        known: Option<&BTreeSet<usize>>,
    ) -> bool {
        let underway = self.progress > 0.;
        let start = if underway { self.route[0] } else { self.corner };
        let Some(route) = layout.walking_route(context, start, target, known) else {
            return false;
        };
        self.route = underway.then_some(start).into_iter().chain(route).collect();
        true
    }

    /// Consumes simulated seconds, carrying unused time across successive edges.
    ///
    /// Uses directional Tobler speed for each edge's rise/run and its actual
    /// unexaggerated 3D length. There is no smoothing-length correction. Zero,
    /// negative, or non-finite elapsed time leaves the walk unchanged. Returns
    /// every corner reached, so discovery cannot skip arrivals on long frames.
    pub fn advance(&mut self, context: Context, layout: &Layout, mut seconds: f64) -> Vec<usize> {
        let mut arrived = Vec::new();
        if !seconds.is_finite() || seconds <= 0. {
            return arrived;
        }
        while let Some(&next) = self.route.front() {
            let duration = Self::edge_seconds(context, layout, self.corner, next);
            let remaining = (1. - self.progress) * duration;
            if seconds < remaining {
                self.progress += seconds / duration;
                break;
            }
            seconds -= remaining;
            self.corner = next;
            arrived.push(next);
            self.route.pop_front();
            self.progress = 0.;
        }
        arrived
    }

    /// Returns the billboard's world position on the displayed, levelled ground.
    pub fn position(&self, context: Context, layout: &Layout, vertical_scale: f32) -> Vec3 {
        let mesh = &context.terrain.mesh;
        let next = self.route.front().copied().unwrap_or(self.corner);
        let fraction = self.progress as f32;
        let point = mesh.positions[self.corner].lerp(mesh.positions[next], fraction);
        let height = layout.heights[self.corner]
            + (layout.heights[next] - layout.heights[self.corner]) * fraction;
        world(point, height * vertical_scale)
    }

    /// Time for one directed edge, using metre scale and Tobler's km/h curve.
    fn edge_seconds(context: Context, layout: &Layout, from: usize, to: usize) -> f64 {
        let mesh = &context.terrain.mesh;
        let run = mesh.positions[from].distance(mesh.positions[to]) as f64;
        let rise = (layout.heights[to] - layout.heights[from]) as f64;
        let speed_kmh = 6. * (-3.5 * (rise / run + 0.05).abs()).exp();
        run.hypot(rise) * context.tuning.edge_m as f64 * 3.6 / speed_kmh
    }
}

/// A hovered path tile and the exact corner a click would request.
///
/// [`Walk::destination`] validates local path membership and reachability.
/// The preview outlines `tile` and marks `corner`; picking uses the same
/// calculation instead of independently snapping to another point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destination {
    pub tile: usize,
    pub corner: usize,
}
