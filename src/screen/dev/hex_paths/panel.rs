//! Controls and statistics for the hex dev-screen preview.
//!
//! [`scene`] binds primary generation controls to [`HexPathsSettings`], view
//! controls to [`HexPathsView`], and layout buttons to [`relayout`](systems::relayout).
//! Slider values apply on release; successful seed edits apply immediately.
//! The screen detects settings changes and replaces the board, whereas explicit
//! layout buttons reuse its terrain. Internal [`HexTerrainTuning`] controls
//! are registered resources but do not all have panel sliders.
//!
//! [`sync_settings`] reflects external edits without replacing focused seed
//! text. [`update_stats`] reports [`Board`]'s window and layout measurements
//! alongside [`BoardStats`] and [`PanelState`] errors.

use super::{data::*, layout::NodeKind, systems};
use crate::prelude::*;
use bevy::{
    feathers::{
        controls::{FeathersButton, FeathersSlider, FeathersTextInput, FeathersTextInputContainer},
        display::{label, label_small},
        theme::ThemeBackgroundColor,
        tokens,
    },
    input_focus::{InputFocus, tab_navigation::TabGroup},
    text::{EditableText, EditableTextFilter, TextEdit, TextEditChange},
    ui_widgets::{Activate, ScrollArea, SliderPrecision, SliderStep, SliderValue, ValueChange},
};

/// Builds the scrollable panel for generation knobs, views, and layout requests.
///
/// Uses [`SettingSlider`] bindings for primary [`HexPathsSettings`] fields,
/// buttons for [`HexPathsView`], and closures that call [`relayout`](systems::relayout).
/// The internal tuning resource is not fully exposed as panel controls.
pub(super) fn scene(width: f32) -> impl Scene {
    bsn! {
        ScreenScoped SettingsPanel TabGroup ScrollArea
        Node {
            position_type: PositionType::Absolute,
            left: px(0), top: px(0), width: px(width), height: percent(100),
            overflow: Overflow::scroll_y(),
        }
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children [(
            Node {
                width: percent(100), flex_shrink: 0.,
                flex_direction: FlexDirection::Column,
                padding: px(12), row_gap: px(6),
            }
            Children [
                label("Hex paths"),
                (@FeathersButton { @caption: bsn! { label("Regenerate") } }
                    on(|_: On<Activate>, mut settings: ResMut<HexSettingsCollection>| { settings.set_changed(); })),
                (label_small("") StatsText),
                label("View"),
                (Node { column_gap: px(4), flex_wrap: FlexWrap::Wrap, row_gap: px(4) } Children [
                    (@FeathersButton { @caption: bsn! { label("Heightmap") } }
                        on(|_: On<Activate>, mut v: ResMut<HexPathsView>| { v.view = TerrainView::Heightmap; })),
                    (@FeathersButton { @caption: bsn! { label("Cost") } }
                        on(|_: On<Activate>, mut v: ResMut<HexPathsView>| { v.view = TerrainView::Cost; })),
                    (@FeathersButton { @caption: bsn! { label("Suitability") } }
                        on(|_: On<Activate>, mut v: ResMut<HexPathsView>| { v.view = TerrainView::Suitability; })),
                    (@FeathersButton { @caption: bsn! { label("Bench score") } }
                        on(|_: On<Activate>, mut v: ResMut<HexPathsView>| { v.view = TerrainView::BenchScore; })),
                ]),
                (@FeathersButton { @caption: bsn! { label("Reveal nodes") } }
                    on(|_: On<Activate>, mut v: ResMut<HexPathsView>| { v.reveal_nodes = !v.reveal_nodes; })),
                label_small("Layout (same terrain; regenerate round the focus)"),
                (Node { column_gap: px(4), flex_wrap: FlexWrap::Wrap, row_gap: px(4) } Children [
                    (@FeathersButton { @caption: bsn! { label("New layout") } } on(relayout(None))),
                    (@FeathersButton { @caption: bsn! { label("Regenerate here") } } on(relayout(Some(false)))),
                    (@FeathersButton { @caption: bsn! { label("Fear here") } } on(relayout(Some(true)))),
                ]),
                label_small("Camera focus (or click a marker)"),
                (Node { column_gap: px(4) } Children [
                    (@FeathersButton { @caption: bsn! { label("Terrain") } }
                        on(|_: On<Activate>, mut v: ResMut<HexPathsView>| { v.focus = CameraFocus::Terrain; })),
                    (@FeathersButton { @caption: bsn! { label("Player") } }
                        on(|_: On<Activate>, mut v: ResMut<HexPathsView>| { v.focus = CameraFocus::Player; })),
                ]),
                label("Generation"),
                label_small("Seed (blank = random)"),
                (@FeathersTextInputContainer Children [(
                    @FeathersTextInput { @max_characters: 20usize }
                    SeedInput EditableTextFilter::new(|c| c.is_ascii_digit())
                    on(set_seed)
                )]),
                (@FeathersButton { @caption: bsn! { label("Pin current seed") } }
                    on(|_: On<Activate>, stats: Option<Res<BoardStats>>, mut settings: HexSettingsMut| {
                        if let (Some(stats), Some(mut paths)) = (stats, settings.paths_mut()) { paths.seed = Some(stats.seed); }
                    })),
                slider("Columns", 4., 40., 1., |s| s.columns as f32, |s, v| s.columns = v as u32),
                slider("Rows", 4., 40., 1., |s| s.rows as f32, |s, v| s.rows = v as u32),
                label("Terrain"),
                slider("Uplift per pass (m)", 0.05, 1., 0.05, |s| s.uplift_m, |s, v| s.uplift_m = v),
                slider("Channels start at (ha)", 0.5, 10., 0.5, |s| s.channel_ha, |s, v| s.channel_ha = v),
                slider("Creeks start at (ha)", 10., 200., 5., |s| s.creek_ha, |s, v| s.creek_ha = v),
                label("Nodes"),
                slider("Points of interest", 0., 20., 1., |s| s.pois as f32, |s, v| s.pois = v as u32),
                slider("Benches", 0., 10., 1., |s| s.benches as f32, |s, v| s.benches = v as u32),
                slider("Prefer flat ground (spread)", 0., 4., 0.1, |s| s.spread, |s, v| s.spread = v),
                slider("Bench view: quiet 0 – overlook 1", 0., 1., 0.05, |s| s.view, |s, v| s.view = v),
                slider("Pull to water", 0., 3., 0.25, |s| s.water_boost, |s, v| s.water_boost = v),
                label("Trails"),
                slider("Crossings (0 ignores creeks)", 0., 4., 1., |s| s.crossings as f32, |s, v| s.crossings = v as u32),
                slider("Teases", 0., 4., 1., |s| s.teases as f32, |s, v| s.teases = v as u32),
            ]
        )]
    }
}

