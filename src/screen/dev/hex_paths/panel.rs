use super::data::*;
use crate::prelude::*;
use bevy::{
    feathers::{
        controls::{
            FeathersButton, FeathersNumberInput, FeathersTextInput, FeathersTextInputContainer,
            NumberFormat, NumberInputValue, UpdateNumberInput,
        },
        display::{label, label_small},
        theme::ThemeBackgroundColor,
        tokens,
    },
    input_focus::{InputFocus, tab_navigation::TabGroup},
    text::{EditableText, EditableTextFilter, TextEdit, TextEditChange},
    ui_widgets::{Activate, ScrollArea, ValueChange},
};

/// Scrollable debug panel, with typed controls for all generation settings.
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
                    on(|_: On<Activate>, mut settings: ResMut<HexPathsSettings>| { settings.set_changed(); })),
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
                label_small("Camera focus"),
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
                    on(|_: On<Activate>, stats: Option<Res<BoardStats>>, mut settings: ResMut<HexPathsSettings>| {
                        if let Some(stats) = stats { settings.seed = Some(stats.seed); }
                    })),
                integer("Columns", |s| s.columns, |s, v| s.columns = v),
                integer("Rows", |s| s.rows, |s, v| s.rows = v),
                label("Terrain"),
                decimal("Hill size (hexes)", |s| s.scale, |s, v| s.scale = v),
                decimal("Relief (edge lengths)", |s| s.relief, |s, v| s.relief = v),
                decimal("Roughness (0–1)", |s| s.roughness, |s, v| s.roughness = v),
                decimal("Ridges (0–1)", |s| s.ridges, |s, v| s.ridges = v),
                label("Nodes"),
                integer("Nodes", |s| s.nodes, |s, v| s.nodes = v),
                decimal("Prefer flat ground (spread)", |s| s.spread, |s, v| s.spread = v),
                decimal("Bench view: quiet 0 – overlook 1", |s| s.view, |s, v| s.view = v),
            ]
        )]
    }
}

/// Row for a non-negative integer, committed on Enter or focus loss.
fn integer(
    caption: &'static str,
    read: fn(&HexPathsSettings) -> u32,
    write: fn(&mut HexPathsSettings, u32),
) -> impl Scene {
    bsn! {
        Node { align_items: AlignItems::Center, column_gap: px(8) }
        Children [
            (label_small(caption) Node { width: percent(65) }),
            (@FeathersNumberInput { @number_format: NumberFormat::I64 }
                template(move |_| Ok(NumberSetting::<u32> { read, write }))
                on(set_integer)),
        ]
    }
}

/// Row for a floating-point setting, committed on Enter or focus loss.
fn decimal(
    caption: &'static str,
    read: fn(&HexPathsSettings) -> f32,
    write: fn(&mut HexPathsSettings, f32),
) -> impl Scene {
    bsn! {
        Node { align_items: AlignItems::Center, column_gap: px(8) }
        Children [
            (label_small(caption) Node { width: percent(65) }),
            (@FeathersNumberInput
                template(move |_| Ok(NumberSetting::<f32> { read, write }))
                on(set_decimal)),
        ]
    }
}

/// Applies an integer input only when its final value fits the setting's type.
fn set_integer(
    change: On<ValueChange<i64>>,
    fields: Query<&NumberSetting<u32>>,
    mut settings: ResMut<HexPathsSettings>,
) {
    if change.is_final
        && let Ok(value) = u32::try_from(change.value)
        && let Ok(field) = fields.get(change.source)
    {
        let mut next = settings.clone();
        (field.write)(&mut next, value);
        settings.set_if_neq(next);
    }
}

/// Applies a final decimal input; the generator reports invalid settings.
fn set_decimal(
    change: On<ValueChange<f32>>,
    fields: Query<&NumberSetting<f32>>,
    mut settings: ResMut<HexPathsSettings>,
) {
    if change.is_final
        && let Ok(field) = fields.get(change.source)
    {
        let mut next = settings.clone();
        (field.write)(&mut next, change.value);
        settings.set_if_neq(next);
    }
}

/// Parses a seed without narrowing it through a signed integer or float.
fn set_seed(
    change: On<TextEditChange>,
    inputs: Query<&EditableText, With<SeedInput>>,
    mut settings: ResMut<HexPathsSettings>,
    mut panel: ResMut<PanelState>,
) {
    let Ok(input) = inputs.get(change.event_target()) else {
        return;
    };
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
    settings: Res<HexPathsSettings>,
    focus: Res<InputFocus>,
    mut commands: Commands,
    integers: Query<(Entity, Ref<NumberSetting<u32>>)>,
    decimals: Query<(Entity, Ref<NumberSetting<f32>>)>,
    mut seeds: Query<(Entity, &mut EditableText, Ref<SeedInput>)>,
) {
    for (entity, field) in &integers {
        if settings.is_changed() || field.is_added() {
            commands.trigger(UpdateNumberInput {
                entity,
                value: NumberInputValue::I64((field.read)(&settings) as i64),
            });
        }
    }
    for (entity, field) in &decimals {
        if settings.is_changed() || field.is_added() {
            commands.trigger(UpdateNumberInput {
                entity,
                value: NumberInputValue::F32((field.read)(&settings)),
            });
        }
    }
    for (entity, mut input, marker) in &mut seeds {
        if (settings.is_changed() || marker.is_added()) && focus.get() != Some(entity) {
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

/// Displays the view and seed, and preserves useful errors while editing.
pub(super) fn update_stats(
    stats: Option<Res<BoardStats>>,
    view: Res<HexPathsView>,
    panel: Res<PanelState>,
    mut texts: Query<&mut Text, With<StatsText>>,
) {
    let Some(stats) = stats else {
        return;
    };
    let mut value = format!(
        "View: {:?} | Focus: {:?}\nSeed: {}",
        view.view, view.focus, stats.seed
    );
    if let Some(error) = &stats.error {
        value.push_str(&format!("\nGeneration error: {error}"));
    }
    if let Some(error) = panel.seed_error {
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
