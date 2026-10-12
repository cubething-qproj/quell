use super::terrain::Terrain;
use crate::prelude::*;

/// Root of the entities replaced when the board regenerates.
#[derive(Component)]
pub struct BoardRoot;

/// The terrain surface, rebuilt when the board or its appearance changes.
#[derive(Component)]
pub struct TerrainSurface;

/// Camera rendering only the hex board.
#[derive(Component)]
pub struct HexPathsCamera;

/// UI camera state restored when the board stops compositing beneath it.
#[derive(Resource)]
pub struct UiCameraState {
    pub entity: Entity,
    pub clear_color: ClearColorConfig,
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

/// Typed binding between a number input and a generation setting.
#[derive(Component, Clone, Copy)]
pub struct NumberSetting<T: Send + Sync + 'static> {
    pub read: fn(&HexPathsSettings) -> T,
    pub write: fn(&mut HexPathsSettings, T),
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
}

/// How the board is viewed. Editing these never regenerates a board.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexPathsView {
    /// Which measure colours the terrain.
    pub view: TerrainView,
    /// Camera pitch below the horizon on entering the screen, in degrees.
    pub initial_tilt: f32,
    /// Distance from the camera to the middle of a new board.
    pub initial_distance: f32,
}

impl Default for HexPathsView {
    fn default() -> Self {
        Self {
            view: TerrainView::default(),
            initial_tilt: 60.,
            initial_distance: 30.,
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
}

impl Default for HexPathsSettings {
    fn default() -> Self {
        Self {
            columns: 10,
            rows: 8,
            seed: None,
            scale: 8.,
            relief: 6.,
            roughness: 0.5,
            ridges: 0.3,
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
                share.contains(&self.roughness) && share.contains(&self.ridges),
                "roughness and ridges must be within 0..=1",
            ),
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
}

impl Default for HexTerrainTuning {
    fn default() -> Self {
        Self {
            margin: 6,
            resolution: 4.,
            octaves: 4,
            valleys: 1.5,
            aversion: 3.5,
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
        ]
        .into_iter()
        .find(|(ok, _)| !ok)
        .map_or(Ok(()), |(_, reason)| Err(reason))
    }
}