/// Creates a layout-button handler that reuses the current board's terrain.
///
/// `None` requests fresh placement; `Some(false)` requests partial regeneration;
/// `Some(true)` requests Fear. [`relayout`](systems::relayout) handles focus, seeds,
/// measurements, and errors. No board means there is no request to perform.
fn relayout(
    regeneration: Option<bool>,
) -> impl FnMut(
    On<Activate>,
    HexSettings,
    ResMut<HexPathsView>,
    Option<ResMut<Board>>,
    ResMut<PanelState>,
) + Clone {
    move |_, settings, mut view, board, mut panel| {
        if let (Some(mut board), Some((settings, tuning))) = (board, settings.get()) {
            systems::relayout(
                regeneration,
                settings,
                tuning,
                &mut view,
                &mut board,
                &mut panel,
            );
        }
    }
}

/// Creates a slider bound to a primary generation setting through [`SettingSlider`].
///
/// Dragging updates its displayed value; release applies the setting through
/// [`set_slider`], causing the screen to request full board generation.
fn slider(
    caption: &'static str,
    min: f32,
    max: f32,
    step: f32,
    read: fn(&HexPathsSettings) -> f32,
    write: fn(&mut HexPathsSettings, f32),
) -> impl Scene {
    let precision = if step < 1. { 2 } else { 0 };
    bsn! {
        Node { align_items: AlignItems::Center, column_gap: px(8) }
        Children [
            (label_small(caption) Node { width: percent(50) }),
            (@FeathersSlider { @min: min, @max: max, @value: min }
                Node { flex_grow: 1. }
                SliderStep(step) SliderPrecision(precision)
                template(move |_| Ok(SettingSlider { read, write }))
                on(set_slider)),
        ]
    }
}

/// Shows a slider's value while it is dragged and applies it on release.
fn set_slider(
    change: On<ValueChange<f32>>,
    mut commands: Commands,
    fields: Query<&SettingSlider>,
    mut settings: HexSettingsMut,
) {
    commands
        .entity(change.source)
        .insert(SliderValue(change.value));
    if change.is_final
        && let Ok(field) = fields.get(change.source)
        && let Some(mut settings) = settings.paths_mut()
    {
        let mut next = settings.clone();
        (field.write)(&mut next, change.value);
        if *settings != next {
            *settings = next;
        }
    }
}

