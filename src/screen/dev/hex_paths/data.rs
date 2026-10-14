//! ECS-facing settings, generated board state, and preview markers.
//!
//! [`HexPathsSettings`] and [`HexTerrainTuning`] feed region simulation, window
//! selection, and layout generation. [`RegionCache`] retains the last matching
//! [`Region`]; [`Board`] owns the chosen [`Terrain`], current [`Layout`], and
//! derived measurements. [`BoardStats`] and [`PanelState`] report failures.
//!
//! [`HexPathsView`] and [`HexPathsAppearance`] control presentation without
//! regenerating ground or trails. [`systems`](super::systems) renders the board with
//! [`TerrainSurface`], [`Marker`], and [`Billboard`] entities beneath [`BoardRoot`].
//! [`screen`](super::screen) creates shared assets on entry and removes play-time
//! resources on unload, while persistent settings remain available.

use super::{
    layout::{Card, Context, Layout, Pace, TrailKind},
    terrain::{
        Terrain,
        erosion::{Region, RegionKey},
        world,
    },
};
use crate::prelude::*;
use std::collections::BTreeSet;

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

/// Marks a preview quad whose rotation follows [`HexPathsCamera`].
///
/// [`spawn_markers`](super::systems::spawn_markers) adds this with [`Marker`] and [`Pad`];
/// [`face_camera`](super::systems::face_camera) turns it toward the board camera each frame.
/// If reused with multiple cameras, an explicit `BillboardTarget` relationship
/// could associate each billboard with its camera rather than assuming one.
#[derive(Component)]
pub struct Billboard;

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
        self.palette
            .windows(2)
            .find(|pair| x <= pair[1].0)
            .map_or(colour(self.palette[self.palette.len() - 1].1), |pair| {
                let (a, b) = (pair[0], pair[1]);
                let t = ((x - a.0) / (b.0 - a.0)).clamp(0., 1.);
                colour(a.1).mix(&colour(b.1), t)
            })
    }
}

/// The measure used to colour the rendered [`TerrainSurface`].
///
/// [`systems`](super::systems) constructs a [`Tint`] for the selected measure in
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
/// [`systems`](super::systems) combines these with hillshade and the height palette.
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

/// The generated state and measurements presented by the hex dev screen.
///
/// Owns one fixed [`Terrain`] selected from a larger [`Region`], and one
/// replaceable [`Layout`] of nodes, pads, and trails over it. [`Self::new`]
/// measures a fresh board; [`Self::replace_layout`] retains the terrain and
/// updates layout statistics during regeneration or a fresh layout request.
///
/// [`systems`](super::systems) bridges this resource into surface and marker entities,
/// while [`panel`](super::panel) displays its pace, held teases, and crossing checks.
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
    /// separately by [`relayout`](super::systems::relayout).
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
    /// camera focus; [`relayout`](super::systems::relayout) handles those after success.
    pub fn replace_layout(&mut self, layout: Layout, count: u32, knobs: (&HexPathsSettings, &HexTerrainTuning)) {
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
                |(min, max, sum, count), ((&p, &h), _)| (min.min(p), max.max(p), sum + h, count + 1.),
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
            CameraFocus::Node(tile) if !self.layout.all_nodes().any(|n| n.tile == tile) => CameraFocus::Player,
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
        world(self.terrain.mesh.centers[tile], (corners[2] + corners[3]) / 2. * vertical_scale)
    }
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

