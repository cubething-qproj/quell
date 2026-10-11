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

/// Displays the seed and preserves useful errors while editing.
pub(super) fn update_stats(
    stats: Option<Res<BoardStats>>,
    panel: Res<PanelState>,
    mut texts: Query<&mut Text, With<StatsText>>,
) {
    let Some(stats) = stats else {
        return;
    };
    let mut value = format!("Seed: {}", stats.seed);
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
