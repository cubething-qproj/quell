//! ECS-facing settings, generated board state, and preview markers.
//!
//! [`HexPathsSettings`] and [`HexTerrainTuning`] feed region simulation, window
//! selection, and layout generation. [`RegionCache`] retains the last matching
//! [`Region`]; [`Board`] owns the chosen [`Terrain`](super::terrain::Terrain), current [`Layout`](super::layout::Layout), and
//! derived measurements. [`BoardStats`] and [`PanelState`] report failures.
//!
//! [`HexPathsView`] and [`HexPathsAppearance`] control presentation without
//! regenerating ground or trails. [`systems`](super::systems) renders the board with
//! [`TerrainSurface`], [`Marker`], and [`Billboard`] entities beneath [`BoardRoot`].
//! [`screen`](super::screen) creates shared assets on entry and removes play-time
//! resources on unload, while persistent settings remain available.

use super::{
    layout::{Card, TrailKind},
    terrain::erosion::{Region, RegionKey},
};
use crate::prelude::*;

mod board;
mod plugin;
mod settings;

pub use board::Board;
pub use plugin::plugin;
pub use settings::*;

/// Saved UI-camera clear colour, restored when the hex screen unloads.
///
/// [`screen`](super::screen) disables the default UI camera's background clear while
/// compositing it over the board camera, then uses this resource to restore
/// the original value on exit.
#[derive(Resource)]
pub struct UiCameraState {
    pub entity: Entity,
    pub clear_color: ClearColorConfig,
}

/// Shared marker quad and materials created by [`screen`](super::screen) on loading.
///
/// [`spawn_markers`](super::systems::spawn_markers) reuses the quad, and
/// [`paint_markers`](super::systems::paint_markers) selects a material for each [`Marker`].
/// Unload removes this resource; board replacement only replaces marker entities.
#[derive(Resource)]
pub struct Billboards {
    /// Upright quad standing on its origin.
    pub quad: Handle<Mesh>,
    pub spawn: Handle<StandardMaterial>,
    pub player: Handle<StandardMaterial>,
    pub bench: Handle<StandardMaterial>,
    /// A point of interest with no card.
    pub empty: Handle<StandardMaterial>,
    pub key: Handle<StandardMaterial>,
    pub npc: Handle<StandardMaterial>,
    pub fear: Handle<StandardMaterial>,
    /// A node whose type is not yet known.
    pub unknown: Handle<StandardMaterial>,
}

/// User-facing seed-edit and layout-regeneration errors for the panel.
///
/// [`panel`](super::panel) parses seed input; [`relayout`](super::systems::relayout) records
/// failed layout attempts without replacing the board. Full-generation errors
/// live separately in [`BoardStats`]. Screen unload resets this resource.
#[derive(Resource, Default)]
pub struct PanelState {
    pub seed_error: Option<&'static str>,
    pub regeneration_error: Option<&'static str>,
}

/// Result of the latest full board-generation attempt, even if no [`Board`] exists.
///
/// [`spawn_board`](super::systems::spawn_board) stores the attempted seed and any generation
/// error; [`update_stats`](super::panel::update_stats) displays them. Layout-only failures
/// instead use [`PanelState`] and retain the previous Board.
#[derive(Resource, Default)]
pub struct BoardStats {
    pub seed: u64,
    pub error: Option<&'static str>,
}

/// The last simulated [`Region`] and the [`RegionKey`] that produced it.
///
/// [`spawn_board`](super::systems::spawn_board) reuses it for a matching seed and erosion
/// settings. Board size, creek threshold, and layout changes can reuse the
/// simulation but may still rerank windows. A random new seed misses the
/// cache. Screen unload clears it; this holds one result, not a region library.
#[derive(Resource, Default)]
pub struct RegionCache(pub Option<(RegionKey, Region)>);

