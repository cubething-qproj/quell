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
    app.add_plugins((DefaultPlugins, quell::AppPlugin { settings }));
    app.run();
}
