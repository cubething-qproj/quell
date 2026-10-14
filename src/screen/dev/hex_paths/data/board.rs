//! Generated board ownership, measurements, and surface rendering.
//!
//! [`Board`] pairs immutable [`Terrain`] with replaceable [`Layout`]. Its
//! methods measure the arrangement and build the visible mesh; screen systems
//! attach that mesh and marker entities to the preview hierarchy.

use super::{
    CameraFocus, HexPathsAppearance, HexPathsSettings, HexTerrainTuning, TerrainView, Tint,
};
use crate::prelude::*;
use crate::screen::dev::hex_paths::{
    layout::{Context, Layout, Pace},
    terrain::{Terrain, point_heights, world},
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
};
use std::collections::BTreeSet;

/// The generated state and measurements presented by the hex dev screen.
///
/// Owns one fixed [`Terrain`] selected from a larger [`Region`](super::super::terrain::erosion::Region), and one
/// replaceable [`Layout`] of nodes, pads, and trails over it. [`Self::new`]
/// measures a fresh board; [`Self::replace_layout`] retains the terrain and
/// updates layout statistics during regeneration or a fresh layout request.
///
/// [`systems`](super::super::systems) bridges this resource into surface and marker entities,
/// while [`panel`](super::super::panel) displays its pace, held teases, and crossing checks.
/// Its heights and measurements use true edge-length scale; [`Self::focus`]
/// and [`Self::pad`] apply preview exaggeration only when returning world points.
/// The screen explicitly removes this play-time resource on unload.
#[derive(Resource)]
pub struct Board {
    /// Immutable chosen ground, reused by every layout on this board.
    pub terrain: Terrain,
    /// Current landmarks, levelled ground copy, and trail network.
    pub layout: Layout,
    /// The terrain seed.
    pub seed: u64,
    /// Successful layout replacements since full board generation; initially zero.
    pub count: u32,
    /// Whether this layout regenerated round the player, so spawn is where
    /// the player stood.
    pub regenerated: bool,
    /// Middle of the playable area, and its mean corner height.
    pub centre: (Vec2, f32),
    pub pace: Pace,
    /// Tease pairs with a surviving detour and visible target.
    /// Pairs use [`Layout::points`] indices, not tiles or stable node IDs.
    pub held: BTreeSet<(usize, usize)>,
    /// Crossings with fewer than two trail-bearing banks, from [`Layout::one_sided`].
    pub one_sided: usize,
}

impl Board {
    /// Wraps a terrain and compatible layout, then measures the preview statistics.
    ///
    /// The caller must supply a layout generated over this terrain and matching
    /// settings. The board starts with a spawn marker; regeneration state is set
    /// separately by [`relayout`](super::super::systems::relayout).
    pub fn new(
        terrain: Terrain,
        layout: Layout,
        knobs: (&HexPathsSettings, &HexTerrainTuning),
        seed: u64,
        count: u32,
    ) -> Self {
        let mut board = Self {
            terrain,
            layout,
            seed,
            count,
            regenerated: false,
            centre: (Vec2::ZERO, 0.),
            pace: Pace::default(),
            held: BTreeSet::new(),
            one_sided: 0,
        };
        board.measure(knobs);
        board
    }

    /// Installs a layout generated over this board's existing terrain and remeasures it.
    ///
    /// Updates the successful layout count, centre, pace, held pairs, and bank
    /// statistics. It does not choose the operation's marker semantics or repair
    /// camera focus; [`relayout`](super::super::systems::relayout) handles those after success.
    pub fn replace_layout(
        &mut self,
        layout: Layout,
        count: u32,
        knobs: (&HexPathsSettings, &HexTerrainTuning),
    ) {
        self.layout = layout;
        self.count = count;
        self.measure(knobs);
    }

    /// Measures the layout: the board's middle, its pace, teases that hold
    /// and one-sided crossings.
    fn measure(&mut self, (settings, tuning): (&HexPathsSettings, &HexTerrainTuning)) {
        let (terrain, layout) = (&self.terrain, &self.layout);
        let context = Context {
            terrain,
            settings,
            tuning,
        };
        let inside = terrain.mesh.playable_corners();
        let (min, max, sum, corners) = terrain
            .mesh
            .positions
            .iter()
            .zip(&layout.heights)
            .zip(&inside)
            .filter(|(_, inside)| **inside)
            .fold(
                (Vec2::MAX, Vec2::MIN, 0., 0.),
                |(min, max, sum, count), ((&p, &h), _)| {
                    (min.min(p), max.max(p), sum + h, count + 1.)
                },
            );
        let broken = layout.broken_teases(context);
        let held = layout
            .tease_pairs
            .iter()
            .copied()
            .filter(|&(v, t)| !broken.contains(&(v, t)) && layout.visible(context, v, t))
            .collect();
        let (pace, one_sided) = (layout.pace(context), layout.one_sided(context));
        self.centre = ((min + max) / 2., sum / corners);
        self.pace = pace;
        self.held = held;
        self.one_sided = one_sided;
    }

