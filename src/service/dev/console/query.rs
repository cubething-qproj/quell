//! The `q` console command: queries the World with [`q_query_lang`].
//!
//! Quote the query so the shell passes it through whole:
//!
//! ```text
//! $ q '(@ Name ?Transform)[PlayerController]'
//! ```

use q_query_lang::QueryPlan;
use q_shell::prelude::*;

/// Marks a `q` invocation.
#[derive(Component, Clone, Copy, Debug, Default, Eq, Hash, PartialEq, ProgramLabel)]
#[program_label("q")]
struct QueryProgram;

/// Runs each invocation's `argv` as one query, printing the result to stdout
/// or the error to stderr. Exclusive, because executing needs `&mut World`.
fn run_query(world: &mut World) {
    let invocations: Vec<(Entity, String)> = world
        .query_filtered::<(Entity, &Process), With<QueryProgram>>()
        .iter(world)
        .map(|(process, info)| (process, info.argv.join(" ")))
        .collect();
    let registry = world.resource::<AppTypeRegistry>().clone();
    for (process, query) in invocations {
        let result = QueryPlan::new(&query, &registry.read())
            .map_err(|error| error.to_string())
            .and_then(|plan| plan.execute(world).map_err(|error| error.to_string()));
        let (write, code) = match result {
            Ok(result) => (
                ProcessWriteMsg::stdout(process, terminal_bytes(&result.to_string())),
                0,
            ),
            Err(error) => (
                ProcessWriteMsg::stderr(process, terminal_bytes(&format!("q: {error}\n"))),
                1,
            ),
        };
        world.write_message(write);
        world.entity_mut(process).exit(code);
    }
}

/// The terminal needs `\r\n` line endings.
fn terminal_bytes(text: &str) -> Vec<u8> {
    text.replace('\n', "\r\n").into_bytes()
}

pub fn plugin(app: &mut App) {
    app.program::<QueryProgram>().add_systems(Update, run_query);
}

#[cfg(test)]
mod tests {
    use bevy::platform::collections::HashMap;

    use super::*;

    #[derive(Resource, Default)]
    struct Exits(Vec<ExitStatus>);

    fn run(query: &str) -> Vec<ExitStatus> {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ShellPlugin::default(), plugin))
            .init_resource::<Exits>()
            .add_observer(|exited: On<ProcessExited>, mut exits: ResMut<Exits>| {
                exits.0.push(exited.status);
            });
        app.world_mut().spawn(Name::new("Player"));
        app.world_mut().spawn(Process {
            prog: QueryProgram.intern(),
            signal_overrides: HashMap::new(),
            argv: vec![query.to_owned()],
            environ: HashMap::new(),
        });
        app.update();
        app.world_mut().remove_resource::<Exits>().unwrap().0
    }

    #[test]
    fn a_valid_query_exits_successfully() {
        assert_eq!(run("#Player"), [ExitStatus::Code(0)]);
    }

    #[test]
    fn an_invalid_query_exits_with_an_error() {
        assert_eq!(run("(@ Name"), [ExitStatus::Code(1)]);
    }
}