/// Visual settings for rendering a [`Board`] and its debug panel.
///
/// Read by [`systems`](super::systems) for colours, height exaggeration, overlays, and
/// billboards, and by [`panel`](super::panel) for width. Changing appearance rebuilds
/// presentation but does not alter terrain, routing, visibility, or pace.
/// Physical scale belongs to [`HexTerrainTuning::edge_m`].
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct HexPathsAppearance {
    pub panel_width: f32,
    /// Zoom per mouse wheel line, in the same units as pixel scrolling.
    pub zoom_per_line: f32,
    pub grid: Color,
    /// Grid lines float this far above the terrain.
    pub grid_lift: f32,
    /// Height colour stops, by height as a fraction of `colour_top_m`.
    pub palette: Vec<(f32, Color)>,
    /// Height drawn in the palette's last colour, in metres above the
    /// region's outlets: absolute, so a colour is the same height on every
    /// board.
    pub colour_top_m: f32,
    /// Heights are drawn this many times taller than true scale, which
    /// reads as flat (about 25 m of relief across 1 km).
    pub vertical_scale: f32,
    /// Trail, creek and tease lines float this far above the terrain.
    pub line_lift: f32,
    /// Trail colours by the pass that laid them.
    pub span: Color,
    pub loop_trail: Color,
    pub tree_trail: Color,
    pub forced_trail: Color,
    pub bank_trail: Color,
    pub port_trail: Color,
    pub pinned_trail: Color,
    /// Trails that had to break a tease.
    pub breaking_trail: Color,
    pub creek: Color,
    /// Outline of a crossing's pad.
    pub crossing: Color,
    /// Tease lines, viewer to target, shown with nodes revealed.
    pub tease_held: Color,
    pub tease_broken: Color,
    /// Background the margin fades towards.
    pub paper: Color,
    /// How far the margin fades towards `paper`, 0–1.
    pub margin_veil: f32,
    /// Direction the hillshade light comes from.
    pub light: Vec3,
    /// Colour of the most buildable tiles in the suitability view.
    pub score: Color,
    /// Colour of the quietest bench sites in the bench score view.
    pub quiet: Color,
    /// Colour of the best overlooks in the bench score view.
    pub overlook: Color,
    /// Height of node and player billboards, in edge lengths.
    pub billboard_height: f32,
    /// Colour of the steepest edges in the cost view.
    pub cost: Color,
    /// Share of playable edges below the cost view's full colour, 0–1.
    pub cost_percentile: f32,
    /// Brightness of a slope facing away from the light.
    pub shade_floor: f32,
    /// Extra brightness of flat ground in full light.
    pub shade_gain: f32,
}

impl Default for HexPathsAppearance {
    fn default() -> Self {
        let rgb = |r: u8, g: u8, b: u8| Color::srgb_u8(r, g, b);
        Self {
            panel_width: 340.,
            zoom_per_line: 150.,
            grid: Color::srgb(0.25, 0.28, 0.32),
            grid_lift: 0.03,
            palette: vec![
                (0., rgb(110, 142, 96)),
                (0.3, rgb(156, 170, 112)),
                (0.55, rgb(190, 172, 124)),
                (0.8, rgb(160, 134, 112)),
                (1., rgb(236, 230, 220)),
            ],
            colour_top_m: 120.,
            vertical_scale: 4.,
            line_lift: 0.05,
            span: rgb(43, 33, 28),
            loop_trail: rgb(47, 111, 143),
            tree_trail: rgb(123, 79, 160),
            forced_trail: rgb(176, 58, 46),
            bank_trail: rgb(31, 155, 176),
            port_trail: rgb(217, 130, 43),
            pinned_trail: rgb(138, 129, 120),
            breaking_trail: rgb(224, 33, 138),
            creek: rgb(47, 111, 176),
            crossing: rgb(47, 111, 176),
            tease_held: rgb(192, 57, 43),
            tease_broken: rgb(127, 127, 127),
            paper: rgb(255, 250, 240),
            margin_veil: 0.45,
            light: Vec3::new(-1., 1.4, 1.),
            score: rgb(40, 90, 170),
            quiet: rgb(40, 90, 170),
            overlook: rgb(60, 150, 70),
            billboard_height: 1.5,
            cost: rgb(122, 28, 28),
            cost_percentile: 0.98,
            shade_floor: 0.62,
            shade_gain: 0.3,
        }
    }
}