    /// Keeps a focus if its node tile is occupied; otherwise falls back to Player.
    ///
    /// Includes [`Layout::player_node`] in the occupancy check. A different node
    /// at the same tile still counts as valid: this does not track a moved node's
    /// identity. Call after replacement, or before using node focus as a player tile.
    pub fn keep_focus(&self, focus: CameraFocus) -> CameraFocus {
        match focus {
            CameraFocus::Node(tile) if !self.layout.all_nodes().any(|n| n.tile == tile) => {
                CameraFocus::Player
            }
            focus => focus,
        }
    }

    /// Resolves a camera focus to a world point with preview height exaggeration.
    ///
    /// Terrain focus uses the board centre and mean playable height; Player uses
    /// spawn; Node uses its tile's pad. Node tile indices must be valid on this
    /// board; [`Self::keep_focus`] validates a retained focus before use.
    pub fn focus(&self, focus: CameraFocus, vertical_scale: f32) -> Vec3 {
        match focus {
            CameraFocus::Terrain => world(self.centre.0, self.centre.1 * vertical_scale),
            CameraFocus::Player => self.pad(self.layout.spawn, vertical_scale),
            CameraFocus::Node(tile) => self.pad(tile, vertical_scale),
        }
    }

    /// Returns a tile centre at the median of its levelled corner heights.
    ///
    /// Applies `vertical_scale` to height before converting to world coordinates.
    /// `tile` must be a valid index on this board, but need not contain a node.
    /// Used to position markers and node/player camera targets.
    pub fn pad(&self, tile: usize, vertical_scale: f32) -> Vec3 {
        let mut corners = self.terrain.mesh.tile_corners[tile].map(|c| self.layout.heights[c]);
        corners.sort_by(f32::total_cmp);
        world(
            self.terrain.mesh.centers[tile],
            (corners[2] + corners[3]) / 2. * vertical_scale,
        )
    }