/// Primary generation knobs shared by the panel, terrain, and layout stages.
///
/// Metres and hectares describe erosion and creeks; counts and placement
/// preferences describe the board and its landmarks. [`screen`](super::screen) detects
/// changes and rebuilds the whole [`Board`], reusing [`RegionCache`] when the
/// erosion key matches. Explicit layout buttons rerun only [`Layout`].
/// Presentation choices belong to [`HexPathsView`] and [`HexPathsAppearance`].
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexPathsSettings {
    /// Playable board width in hex columns, excluding the scenery margin.
    pub columns: u32,
    /// Playable board height in hex rows, excluding the scenery margin.
    pub rows: u32,
    /// The terrain seed; `None` draws a fresh one every generation. It also
    /// seeds the first layout.
    pub seed: Option<u64>,
    /// Metres the region rises against its outlets each erosion pass; more
    /// gives deeper valleys.
    pub uplift_m: f32,
    /// Water gathered from this many hectares starts cutting a channel; less
    /// gives more ravines.
    pub channel_ha: f32,
    /// Channels draining this many hectares are creeks.
    pub creek_ha: f32,
    /// Points of interest to place, on buildable ground.
    pub pois: u32,
    /// Benches to place, at the best views and hollows.
    pub benches: u32,
    /// How strongly node sites prefer buildable ground; 0 ignores it.
    pub spread: f32,
    /// Bench preference, 0–1: 0 favours quiet, enclosed spots, 1 overlooks.
    pub view: f32,
    /// Requested crossing count after required connectivity sites are selected.
    /// The final count can differ after validity filtering. Zero disables creek
    /// exclusions and barriers for debugging.
    pub crossings: u32,
    /// Tease pairs: a view of a point of interest that is a long walk away.
    pub teases: u32,
    /// POI score multiplier near water: `1 + water_boost × exp(-distance / 2)`.
    /// Distance is in hex spacings; this affects weighted placement, not an
    /// independent probability. It applies when creek restrictions are enabled.
    pub water_boost: f32,
}

impl Default for HexPathsSettings {
    fn default() -> Self {
        Self {
            columns: 20,
            rows: 20,
            seed: None,
            uplift_m: 0.2,
            channel_ha: 2.,
            creek_ha: 40.,
            pois: 6,
            benches: 3,
            spread: 2.,
            view: 0.7,
            crossings: 2,
            teases: 2,
            water_boost: 1.,
        }
    }
}

impl HexPathsSettings {
    /// Checks board dimensions, positive uplift and channel thresholds, and score ranges.
    ///
    /// Returns the first failing constraint. [`spawn_board`](super::systems::spawn_board) calls
    /// this before constructing a [`RegionKey`]. Region fit and placement capacity
    /// are checked later by terrain and layout generation.
    pub fn validate(&self) -> Result<(), &'static str> {
        [
            (self.columns > 0 && self.rows > 0, "grid must not be empty"),
            (self.uplift_m > 0., "uplift must be positive"),
            (
                self.channel_ha > 0. && self.creek_ha >= self.channel_ha,
                "channels must start at a positive area, and creeks at no less",
            ),
            ((0. ..=1.).contains(&self.view), "view must be within 0..=1"),
            (
                self.spread >= 0. && self.water_boost >= 0.,
                "spread and water boost must not be negative",
            ),
        ]
        .into_iter()
        .find(|(ok, _)| !ok)
        .map_or(Ok(()), |(_, reason)| Err(reason))
    }
}

/// Weights and targets used by [`Window::choose`](super::terrain::window::Window::choose).
///
/// Creek, ford, and natural-view counts contribute normalised scores capped
/// at one. Relief adds a bonus inside its range; river-sized flow subtracts
/// a penalty. The highest total selects the board before actual nodes or
/// trails are placed. Stored within [`HexTerrainTuning::window`].
#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct WindowWeights {
    /// Creek tiles in the playable area, full marks at `creek_tiles`.
    pub creek: f32,
    pub creek_tiles: f32,
    /// Ford sites (main-creek tiles whose corners span at most
    /// `ford_bank_m`), `ford_spacing` hexes apart; full marks at
    /// `fords_wanted`.
    pub fords: f32,
    pub fords_wanted: f32,
    pub ford_bank_m: f32,
    pub ford_spacing: f32,
    /// Natural teases: tiles that see a tile 3–6 hexes away at least
    /// `tease_drop_m` lower; full marks at `viewers`.
    pub teases: f32,
    pub viewers: f32,
    pub tease_drop_m: f32,
    /// A creek, not a river: a penalty when the biggest flow passes
    /// `river_ha`.
    pub river: f32,
    pub river_ha: f32,
    /// Weak preference for relief within `relief_m`.
    pub relief: f32,
    pub relief_m: Vec2,
}

