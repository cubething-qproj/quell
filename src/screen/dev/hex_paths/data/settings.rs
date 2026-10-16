//! Physical scale, window ranking, and placement/routing controls.
//!
//! [`HexPathsSettings`] supplies primary generation knobs; [`HexTerrainTuning`]
//! supplies algorithm controls including [`WindowWeights`]. The screen detects
//! changes to these assets and rebuilds the board, reusing cached erosion.

use bevy::{asset::AssetMut, ecs::system::SystemParam};
use serde::Deserialize;

use crate::prelude::*;

/// Handles to the RON-authored [`HexPathsSettings`] and [`HexTerrainTuning`].
///
/// Loaded by `bevy_asset_loader` when the data plugin is built. Systems read
/// the assets through [`HexSettings`] or [`HexSettingsMut`]; marking this
/// resource changed requests a full board rebuild, as does [`Self::changed`].
#[derive(Default, AssetCollection, Resource, Reflect, Debug, Clone, PartialEq)]
pub struct HexSettingsCollection {
    #[asset(path = "terrain/hex.paths.ron")]
    pub paths: Handle<HexPathsSettings>,
    #[asset(path = "terrain/hex.tuning.ron")]
    pub terrain: Handle<HexTerrainTuning>,
}

impl HexSettingsCollection {
    /// Run condition for full board generation.
    ///
    /// True when either settings asset was added (loaded) or modified (hot
    /// reload, panel edit), or when this resource was marked changed (screen
    /// entry, the Regenerate button). Both readers are drained every run.
    pub fn changed(
        collection: Res<Self>,
        mut paths: MessageReader<AssetEvent<HexPathsSettings>>,
        mut terrain: MessageReader<AssetEvent<HexTerrainTuning>>,
    ) -> bool {
        let (p, t) = (&collection.paths, &collection.terrain);
        let paths = paths.read().filter(|e| e.is_added(p) || e.is_modified(p));
        let terrain = terrain.read().filter(|e| e.is_added(t) || e.is_modified(t));
        let edits = paths.count() + terrain.count();
        collection.is_changed() || edits > 0
    }
}

/// Read access to the loaded settings assets named by [`HexSettingsCollection`].
///
/// Accessors return `None` until loading finishes; systems should bail rather
/// than panic. Pure generation code takes the plain references from [`Self::get`].
#[derive(SystemParam)]
pub struct HexSettings<'w> {
    collection: Res<'w, HexSettingsCollection>,
    paths: Res<'w, Assets<HexPathsSettings>>,
    terrain: Res<'w, Assets<HexTerrainTuning>>,
}

impl<'w> HexSettings<'w> {
    /// Both settings assets, once both are loaded.
    pub fn get(&self) -> Option<(&HexPathsSettings, &HexTerrainTuning)> {
        self.paths().zip(self.terrain())
    }
    /// The primary generation knobs, once loaded.
    pub fn paths(&self) -> Option<&HexPathsSettings> {
        self.paths.get(self.collection.paths.id())
    }
    /// The algorithm tuning, once loaded.
    pub fn terrain(&self) -> Option<&HexTerrainTuning> {
        self.terrain.get(self.collection.terrain.id())
    }
    /// The handle collection, e.g. for change detection.
    pub fn collection(&self) -> &Res<'w, HexSettingsCollection> {
        &self.collection
    }
}

/// Write access to the [`HexPathsSettings`] asset named by [`HexSettingsCollection`].
///
/// Mutating through [`AssetMut`] emits [`AssetEvent::Modified`], which
/// [`HexSettingsCollection::changed`] turns into a board rebuild. Tuning is
/// not panel-editable; edit its RON file and rely on hot reload instead.
#[derive(SystemParam)]
pub struct HexSettingsMut<'w> {
    collection: Res<'w, HexSettingsCollection>,
    paths: ResMut<'w, Assets<HexPathsSettings>>,
}

impl HexSettingsMut<'_> {
    /// Mutable primary knobs, once loaded; only dereferencing mutably marks them modified.
    pub fn paths_mut(&mut self) -> Option<AssetMut<'_, HexPathsSettings>> {
        self.paths.get_mut(self.collection.paths.id())
    }
}

/// Primary generation knobs shared by the panel, terrain, and layout stages.
///
/// Metres and hectares describe erosion and creeks; counts and placement
/// preferences describe the board and its landmarks. [`screen`](super::super::screen) detects
/// changes and rebuilds the whole [`Board`](super::Board), reusing [`RegionCache`](super::RegionCache) when the
/// erosion key matches. Explicit layout buttons rerun only [`Layout`](super::super::layout::Layout).
/// Presentation choices belong to [`HexPathsView`](super::HexPathsView) and [`HexPathsAppearance`](super::HexPathsAppearance).
#[derive(Debug, Clone, PartialEq, Reflect, Asset, Deserialize)]
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
    /// Returns the first failing constraint. [`spawn_board`](super::super::systems::spawn_board) calls
    /// this before constructing a [`RegionKey`](super::super::terrain::erosion::RegionKey). Region fit and placement capacity
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

/// Weights and targets used by [`Window::choose`](super::super::terrain::window::Window::choose).
///
/// Creek, ford, and natural-view counts contribute normalised scores capped
/// at one. Relief adds a bonus inside its range; river-sized flow subtracts
/// a penalty. The highest total selects the board before actual nodes or
/// trails are placed. Stored within [`HexTerrainTuning::window`].
#[derive(Debug, Clone, PartialEq, Reflect, Deserialize)]
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
/// [`RegionKey`](super::super::terrain::erosion::RegionKey) selects erosion inputs; [`WindowWeights`] controls candidate
/// selection; [`Terrain`](super::super::terrain::Terrain) computes site scores; [`Layout`](super::super::layout::Layout) reads placement,
/// tease, routing, and pin controls through [`Context`](super::super::layout::Context). Lengths annotated as
/// metres are converted by [`Self::edges`]; planar hex distances generally
/// mean centre spacings of √3 edge lengths.
///
/// Values follow hex-lab's `exp/15-density`, `trunk-sight` arm. Editing this
/// resource requests full board regeneration. Visual exaggeration and colours
/// are separate in [`HexPathsAppearance`](super::HexPathsAppearance).
#[derive(Asset, Debug, Clone, PartialEq, Reflect, Deserialize)]
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
    /// validation of every physical input; [`spawn_board`](super::super::systems::spawn_board) also
    /// checks primary settings before generation.
    pub fn validate(&self) -> Result<(), &'static str> {
        [
            (self.margin.is_multiple_of(2), "margin must be even"),
            (
                self.edge_m > 0. && self.resolution > 0.,
                "edge length and resolution must be positive",
            ),
            (self.candidates > 0, "at least one window must be ranked"),
            (
                (0. ..0.25).contains(&self.creep),
                "creep must be within 0..0.25",
            ),
            (
                self.bench_sharpness > 0.,
                "bench sharpness must be positive",
            ),
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
