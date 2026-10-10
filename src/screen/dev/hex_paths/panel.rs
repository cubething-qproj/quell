use super::data::*;
use crate::prelude::*;
use bevy::{
    feathers::{
        controls::{
            FeathersButton, FeathersCheckbox, FeathersNumberInput, FeathersSlider,
            FeathersTextInput, FeathersTextInputContainer, NumberFormat, NumberInputValue,
            UpdateNumberInput,
        },
        display::{label, label_small},
        theme::ThemeBackgroundColor,
        tokens,
    },
    input_focus::{InputFocus, tab_navigation::TabGroup},
    text::{EditableText, EditableTextFilter, TextEdit, TextEditChange},
    ui::Checked,
    ui_widgets::{Activate, ScrollArea, SliderPrecision, SliderStep, SliderValue, ValueChange},
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
                toggle("Show all paths", ViewToggle::Paths),
                toggle("Show node traits", ViewToggle::NodeTraits),
                label_small("Key: gold | NPC: blue | Fear: red | Empty: violet"),
                label_small("Walking step duration (seconds)"),
                (@FeathersSlider { @min: 0.05, @max: 2.0, @value: 0.5 }
                    StepDuration SliderStep(0.05) SliderPrecision(2)
                    on(|change: On<ValueChange<f32>>, mut view: ResMut<HexPathsView>| {
                        view.step_duration = change.value;
                    })),
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
                (label_small("") SpawnText),
                (Node { column_gap: px(4) } Children [
                    (@FeathersButton { @caption: bsn! { label("Center") } }
                        on(|_: On<Activate>, mut s: ResMut<HexPathsSettings>| { s.spawn = SpawnPlacement::Center; })),
                    (@FeathersButton { @caption: bsn! { label("Random") } }
                        on(|_: On<Activate>, mut s: ResMut<HexPathsSettings>| { s.spawn = SpawnPlacement::Random; })),
                    (@FeathersButton { @caption: bsn! { label("Fixed") } }
                        on(|_: On<Activate>, mut s: ResMut<HexPathsSettings>| {
                            s.spawn = SpawnPlacement::Fixed { column: s.columns / 2, row: s.rows / 2 };
                        })),
                ]),
                integer("Fixed spawn column", |s| match s.spawn {
                    SpawnPlacement::Fixed { column, .. } => column, _ => s.columns / 2,
                }, |s, column| {
                    let row = match s.spawn { SpawnPlacement::Fixed { row, .. } => row, _ => s.rows / 2 };
                    s.spawn = SpawnPlacement::Fixed { column, row };
                }),
                integer("Fixed spawn row", |s| match s.spawn {
                    SpawnPlacement::Fixed { row, .. } => row, _ => s.rows / 2,
                }, |s, row| {
                    let column = match s.spawn { SpawnPlacement::Fixed { column, .. } => column, _ => s.columns / 2 };
                    s.spawn = SpawnPlacement::Fixed { column, row };
                }),
                integer("Key locations", |s| s.deck.key_locations, |s, v| s.deck.key_locations = v),
                integer("NPCs", |s| s.deck.npcs, |s, v| s.deck.npcs = v),
                integer("Fear events", |s| s.deck.fears, |s, v| s.deck.fears = v),
                integer("Empty nodes", |s| s.deck.empty, |s, v| s.deck.empty = v),
                integer("Node spacing", |s| s.min_node_distance, |s, v| s.min_node_distance = v),
                integer("Spawn spacing", |s| s.min_spawn_distance, |s, v| s.min_spawn_distance = v),
                integer("Spawn spokes minimum", |s| s.min_spawn_spokes, |s, v| s.min_spawn_spokes = v),
                integer("Spawn spokes maximum", |s| s.max_spawn_spokes, |s, v| s.max_spawn_spokes = v),
                decimal("Noise (0–10)", |s| s.noise, |s, v| s.noise = v),
                decimal("Sharp shape weight (0–10)", |s| s.shape_weights.sharp, |s, v| s.shape_weights.sharp = v),
                decimal("Wide shape weight (0–10)", |s| s.shape_weights.wide, |s, v| s.shape_weights.wide = v),
                decimal("Straight shape weight (0–10)", |s| s.shape_weights.straight, |s, v| s.shape_weights.straight = v),
                decimal("Junction chance (0–1)", |s| s.junction_chance, |s, v| s.junction_chance = v),
                integer("Branch length minimum", |s| s.min_branch_length, |s, v| s.min_branch_length = v),
                integer("Branch length maximum", |s| s.max_branch_length, |s, v| s.max_branch_length = v),
                decimal("Branch join chance (0–1)", |s| s.branch_join_chance, |s, v| s.branch_join_chance = v),
                integer("Target path tiles", |s| s.target_paths, |s, v| s.target_paths = v),
                decimal("Target tolerance (0–1)", |s| s.target_tolerance, |s, v| s.target_tolerance = v),
                flag("No dead ends", |s| s.no_dead_ends, |s, v| s.no_dead_ends = v),
                flag("Every node on a loop", |s| s.node_loops, |s, v| s.node_loops = v),
                integer("Minimum loop length", |s| s.min_loop, |s, v| s.min_loop = v),
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

/// Checkbox for a boolean generation setting.
fn flag(
    caption: &'static str,
    read: fn(&HexPathsSettings) -> bool,
    write: fn(&mut HexPathsSettings, bool),
) -> impl Scene {
    bsn! {
        @FeathersCheckbox { @caption: bsn! { label_small(caption) } }
        template(move |_| Ok(FlagSetting { read, write }))
        on(set_flag)
    }
}

/// Checkbox for a view option, independent of generation.
fn toggle(caption: &'static str, option: ViewToggle) -> impl Scene {
    bsn! {
        @FeathersCheckbox { @caption: bsn! { label_small(caption) } }
        template(move |_| Ok(option))
        on(set_toggle)
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

/// Applies a checkbox to its generation setting.
fn set_flag(
    change: On<ValueChange<bool>>,
    fields: Query<&FlagSetting>,
    mut settings: ResMut<HexPathsSettings>,
) {
    if let Ok(field) = fields.get(change.source) {
        let mut next = settings.clone();
        (field.write)(&mut next, change.value);
        settings.set_if_neq(next);
    }
}

/// Applies a checkbox to its view option.
fn set_toggle(
    change: On<ValueChange<bool>>,
    toggles: Query<&ViewToggle>,
    mut view: ResMut<HexPathsView>,
) {
    if let Ok(toggle) = toggles.get(change.source) {
        match toggle {
            ViewToggle::Paths => view.show_paths = change.value,
            ViewToggle::NodeTraits => view.show_node_traits = change.value,
        }
    }
}

/// Reflects settings edits into the panel without overwriting focused seed text.
pub(super) fn sync_settings(
    settings: Res<HexPathsSettings>,
    focus: Res<InputFocus>,
    mut commands: Commands,
    integers: Query<(Entity, Ref<NumberSetting<u32>>)>,
    decimals: Query<(Entity, Ref<NumberSetting<f32>>)>,
    flags: Query<(Entity, &FlagSetting, Has<Checked>)>,
    mut seeds: Query<(Entity, &mut EditableText, Ref<SeedInput>)>,
    mut spawn_text: Query<&mut Text, With<SpawnText>>,
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
    for (entity, field, checked) in &flags {
        let value = (field.read)(&settings);
        if value != checked {
            if value {
                commands.entity(entity).insert(Checked);
            } else {
                commands.entity(entity).remove::<Checked>();
            }
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
    for mut text in &mut spawn_text {
        let value = format!("Spawn: {:?}", settings.spawn);
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Reflects view edits, including inspector changes, into checkboxes and slider.
pub(super) fn sync_view(
    view: Res<HexPathsView>,
    mut commands: Commands,
    toggles: Query<(Entity, &ViewToggle, Has<Checked>)>,
    duration: Query<(Entity, &SliderValue), With<StepDuration>>,
) {
    for (entity, toggle, checked) in &toggles {
        let value = match toggle {
            ViewToggle::Paths => view.show_paths,
            ViewToggle::NodeTraits => view.show_node_traits,
        };
        if value != checked {
            if value {
                commands.entity(entity).insert(Checked);
            } else {
                commands.entity(entity).remove::<Checked>();
            }
        }
    }
    for (entity, slider) in &duration {
        if slider.0 != view.step_duration {
            commands
                .entity(entity)
                .insert(SliderValue(view.step_duration));
        }
    }
}

/// Displays the actual distribution and preserves useful errors while editing.
pub(super) fn update_stats(
    stats: Option<Res<BoardStats>>,
    settings: Res<HexPathsSettings>,
    panel: Res<PanelState>,
    mut texts: Query<&mut Text, With<StatsText>>,
) {
    let Some(stats) = stats else {
        return;
    };
    let window = settings.target_window();
    let mut value = format!(
        "Seed: {}\nAttempts: {}\nPath tiles: {} (target {}–{})\n2 spokes: {} | 3 spokes: {}\nSharp: {} | Wide: {} | Straight: {}",
        stats.seed,
        stats.attempts,
        stats.two_spoke + stats.three_spoke,
        window.start(),
        window.end(),
        stats.two_spoke,
        stats.three_spoke,
        stats.shapes[0],
        stats.shapes[1],
        stats.shapes[2]
    );
    if let Some(error) = &stats.error {
        value.push_str(&format!("\nGeneration error: {error:?}"));
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
