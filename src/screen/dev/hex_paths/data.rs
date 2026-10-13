use super::terrain::Terrain;
use crate::prelude::*;

/// Root of the entities replaced when the board regenerates.
#[derive(Component)]
pub struct BoardRoot;

/// The terrain surface, rebuilt when the board or its appearance changes.
#[derive(Component)]
pub struct TerrainSurface;

/// A quad that turns to face the camera.
#[derive(Component)]
pub struct Billboard;

/// The tile of the pad a marker stands on.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pad(pub usize);

/// What a billboard marks.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Spawn,
    Poi,
    Bench,
}

/// Camera rendering only the hex board.
#[derive(Component)]
pub struct HexPathsCamera;

/// UI camera state restored when the board stops compositing beneath it.
#[derive(Resource)]
pub struct UiCameraState {
    pub entity: Entity,
    pub clear_color: ClearColorConfig,
}

/// Shared billboard quad and materials, loaded with the screen.
#[derive(Resource)]
pub struct Billboards {
    /// Upright quad standing on its origin.
    pub quad: Handle<Mesh>,
    pub spawn: Handle<StandardMaterial>,
    pub bench: Handle<StandardMaterial>,
    /// A node whose type is not yet known.
    pub unknown: Handle<StandardMaterial>,
}

/// Separate gizmo layer for the hex camera.
#[derive(Default, GizmoConfigGroup, Reflect)]
pub struct HexPathsGizmos;

/// Text displaying the generated seed and errors.
#[derive(Component, Default, Clone)]
pub struct StatsText;

/// Text input for a full-width unsigned seed; blank means random.
#[derive(Component, Default, Clone)]
pub struct SeedInput;

/// Root node of the scrollable settings panel.
#[derive(Component, Default, Clone)]
pub struct SettingsPanel;

/// Error in the seed text being edited.
#[derive(Resource, Default)]
pub struct PanelState {
    pub seed_error: Option<&'static str>,
}

/// Binding between a slider and a generation setting.
#[derive(Component, Clone, Copy)]
pub struct SettingSlider {
    pub read: fn(&HexPathsSettings) -> f32,
    pub write: fn(&mut HexPathsSettings, f32),
}

/// Sizes and colors for the board preview and its debug panel.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct HexPathsAppearance {
    pub panel_width: f32,
    /// Zoom per mouse wheel line, in the same units as pixel scrolling.
    pub zoom_per_line: f32,
    pub grid: Color,
    /// Grid lines float this far above the terrain.
    pub grid_lift: f32,
    /// Height colour stops, by height as a fraction of `colour_top`.
    pub palette: Vec<(f32, Color)>,
    /// Height drawn in the palette's last colour.
    pub colour_top: f32,
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
            colour_top: 9.,
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
    /// Palette colour for `height`, interpolated between stops.
    pub fn ramp(&self, height: f32) -> LinearRgba {
        let x = (height / self.colour_top).clamp(0., 1.);
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

/// Which measure the terrain is coloured by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Reflect)]
pub enum TerrainView {
    /// Height and hillshade only.
    #[default]
    Heightmap,
    /// Steep edges darken towards `HexPathsAppearance::cost`.
    Cost,
    /// Buildable ground, as node placement sees it, tints towards
    /// `HexPathsAppearance::score`.
    Suitability,
    /// Good bench sites on paper: `HexPathsAppearance::quiet` for enclosed
    /// spots, through to `overlook` for views down.
    BenchScore,
}

/// How a view colours the terrain, per mesh point (corners, then centres).
pub struct Tint {
    /// Replaces the height colours underneath, if set.
    pub base: Option<LinearRgba>,
    pub colours: Vec<LinearRgba>,
    /// How far each point leans towards its colour, 0–1.
    pub amounts: Vec<f32>,
}

/// What the camera orbits and follows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Reflect)]
pub enum CameraFocus {
    /// The middle of the board, from afar.
    #[default]
    Terrain,
    /// The player, from close by. Spawn until there is a player.
    Player,
    /// The node on this tile, from close by.
    Node(usize),
}

/// How the board is viewed. Editing these never regenerates a board.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexPathsView {
    /// Which measure colours the terrain.
    pub view: TerrainView,
    pub focus: CameraFocus,
    /// Shows every node's type, not just those the player has reached.
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

/// The generated board and the middle of its playable area.
#[derive(Resource)]
pub struct Board {
    pub terrain: Terrain,
    /// Middle of the playable area, at its mean corner height.
    pub centre: Vec3,
}

impl Board {
    /// Where the camera looks for `focus`.
    pub fn focus(&self, focus: CameraFocus) -> Vec3 {
        match focus {
            CameraFocus::Terrain => self.centre,
            CameraFocus::Player => self.pad(self.terrain.spawn),
            CameraFocus::Node(tile) => self.pad(tile),
        }
    }

    /// The middle of the pad on `tile`, at its height.
    pub fn pad(&self, tile: usize) -> Vec3 {
        super::terrain::world(self.terrain.mesh.centers[tile], self.terrain.pad_height(tile))
    }

    /// Finds the middle of the playable area of `terrain`.
    pub fn new(terrain: Terrain) -> Self {
        let inside = terrain.mesh.playable_corners();
        let (min, max, sum, count) = terrain
            .mesh
            .positions
            .iter()
            .zip(&terrain.heights)
            .zip(&inside)
            .filter(|(_, inside)| **inside)
            .fold(
                (Vec2::MAX, Vec2::MIN, 0., 0.),
                |(min, max, sum, count), ((&p, &h), _)| (min.min(p), max.max(p), sum + h, count + 1.),
            );
        Self {
            centre: super::terrain::world((min + max) / 2., sum / count),
            terrain,
        }
    }
}

/// Seed and error from the latest generation attempt.
#[derive(Resource, Default)]
pub struct BoardStats {
    pub seed: u64,
    pub error: Option<&'static str>,
}

/// Board generation settings: the design's knobs. Heights are in hex edge
/// lengths. Changing these regenerates the board.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexPathsSettings {
    pub columns: u32,
    pub rows: u32,
    /// `None` draws a fresh seed every generation.
    pub seed: Option<u64>,
    /// Size of the largest hills, in hexes.
    pub scale: f32,
    /// Height of the highest ground.
    pub relief: f32,
    /// Share of each finer octave of noise, 0–1.
    pub roughness: f32,
    /// Blend towards ridged noise, 0–1.
    pub ridges: f32,
    /// Points of interest to place, on buildable ground.
    pub pois: u32,
    /// Benches to place, at the best views and hollows.
    pub benches: u32,
    /// How strongly node sites prefer buildable ground; 0 ignores it.
    pub spread: f32,
    /// Bench preference, 0–1: 0 favours quiet, enclosed spots, 1 overlooks.
    pub view: f32,
}

