//! The application plugin and its launch settings.

use crate::prelude::*;

/// Launch settings passed to [`AppPlugin`].
///
/// The binary reads these from command-line arguments in `dev` builds and
/// uses their defaults otherwise.
#[derive(Resource, Clone, Debug, Default, Reflect)]
pub struct AppSettings {
    /// Registered screen name to start on. `None` starts on [`SplashScreen`].
    pub initial_screen: Option<String>,
}

/// Registers screens and services, then selects the initial screen from [`AppSettings`].
#[derive(Default, Clone)]
pub struct AppPlugin {
    /// Inserted as a resource and used to pick the initial screen.
    pub settings: AppSettings,
}
impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().insert_resource(self.settings.clone());
        app.add_plugins((ScreenPlugin, crate::service::plugin, crate::screen::plugin));
        app.insert_resource(self.settings.initial_screen());
    }
}

impl AppSettings {
    /// Selects [`SplashScreen`] when no name is given; otherwise requests the named screen.
    fn initial_screen(&self) -> InitialScreen {
        self.initial_screen
            .clone()
            .map_or_else(InitialScreen::new::<SplashScreen>, InitialScreen::from_name)
    }
}
