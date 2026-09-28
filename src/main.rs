use quell::prelude::*;

mod clap;

fn main() {
    let settings = clap::parse_args();

    let mut app = App::new();
    app.add_plugins((
        // Headless for the editor's schema query and embedded Play; unchanged otherwise.
        jackdaw_runtime::maybe_windowless(DefaultPlugins),
        quell::AppPlugin { settings },
        jackdaw_runtime::JackdawPlugin,
    ));
    app.run();
}