impl HexPathsAppearance {
    /// Returns the configured overlay colour for the pass recorded by [`TrailKind`].
    pub fn trail(&self, kind: TrailKind) -> Color {
        match kind {
            TrailKind::Span => self.span,
            TrailKind::Loop => self.loop_trail,
            TrailKind::Tree => self.tree_trail,
            TrailKind::Forced => self.forced_trail,
            TrailKind::Bank => self.bank_trail,
            TrailKind::Port => self.port_trail,
            TrailKind::Pinned => self.pinned_trail,
        }
    }

    /// Interpolates the height palette at an absolute height above regional outlets.
    ///
    /// `height_m` is in metres, before preview exaggeration. The normalised height
    /// is clamped to 0–1. The palette must be nonempty, have ascending stops,
    /// and span the configured range; [`Self::colour_top_m`] must be positive.
    pub fn ramp(&self, height_m: f32) -> LinearRgba {
        let x = (height_m / self.colour_top_m).clamp(0., 1.);
        let colour = |c: Color| c.to_linear();
        self.palette.windows(2).find(|pair| x <= pair[1].0).map_or(
            colour(self.palette[self.palette.len() - 1].1),
            |pair| {
                let (a, b) = (pair[0], pair[1]);
                let t = ((x - a.0) / (b.0 - a.0)).clamp(0., 1.);
                colour(a.1).mix(&colour(b.1), t)
            },
        )
    }
}

/// Presentation choices for a [`Board`], separate from generation settings.
///
/// [`panel`](super::panel) and picking edit the view or focus. [`systems`](super::systems)
/// updates surface colours, marker materials, and camera aim accordingly.
/// Changing these values does not regenerate the terrain or trail network.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexPathsView {
    /// Which measure colours the terrain.
    pub view: TerrainView,
    pub focus: CameraFocus,
    /// Shows every node's type and card, and draws tease lines.
    /// There is no per-node arrival or walking-based reveal state in this pass.
    pub reveal_nodes: bool,
    /// Camera pitch below the horizon on entering the screen, in degrees.
    pub initial_tilt: f32,
    /// Distance from the camera to the middle of a new board.
    pub initial_distance: f32,
    /// Distance from the camera to the player on focusing it.
    pub player_distance: f32,
}

impl Default for HexPathsView {
    fn default() -> Self {
        Self {
            view: TerrainView::default(),
            focus: CameraFocus::default(),
            reveal_nodes: false,
            initial_tilt: 60.,
            initial_distance: 30.,
            player_distance: 10.,
        }
    }
}

/// Root of the rendered board hierarchy, replaced by full board generation.
///
/// [`spawn_board`](super::systems::spawn_board) creates it, then attaches [`TerrainSurface`]
/// and marker children. Layout-only regeneration retains this root and rebuilds
/// its surface and markers. Screen cleanup despawns it through screen scoping.
#[derive(Component)]
pub struct BoardRoot;

/// Marks the rendered mesh built from [`Board::layout`]'s levelled heights.
///
/// [`spawn_surface`](super::systems::spawn_surface) replaces this child of [`BoardRoot`] when
/// the board, view, or appearance changes.
#[derive(Component)]
pub struct TerrainSurface;

/// Identifies the terrain-mesh tile beneath a [`Marker`] billboard.
///
/// Picking uses this tile to set [`CameraFocus::Node`], while [`Board::pad`]
/// positions the billboard on the levelled ground. The index belongs to the
/// current board's mesh, rather than being a stable node identity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pad(pub usize);

/// The displayed role of a billboard spawned from a [`Board`]'s layout.
///
/// [`paint_markers`](super::systems::paint_markers) selects a [`Billboards`] material from
/// this value and [`HexPathsView::reveal_nodes`]. Crossing nodes use gizmo
/// outlines rather than billboard markers.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// Spawn of a fresh layout, including unpinned “New layout” operations.
    Spawn,
    /// Where the player stood at the last regeneration: the new spawn.
    Player,
    /// A point of interest and the card dealt to it, if any.
    Poi(Option<Card>),
    /// A bench node, shown as unknown until nodes are revealed.
    Bench,
}

