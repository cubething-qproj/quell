use crate::prelude::*;
use hexx::{Hex, HexLayout, HexOrientation, OffsetHexMode};

const OFFSET_MODE: OffsetHexMode = OffsetHexMode::Even;

/// Root of the entities replaced when the board regenerates.
#[derive(Component)]
pub struct BoardRoot;

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
    pub board_padding: f32,
    pub grid: Color,
}

impl Default for HexPathsAppearance {
    fn default() -> Self {
        Self {
            panel_width: 340.,
            board_padding: 1.,
            grid: Color::srgb(0.25, 0.28, 0.32),
        }
    }
}

/// Play-time geometry, with the bounds of the generated grid.
#[derive(Resource)]
pub struct Board {
    pub layout: HexLayout,
    pub size: Vec2,
    pub columns: u32,
    pub rows: u32,
}

impl Board {
    /// Centers the grid in world space, keeping its full hex outlines in view.
    pub fn new(settings: &HexPathsSettings) -> Self {
        let mut layout = HexLayout::flat();
        let (min, max) = settings
            .cells()
            .flat_map(|hex| layout.hex_corners(hex))
            .fold(
                (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                |(min, max), point| (min.min(point), max.max(point)),
            );
        layout.origin = -(min + max) / 2.;
        Self {
            layout,
            size: max - min,
            columns: settings.columns,
            rows: settings.rows,
        }
    }

    /// Cells inside the generated grid, independent of current settings edits.
    pub fn cells(&self) -> impl Iterator<Item = Hex> + '_ {
        (0..self.columns).flat_map(move |column| {
            (0..self.rows).map(move |row| {
                Hex::from_offset_coordinates(
                    [column as i32, row as i32],
                    OFFSET_MODE,
                    HexOrientation::Flat,
                )
            })
        })
    }
}

/// Seed and error from the latest generation attempt.
#[derive(Resource, Default)]
pub struct BoardStats {
    pub seed: u64,
    pub error: Option<&'static str>,
}

/// Board generation settings. Changing these regenerates the board.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HexPathsSettings {
    pub columns: u32,
    pub rows: u32,
    /// `None` draws a fresh seed every generation.
    pub seed: Option<u64>,
}

impl Default for HexPathsSettings {
    fn default() -> Self {
        Self {
            columns: 10,
            rows: 8,
            seed: None,
        }
    }
}

impl HexPathsSettings {
    /// The cell at offset coordinates `column`, `row`.
    pub fn cell(&self, column: u32, row: u32) -> Hex {
        Hex::from_offset_coordinates(
            [column as i32, row as i32],
            OFFSET_MODE,
            HexOrientation::Flat,
        )
    }

    /// All grid cells, column by column.
    pub fn cells(&self) -> impl Iterator<Item = Hex> + '_ {
        (0..self.columns)
            .flat_map(move |column| (0..self.rows).map(move |row| self.cell(column, row)))
    }

    /// Validates invariants on HexPathsSettings. Returns the first violated
    /// invariant.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.columns > 0 && self.rows > 0 {
            Ok(())
        } else {
            Err("grid must not be empty")
        }
    }
}