impl Default for WindowWeights {
    fn default() -> Self {
        Self {
            creek: 1.,
            creek_tiles: 40.,
            fords: 1.,
            fords_wanted: 2.,
            ford_bank_m: 3.,
            ford_spacing: 4.,
            teases: 1.,
            viewers: 4.,
            tease_drop_m: 12.,
            river: 1.,
            river_ha: 600.,
            relief: 0.25,
            relief_m: Vec2::new(15., 60.),
        }
    }
}

/// Physical scale and algorithm controls shared by all generation stages.
///
/// [`RegionKey`] selects erosion inputs; [`WindowWeights`] controls candidate
/// selection; [`Terrain`] computes site scores; [`Layout`] reads placement,
/// tease, routing, and pin controls through [`Context`]. Lengths annotated as
/// metres are converted by [`Self::edges`]; planar hex distances generally
/// mean centre spacings of √3 edge lengths.
///
/// Values follow hex-lab's `exp/15-density`, `trunk-sight` arm. Editing this
/// resource requests full board regeneration. Visual exaggeration and colours
/// are separate in [`HexPathsAppearance`].
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexTerrainTuning {
    /// Extra hexes of scenery around the playable board. Even, to keep the
    /// even-q offset parity.
    pub margin: u32,
    /// Length of one hex edge, in metres.
    pub edge_m: f32,
    /// Board raster cells per hex edge length.
    pub resolution: f32,
    /// Regional width in hex columns; simulation uses `1.5 × region_hexes` square cells per side.
    pub region_hexes: u32,
    /// Starting height rise along the region X axis, in metres per 100 horizontal metres.
    pub regional_pct: f32,
    /// Standard-deviation scale of the initial blurred rolls, in metres.
    pub roll_m: f32,
    /// Roll length scale in metres; Gaussian sigma is half this scale.
    pub roll_size_m: f32,
    /// White-noise roughness on the starting surface; breaks grid-straight
    /// channels.
    pub roughness_m: f32,
    /// Metres cut per pass by a channel draining 1 km² on a 1:10 slope.
    pub erodibility: f32,
    /// Erosion passes.
    pub iterations: u32,
    /// Slope creep per pass, as a fraction of the 4-neighbour Laplacian
    /// (below 0.25).
    pub creep: f32,
    /// Random windows ranked; the best becomes the board.
    pub candidates: u32,
    /// Scoring weights and feature thresholds for selecting the playable window.
    pub window: WindowWeights,
    /// Exponent controlling symmetric terrain route weights: `exp(aversion × |rise|)`.
    /// Rise is in edge lengths. Actual walk minutes use Tobler's full speed curve.
    pub aversion: f32,
    /// Nodes keep at least this many hexes apart while they can.
    pub min_spacing: f32,
    /// How far a bench's view counts, in hexes.
    pub view_radius: f32,
    /// Eye height above the ground, in metres.
    pub eye_m: f32,
    /// Height of whatever marks a node (a roof, a post), in metres.
    pub landmark_m: f32,
    /// Ground this far below a viewer counts fully toward an overlook.
    pub overlook_m: f32,
    /// Tease ranking stops rewarding drops beyond this.
    pub tease_top_m: f32,
    /// Teases are this many hexes apart at most.
    pub sight: f32,
    /// A tease holds while walking to its target is at least this many times
    /// the open ground between the pads.
    pub tease_detour: f32,
    /// Guarded builds attempted after the tease-free pass; zero still attempts one.
    pub tease_retries: u32,
    /// Exponent on playable quiet/overlook ranks before blending bench scores.
    /// Higher values concentrate preference on exceptional sites; one leaves
    /// the ranks unsharpened.
    pub bench_sharpness: f32,
    /// The spanner builds a trail when the network makes you walk more than
    /// this many times the direct cost.
    pub stretch: f32,
    /// Cost multiplier for walking an existing trail, so trails merge.
    pub reuse: f32,
    /// Cost multiplier for walking beside a trail without joining it.
    pub alongside: f32,
    /// Maximum connector edge count in the cheap-loop pass; the no-tree pass has no cap.
    pub max_connector: u32,
    /// Cheap-loop cost limit relative to the node's nearest existing link.
    /// Spawn has no cost limit; the later no-tree pass removes this limit for all nodes.
    pub cheap_loop: f32,
    /// Minimum connector length plus shortest contracted return hops for loop candidates.
    pub min_loop: u32,
    /// Allowed rise per edge length while grading near a pad.
    /// The height adjustment fades farther away, so this is not a global slope limit.
    pub grade: f32,
    /// A part of the board cut off by the main creek needs this many dry
    /// interior tiles to be worth a crossing.
    pub min_part: u32,
    /// Partial-regeneration radius in hex spacings around the player.
    /// Nearby nodes and trail edges with both endpoints inside are pinned;
    /// crossings and the node underfoot are always retained.
    pub pin_radius: f32,
    /// Preferred minimum/maximum movement in hex spacings; placement can relax this annulus.
    pub displacement: Vec2,
}