    /// Builds the visible mesh from levelled corner heights and mean-height tile centres.
    ///
    /// Uses [`point_heights`] and the terrain mesh's triangles,
    /// corrects winding to face up, and computes area-weighted vertex normals.
    /// Normals drive hillshade; [`Self::view_tint`] blends the chosen measure over an
    /// absolute metre-height palette. Scenery fades toward paper. Vertical
    /// exaggeration changes positions and shading, but not the palette's heights.
    pub fn surface_mesh(
        &self,
        settings: &HexPathsSettings,
        tuning: &HexTerrainTuning,
        view: TerrainView,
        appearance: &HexPathsAppearance,
    ) -> Mesh {
        let terrain = &self.terrain;
        let mesh = &terrain.mesh;
        let heights = point_heights(mesh, &self.layout.heights);
        let positions: Vec<Vec3> = mesh
            .positions
            .iter()
            .chain(&mesh.centers)
            .zip(&heights)
            .map(|(&p, &h)| world(p, h * appearance.vertical_scale))
            .collect();
        // Wind every triangle to face up.
        let triangles: Vec<[usize; 3]> = mesh
            .triangles()
            .map(|[a, b, c]| {
                let up = (positions[b] - positions[a])
                    .cross(positions[c] - positions[a])
                    .y;
                if up >= 0. { [a, b, c] } else { [a, c, b] }
            })
            .collect();
        let mut normals = vec![Vec3::ZERO; positions.len()];
        for &[a, b, c] in &triangles {
            // Unnormalised, so larger triangles weigh more.
            let normal = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
            [a, b, c].iter().for_each(|&i| normals[i] += normal);
        }
        let normals: Vec<Vec3> = normals.into_iter().map(Vec3::normalize_or_zero).collect();
        let inside: Vec<bool> = mesh
            .playable_corners()
            .into_iter()
            .chain(mesh.playable.iter().copied())
            .collect();
        let light = appearance.light.normalize();
        let paper = appearance.paper.to_linear();
        let tint = self.view_tint(settings, tuning, view, appearance);
        let colours: Vec<[f32; 4]> = heights
            .iter()
            .enumerate()
            .zip(&normals)
            .zip(&inside)
            .map(|(((i, &h), &n), &inside)| {
                let shade =
                    appearance.shade_floor + appearance.shade_gain * n.dot(light).max(0.) / light.y;
                let base = tint
                    .base
                    .unwrap_or_else(|| appearance.ramp(h * tuning.edge_m));
                let lit = (base * shade).mix(&tint.colours[i], tint.amounts[i]);
                let veil = if inside { 0. } else { appearance.margin_veil };
                lit.mix(&paper, veil).with_alpha(1.).to_f32_array()
            })
            .collect();
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32(
            triangles.into_iter().flatten().map(|i| i as u32).collect(),
        ))
    }

    /// Creates per-point colour targets and weights for the selected [`TerrainView`].
    ///
    /// Heightmap contributes no tint. Cost uses levelled-ground steepness;
    /// suitability uses original terrain scores sharpened by spread; BenchScore
    /// blends quiet and overlook shares over paper. Arrays follow corners, then
    /// tile centres, matching [`Tint`] and the surface point order.
    fn view_tint(
        &self,
        settings: &HexPathsSettings,
        tuning: &HexTerrainTuning,
        view: TerrainView,
        appearance: &HexPathsAppearance,
    ) -> Tint {
        let terrain = &self.terrain;
        let mesh = &terrain.mesh;
        let points = mesh.positions.len() + mesh.tiles.len();
        let single = |colour: Color, amounts: Vec<f32>| Tint {
            base: None,
            colours: vec![colour.to_linear(); points],
            amounts,
        };
        match view {
            TerrainView::Heightmap => single(Color::NONE, vec![0.; points]),
            TerrainView::Cost => single(
                appearance.cost,
                terrain.corner_amounts(&self.steepness(tuning, appearance)),
            ),
            TerrainView::Suitability => single(
                appearance.score,
                terrain.tile_amounts(
                    &terrain
                        .suitability
                        .iter()
                        .map(|s| s.powf(settings.spread))
                        .collect::<Vec<_>>(),
                ),
            ),
            TerrainView::BenchScore => {
                let quiet = terrain.tile_amounts(&terrain.bench_quiet);
                let overlook = terrain.tile_amounts(&terrain.bench_overlook);
                let (quiet_colour, overlook_colour) = (
                    appearance.quiet.to_linear(),
                    appearance.overlook.to_linear(),
                );
                Tint {
                    base: Some(appearance.paper.to_linear()),
                    colours: quiet
                        .iter()
                        .zip(&overlook)
                        .map(|(q, o)| {
                            let towards_overlook = o / (q + o).max(f32::EPSILON);
                            quiet_colour.mix(&overlook_colour, towards_overlook)
                        })
                        .collect(),
                    amounts: quiet
                        .iter()
                        .zip(&overlook)
                        .map(|(q, o)| (q + o).min(1.))
                        .collect(),
                }
            }
        }
    }

    /// Each corner's steepest edge over the levelled ground as
    /// `aversion × |rise|`, the log of the step cost, scaled so the top few
    /// percent of playable edges reach 1.
    fn steepness(&self, tuning: &HexTerrainTuning, appearance: &HexPathsAppearance) -> Vec<f32> {
        let mesh = &self.terrain.mesh;
        let steepness: Vec<f32> = self
            .terrain
            .step_costs(&self.layout.heights, tuning.aversion)
            .into_iter()
            .map(f32::ln)
            .collect();
        let mut playable: Vec<f32> = steepness
            .iter()
            .copied()
            .filter(|s| s.is_finite())
            .collect();
        playable.sort_by(f32::total_cmp);
        let top = playable
            .get(((playable.len() as f32 - 1.) * appearance.cost_percentile) as usize)
            .copied()
            .unwrap_or(1.)
            .max(f32::EPSILON);
        let mut corners = vec![0f32; mesh.positions.len()];
        for (&[a, b], &s) in mesh.edges.iter().zip(&steepness) {
            if s.is_finite() {
                let amount = (s / top).min(1.);
                corners[a] = corners[a].max(amount);
                corners[b] = corners[b].max(amount);
            }
        }
        corners
    }
}