/// Marks the orbit camera reserved for this dev screen's board preview.
///
/// It renders the layer shared by the terrain, billboards, and [`HexPathsGizmos`].
/// [`CameraFocus`] sets its orbit centre; viewport fitting leaves room for
/// [`SettingsPanel`]. The UI is rendered by a separate camera.
#[derive(Component)]
pub struct HexPathsCamera;

/// Gizmo group that confines board overlays to [`HexPathsCamera`]'s render layer.
///
/// [`draw_board`](super::systems::draw_board) uses it for the grid, trails, creeks, crossings,
/// and tease lines. These overlays are temporary preview geometry; terrain
/// materials or meshes can replace them when trails modify the displayed ground.
#[derive(Default, GizmoConfigGroup, Reflect)]
pub struct HexPathsGizmos;

/// Marks the panel text updated from [`BoardStats`], [`Board`], and [`PanelState`].
/// [`update_stats`](super::panel::update_stats) also shows the active view and camera focus.
#[derive(Component, Default, Clone)]
pub struct StatsText;

/// Marks the panel's full-width unsigned seed editor for [`HexPathsSettings`].
/// Blank text selects a random seed; [`PanelState::seed_error`] records overflow.
/// Settings synchronisation avoids replacing text while this editor has focus.
#[derive(Component, Default, Clone)]
pub struct SeedInput;

/// Marks the scrollable panel whose width matches the reserved camera area.
/// [`resize_panel`](super::panel::resize_panel) reads [`HexPathsAppearance::panel_width`].
#[derive(Component, Default, Clone)]
pub struct SettingsPanel;

/// Connects a panel slider to one field of [`HexPathsSettings`].
///
/// The read function synchronises external settings edits into the control;
/// the write function applies the released slider value. This binding keeps
/// the UI generic without converting an entire settings resource to fields.
#[derive(Component, Clone, Copy)]
pub struct SettingSlider {
    pub read: fn(&HexPathsSettings) -> f32,
    pub write: fn(&mut HexPathsSettings, f32),
}

/// The measure used to colour the rendered [`TerrainSurface`].
///
/// [`Board::view_tint`] constructs a [`Tint`] for the selected measure in
/// [`HexPathsView`]. The ground stays the current layout's levelled surface;
/// changing this value changes colours rather than generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Reflect)]
pub enum TerrainView {
    /// Height and hillshade only.
    #[default]
    Heightmap,
    /// Steep edges tint toward [`HexPathsAppearance::cost`], using levelled heights.
    Cost,
    /// Original-ground buildability, sharpened by [`HexPathsSettings::spread`],
    /// tints toward [`HexPathsAppearance::score`]; water weighting is not displayed.
    Suitability,
    /// Quiet and overlook shares blend [`HexPathsAppearance::quiet`] and
    /// [`HexPathsAppearance::overlook`] over a paper background.
    BenchScore,
}

/// Per-point colour targets and blend weights for a [`TerrainView`].
///
/// [`Board::surface_mesh`] combines these with hillshade and the height palette.
/// Arrays follow mesh corners first, then tile centres, matching
/// [`point_heights`](super::terrain::point_heights). This is temporary rendering data.
pub struct Tint {
    /// Replaces the height colours underneath, if set.
    pub base: Option<LinearRgba>,
    pub colours: Vec<LinearRgba>,
    /// How far each point leans towards its colour, 0–1.
    pub amounts: Vec<f32>,
}

/// Selects the world-space orbit centre resolved by [`Board::focus`].
///
/// Stored in [`HexPathsView`] and edited by panel buttons or marker picking.
/// Node focus holds a tile index, not a stable node ID; [`Board::keep_focus`]
/// validates occupancy when a board or layout changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Reflect)]
pub enum CameraFocus {
    /// The middle of the board, from afar.
    #[default]
    Terrain,
    /// The current layout's spawn: the player's last regeneration position, or initial spawn.
    Player,
    /// The node on this tile, from close by.
    Node(usize),
}
