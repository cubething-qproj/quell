//! Knowledge accumulated while walking one layout.
//!
//! [`Exploration`] accompanies [`Walk`] on the player entity. It separates
//! current sight and visited node IDs from the generated
//! [`Layout`]. Rendering and destination picking read this state; inspection
//! overrides never add knowledge. Replacing the board or leaving walking mode
//! removes both components together.

use super::super::layout::{Context, Layout};
use super::data::Walk;
use bevy::prelude::*;
use std::collections::BTreeSet;

/// Local trail knowledge and visited landmarks for one walking session.
///
/// An edge is visible if any part lies within horizontal sight radius. Only
/// visible edges are drawn or routed over, including travel around pads; trails
/// are forgotten once they leave sight. `revealed` uses
/// stable node IDs; `nearby` controls billboard presence independently of cards.
#[derive(Component, Default)]
pub struct Exploration {
    pub visible: BTreeSet<usize>,
    pub revealed: BTreeSet<u32>,
    pub nearby: BTreeSet<u32>,
}

impl Exploration {
    /// Refreshes local sight and reveals nodes at every supplied arrival corner.
    ///
    /// Always includes the last reached corner, so starting on a landmark
    /// reveals it immediately. Visibility uses horizontal distance, without
    /// terrain occlusion or preview exaggeration. Returns whether billboard
    /// presence or revealed cards changed; callers then request a marker update.
    pub fn observe(
        &mut self,
        context: Context,
        layout: &Layout,
        walk: &Walk,
        sight: f32,
        arrived: &[usize],
    ) -> bool {
        let mesh = &context.terrain.mesh;
        let position = walk.position(context, layout, 1.);
        let point = Vec2::new(position.x, -position.z);
        let radius_squared = sight.max(0.).powi(2);
        self.visible = layout
            .trail
            .keys()
            .copied()
            .chain(layout.points().into_iter().flat_map(|t| mesh.perimeter(t)))
            .filter(|&e| {
                let [a, b] = mesh.edges[e].map(|c| mesh.positions[c]);
                let along = ((point - a).dot(b - a) / a.distance_squared(b)).clamp(0., 1.);
                point.distance_squared(a.lerp(b, along)) <= radius_squared
            })
            .collect();
        let nearby = layout
            .all_nodes()
            .filter(|n| mesh.centers[n.tile].distance_squared(point) <= radius_squared)
            .map(|n| n.id)
            .collect();
        let mut changed = self.nearby != nearby;
        self.nearby = nearby;
        for node in layout.all_nodes() {
            let corners = mesh.tile_corners[node.tile];
            if corners.contains(&walk.corner) || arrived.iter().any(|c| corners.contains(c)) {
                changed |= self.revealed.insert(node.id);
            }
        }
        changed
    }

    /// Advances movement and observes every arrival.
    ///
    /// Keeping the arrival list ensures a fast walk that crosses several pads
    /// in one frame reveals each of them. Returns whether marker presentation changed.
    pub fn advance(
        &mut self,
        context: Context,
        layout: &Layout,
        walk: &mut Walk,
        seconds: f64,
        sight: f32,
    ) -> bool {
        let arrived = walk.advance(context, layout, seconds);
        self.observe(context, layout, walk, sight, &arrived)
    }
}
