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
    // Jackdaw's embedded Play streams frames through a `Readback` entity it spawns
    // lazily; keep it alive across screen changes.
    app.register_persistent_type::<bevy::render::gpu_readback::Readback>(
        PersistentTypeScope::Anywhere,
    );
    app.run();
}