impl Default for HexPathsSettings {
    fn default() -> Self {
        Self {
            columns: 20,
            rows: 20,
            seed: None,
            scale: 8.,
            relief: 6.,
            roughness: 0.5,
            ridges: 0.3,
            pois: 6,
            benches: 3,
            spread: 2.,
            view: 0.7,
        }
    }
}

impl HexPathsSettings {
    /// Validates invariants on HexPathsSettings. Returns the first violated
    /// invariant.
    pub fn validate(&self) -> Result<(), &'static str> {
        let share = 0. ..=1.;
        [
            (self.columns > 0 && self.rows > 0, "grid must not be empty"),
            (self.scale > 0., "scale must be positive"),
            (self.relief >= 0., "relief must not be negative"),
            (
                share.contains(&self.roughness)
                    && share.contains(&self.ridges)
                    && share.contains(&self.view),
                "roughness, ridges and view must be within 0..=1",
            ),
            (self.spread >= 0., "spread must not be negative"),
        ]
        .into_iter()
        .find(|(ok, _)| !ok)
        .map_or(Ok(()), |(_, reason)| Err(reason))
    }
}

/// The prototype's internal constants. Changing these regenerates the board.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexTerrainTuning {
    /// Extra hexes of scenery around the playable board. Even, to keep the
    /// even-q offset parity.
    pub margin: u32,
    /// Raster cells per hex edge length.
    pub resolution: f32,
    /// Noise octaves, each half the size of the last.
    pub octaves: u32,
    /// Exponent that flattens lowlands.
    pub valleys: f32,
    /// Steepness aversion in Tobler's hiking function: a step costs
    /// `exp(aversion × |rise|)`.
    pub aversion: f32,
    /// Nodes keep at least this many hexes apart while they can.
    pub min_spacing: f32,
    /// How far a bench's view counts, in hexes.
    pub view_radius: f32,
    /// Eye height above the ground, for views.
    pub eye_height: f32,
    /// Exponent on the quiet and overlook ranks before they blend into the
    /// bench score. 1 is the prototype; higher keeps only the extremes.
    pub bench_sharpness: f32,
}

impl Default for HexTerrainTuning {
    fn default() -> Self {
        Self {
            margin: 6,
            resolution: 4.,
            octaves: 4,
            valleys: 1.5,
            aversion: 3.5,
            min_spacing: 3.,
            view_radius: 6.,
            eye_height: 0.5,
            bench_sharpness: 4.,
        }
    }
}

impl HexTerrainTuning {
    /// Validates invariants on HexTerrainTuning. Returns the first violated
    /// invariant.
    pub fn validate(&self) -> Result<(), &'static str> {
        [
            (self.margin.is_multiple_of(2), "margin must be even"),
            (self.resolution > 0., "resolution must be positive"),
            (self.octaves > 0, "octaves must be positive"),
            (self.valleys > 0., "valleys must be positive"),
            (self.bench_sharpness > 0., "bench sharpness must be positive"),
        ]
        .into_iter()
        .find(|(ok, _)| !ok)
        .map_or(Ok(()), |(_, reason)| Err(reason))
    }
}
