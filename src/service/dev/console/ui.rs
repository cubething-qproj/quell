// add a bevy-ui based console wrapper here.
// open and close with `

use crate::prelude::*;
use bevy::input_focus::InputFocus;
use q_term::prelude::*;

fn toggle_console(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    mut consoles: Query<(Entity, &mut Visibility), With<Console>>,
) {
    if keys.just_pressed(KeyCode::Backquote)
        && (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
    {
        for (id, mut visibility) in consoles.iter_mut() {
            match *visibility {
                Visibility::Hidden => {
                    *visibility = Visibility::Visible;
                    focus.set(id);
                    debug!("Opening and focusing dev console.")
                }
                Visibility::Visible => {
                    *visibility = Visibility::Hidden;
                    if let Some(focus_id) = focus.0
                        && focus_id == id
                    {
                        focus.clear()
                    }
                    debug!("Closing dev console.")
                }
                _ => {
                    warn!("Got unsupported visibility: Inherited");
                }
            }
        }
    }
}

// this should really be an observer but AcquireFocus isn't publically accessible
fn on_focus(
    focus: Res<InputFocus>,
    console: Query<Entity, With<ConsoleUiSettings>>,
    mut commands: Commands,
) {
    if !focus.is_changed() {
        return;
    }
    let focus_entity = focus.get();
    if let Some(id) = focus_entity
        && console.get(id).is_ok()
    {
        commands.entity(id).insert(ConsoleUiSettings {
            font_color: LinearRgba::new(1., 1., 1., 1.).into(),
            background_color: LinearRgba::new(0., 0., 0., 0.8).into(),
        });
    } else {
        console.iter().for_each(|entity| {
            commands.entity(entity).insert(ConsoleUiSettings {
                font_color: LinearRgba::new(1., 1., 1., 0.5).into(),
                background_color: LinearRgba::new(0., 0., 0., 0.4).into(),
            });
        });
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, (toggle_console, on_focus).chain());
    app.add_systems(Startup, |mut commands: Commands| {
        // TODO: Dev overlay should be put into a more generalized editor mod
        commands.spawn((
            Persistent,
            Name::new("Dev overlay"),
            Node {
                width: vw(100),
                height: vh(100),
                ..Default::default()
            },
            children![(
                Name::new("Console wrapper"),
                Node {
                    width: percent(100),
                    height: percent(33),
                    top: percent(67),
                    ..Default::default()
                },
                children![(
                    Name::new("Console"),
                    Visibility::Hidden,
                    ConsolePrompt("\n\n> ".into()),
                    Console,
                    ConsoleAssetHandle::<ConsoleHistory>::new("dev/console.history".to_string()),
                    ConsoleAssetHandle::<ConsoleEnvVars>::new("dev/console.env".to_string()),
                )],
            )],
        ));
    });
}
