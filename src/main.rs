use quell::prelude::*;
#[cfg(not(feature = "dev"))]
use quell::AppSettings;

#[cfg(feature = "dev")]
mod clap;

fn main() {
    #[cfg(feature = "dev")]
    let settings = clap::parse_args();

    #[cfg(not(feature = "dev"))]
    let settings = AppSettings::default();

    let mut app = App::new();
    app.add_plugins((
        // Headless for the editor's schema query and embedded Play; unchanged otherwise.
        jackdaw_runtime::maybe_windowless(DefaultPlugins),
        quell::AppPlugin { settings },
        jackdaw_runtime::JackdawPlugin,
    ));
    app.run();
}
