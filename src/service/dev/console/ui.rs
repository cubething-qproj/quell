//! A q_shell terminal overlay for development.
//! Open and close with Ctrl+`.

use crate::prelude::*;
use bevy::app::Propagate;
use q_shell::prelude::*;

/// The dev console overlay, rendering `terminal`. Closed when its shell exits.
#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub(crate) struct Console {
    terminal: Entity,
}

impl Console {
    /// Spawns a visible console with a new terminal and shell, and routes
    /// keyboard input to it.
    fn spawn(commands: &mut Commands) {
        // TODO: Dev overlay should be put into a more generalized editor mod
        let terminal = commands
            .spawn((Persistent, Name::new("Console terminal"), Terminal))
            .id();
        commands.spawn((
            Persistent,
            Name::new("Console shell"),
            Shell::<TerminalIoEndpoint>::new(terminal),
        ));
        commands.spawn((
            Propagate(Persistent),
            Name::new("Dev overlay"),
            Console { terminal },
            Visibility::Visible,
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
                    Node {
                        width: percent(100),
                        height: percent(100),
                        ..Default::default()
                    },
                    BackgroundColor(LinearRgba::new(0., 0., 0., 0.8).into()),
                    VtUi::new(terminal),
                )],
            )],
        ));
        commands.insert_resource(ActiveShellKeyboardInput::new(terminal));
    }
}

/// Shows or hides the console with Ctrl+`, opening a new one if none exists.
/// The console's terminal receives keyboard input only while it is visible.
fn toggle_console(
    keys: Res<ButtonInput<KeyCode>>,
    mut consoles: Query<(&Console, &mut Visibility)>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Backquote)
        && (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
    {
        if consoles.is_empty() {
            Console::spawn(&mut commands);
            debug!("Opening new dev console.");
        }
        for (console, mut visibility) in consoles.iter_mut() {
            match *visibility {
                Visibility::Hidden => {
                    *visibility = Visibility::Visible;
                    commands.insert_resource(ActiveShellKeyboardInput::new(console.terminal));
                    debug!("Opening dev console.");
                }
                Visibility::Visible => {
                    *visibility = Visibility::Hidden;
                    commands.remove_resource::<ActiveShellKeyboardInput>();
                    debug!("Closing dev console.");
                }
                _ => {
                    warn!("Got unsupported visibility: Inherited");
                }
            }
        }
    }
}

/// Despawns a console and its terminal once the terminal's shell has exited.
fn close_console(
    remove: On<Remove, ShellTarget<TerminalIoEndpoint>>,
    consoles: Query<(Entity, &Console)>,
    active: Option<Res<ActiveShellKeyboardInput>>,
    mut commands: Commands,
) {
    for (overlay, console) in consoles.iter() {
        if console.terminal == remove.entity {
            commands.entity(overlay).despawn();
            commands.entity(console.terminal).try_despawn();
            if active
                .as_ref()
                .is_some_and(|active| active.terminal() == console.terminal)
            {
                commands.remove_resource::<ActiveShellKeyboardInput>();
            }
            debug!("Dev console shell exited; closing console.");
        }
    }
}

pub fn plugin(app: &mut App) {
    // The terminal and shell spawn these unparented, so `Propagate` can't reach them.
    // This exempts them app-wide: a non-Persistent terminal or shell would leak
    // them on screen cleanup, since q_screens unlinks rather than despawns them.
    app.register_persistent_type::<VtCharWidth>(PersistentTypeScope::TopLevel)
        .register_persistent_type::<VtLine>(PersistentTypeScope::TopLevel)
        .register_persistent_type::<VtRow>(PersistentTypeScope::TopLevel)
        .register_persistent_type::<ShellJob>(PersistentTypeScope::TopLevel);
    app.add_systems(PostUpdate, toggle_console);
    app.add_observer(close_console);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_exit_closes_the_console_and_its_terminal() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ShellPlugin::default()))
            .add_observer(close_console);
        let world = app.world_mut();
        Console::spawn(&mut world.commands());
        world.flush();
        let console = world
            .query::<&Console>()
            .single(world)
            .expect("one console");
        let terminal = console.terminal;
        let shell = world
            .query_filtered::<Entity, With<ShellProcess>>()
            .single(world)
            .expect("one shell");

        // ^D exits the shell by removing its process.
        world.entity_mut(shell).remove::<Process>();
        app.update();

        let world = app.world_mut();
        assert!(world.query::<&Console>().iter(world).next().is_none());
        assert!(world.get_entity(terminal).is_err());
        assert!(!world.contains_resource::<ActiveShellKeyboardInput>());
    }
}