/// Parses the seed editor directly as `u64`, with blank text meaning random.
///
/// Successful edits update [`HexPathsSettings`] immediately when the parsed
/// value differs. Overflow records [`PanelState::seed_error`] and leaves the
/// last valid setting intact; no narrowing through signed integers or floats occurs.
fn set_seed(
    change: On<TextEditChange>,
    inputs: Query<&EditableText, With<SeedInput>>,
    mut settings: HexSettingsMut,
    mut panel: ResMut<PanelState>,
) {
    let Ok(input) = inputs.get(change.event_target()) else {
        return;
    };
    let mut settings = r!(settings.paths_mut());
    let text = input.value().to_string();
    let seed = if text.is_empty() {
        Ok(None)
    } else {
        text.parse::<u64>().map(Some)
    };
    match seed {
        Ok(seed) => {
            panel.seed_error = None;
            if settings.seed != seed {
                settings.seed = seed;
            }
        }
        Err(_) => panel.seed_error = Some("Seed must fit an unsigned 64-bit integer."),
    }
}

/// Reflects settings edits into the panel without overwriting focused seed text.
pub(super) fn sync_settings(
    settings: HexSettings,
    mut edits: MessageReader<AssetEvent<HexPathsSettings>>,
    focus: Res<InputFocus>,
    mut commands: Commands,
    sliders: Query<(Entity, Ref<SettingSlider>, &SliderValue)>,
    mut seeds: Query<(Entity, &mut EditableText, Ref<SeedInput>)>,
) {
    let changed = edits.read().count() > 0 || settings.collection().is_changed();
    let settings = r!(settings.paths());
    for (entity, field, value) in &sliders {
        let setting = (field.read)(settings);
        if (changed || field.is_added()) && value.0 != setting {
            commands.entity(entity).insert(SliderValue(setting));
        }
    }
    for (entity, mut input, marker) in &mut seeds {
        if (changed || marker.is_added()) && focus.get() != Some(entity) {
            let text = settings
                .seed
                .map(|seed| seed.to_string())
                .unwrap_or_default();
            if input.value().to_string() != text {
                input.queue_edit(TextEdit::SelectAll);
                input.queue_edit(TextEdit::Insert(text.into()));
            }
        }
    }
}

/// Displays seed, selected view, landscape opportunities, and current layout measures.
///
/// [`BoardStats`] exists even after a failed full generation; [`Board`] adds
/// window features, pace, crossing checks, and held/selected tease counts when
/// present. [`PanelState`] adds seed-edit or regeneration errors. Only rewrites
/// the text when its contents differ.
pub(super) fn update_stats(
    stats: Option<Res<BoardStats>>,
    board: Option<Res<Board>>,
    view: Res<HexPathsView>,
    panel: Res<PanelState>,
    mut texts: Query<&mut Text, With<StatsText>>,
) {
    let Some(stats) = stats else {
        return;
    };
    let mut value = format!(
        "View: {:?} | Focus: {:?} | Nodes revealed: {}\nSeed: {}",
        view.view, view.focus, view.reveal_nodes, stats.seed
    );
    if let Some(board) = board {
        let layout = &board.layout;
        let crossings = layout
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Crossing)
            .count();
        let window = &board.terrain.features;
        value.push_str(&format!(
            "\nWindow: relief {:.0} m | creek tiles {} | ford sites {} | tease spots {}\n\
             Layout {} | walk all {:.0} min ({:.1} km) | spawn to farthest {:.0} min\n\
             Crossings {crossings} ({} one-sided) | teases held {} of {} | unreachable {}",
            window.relief_m,
            window.creek_tiles,
            window.fords,
            window.tease_viewers,
            board.count,
            board.pace.walk_all_min,
            board.pace.trail_km,
            board.pace.farthest_min,
            board.one_sided,
            board.held.len(),
            layout.tease_pairs.len(),
            board.pace.unreachable,
        ));
    }
    if let Some(error) = &stats.error {
        value.push_str(&format!("\nGeneration error: {error}"));
    }
    if let Some(error) = panel.seed_error.or(panel.regeneration_error) {
        value.push_str(&format!("\n{error}"));
    }
    for mut text in &mut texts {
        if text.0 != value {
            text.0.clone_from(&value);
        }
    }
}

/// Keeps panel width consistent with the camera's reserved area.
pub(super) fn resize_panel(
    appearance: Res<HexPathsAppearance>,
    mut panels: Query<&mut Node, With<SettingsPanel>>,
) {
    for mut node in &mut panels {
        let width = px(appearance.panel_width);
        if node.width != width {
            node.width = width;
        }
    }
}