impl Default for HexTerrainTuning {
    fn default() -> Self {
        Self {
            margin: 6,
            edge_m: 33.,
            resolution: 2.,
            region_hexes: 100,
            regional_pct: 0.2,
            roll_m: 4.,
            roll_size_m: 1500.,
            roughness_m: 1.,
            erodibility: 60.,
            iterations: 250,
            creep: 0.08,
            candidates: 60,
            window: WindowWeights::default(),
            aversion: 3.5,
            min_spacing: 3.,
            view_radius: 6.,
            eye_m: 1.5,
            landmark_m: 4.,
            overlook_m: 12.,
            tease_top_m: 24.,
            sight: 6.,
            tease_detour: 3.,
            tease_retries: 4,
            bench_sharpness: 4.,
            stretch: 2.,
            reuse: 0.6,
            alongside: 2.5,
            max_connector: 18,
            cheap_loop: 3.,
            min_loop: 14,
            grade: 0.5,
            min_part: 7,
            pin_radius: 4.,
            displacement: Vec2::new(2., 4.),
        }
    }
}

impl HexTerrainTuning {
    /// Checks margin parity, positive scale/resolution, candidates, creep, and preference ranges.
    ///
    /// Returns the first failing constraint. This is a basic settings check, not
    /// validation of every physical input; [`spawn_board`](super::systems::spawn_board) also
    /// checks primary settings before generation.
    pub fn validate(&self) -> Result<(), &'static str> {
        [
            (self.margin.is_multiple_of(2), "margin must be even"),
            (
                self.edge_m > 0. && self.resolution > 0.,
                "edge length and resolution must be positive",
            ),
            (self.candidates > 0, "at least one window must be ranked"),
            ((0. ..0.25).contains(&self.creep), "creep must be within 0..0.25"),
            (self.bench_sharpness > 0., "bench sharpness must be positive"),
            (
                self.displacement.x <= self.displacement.y,
                "displacement must be a range",
            ),
        ]
        .into_iter()
        .find(|(ok, _)| !ok)
        .map_or(Ok(()), |(_, reason)| Err(reason))
    }

    /// Converts metre height or distance to generator edge lengths using [`Self::edge_m`].
    /// The region uses metres; terrain and layout height arrays use this unit.
    pub fn edges(&self, metres: f32) -> f32 {
        metres / self.edge_m
    }
}
