use crate::prelude::*;

/// Allows for configuration of the application. When the "dev" feature is set,
/// this should be handled via command line arguments. Otherwise, it is kept as the
/// default value.
#[derive(Resource, Clone, Debug)]
pub struct AppSettings {
    /// Registered screen name to start on. `None` starts on [`SplashScreen`].
    pub initial_screen: Option<String>,
    pub use_physics: bool,
}
#[allow(clippy::derivable_impls)]
impl Default for AppSettings {
    fn default() -> Self {
        #[cfg(not(test))]
        {
            Self {
                initial_screen: Default::default(),
                use_physics: true,
            }
        }
        #[cfg(test)]
        {
            Self {
                initial_screen: Default::default(),
                use_physics: false,
            }
        }
    }
}

/// The main exported plugin for the application.
#[derive(Default, Clone)]
pub struct AppPlugin {
    pub settings: AppSettings,
}
impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().insert_resource(self.settings.clone());
        app.add_plugins((ScreenPlugin, crate::service::plugin, crate::screen::plugin))
            .insert_resource(self.settings.clone());
        app.insert_resource(self.settings.initial_screen());
    }
}

impl AppSettings {
    fn initial_screen(&self) -> InitialScreen {
        self.initial_screen
            .clone()
            .map_or_else(InitialScreen::new::<SplashScreen>, InitialScreen::from_name)
    }
}
