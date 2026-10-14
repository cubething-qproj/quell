//! The splash screen definition.

use crate::prelude::*;

/// Placeholder splash that switches straight to [`WorldScreen`] once ready.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct SplashScreen;
impl Screen for SplashScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::OnReady, |mut commands: Commands| {
            commands.trigger(switch_to_screen::<WorldScreen>());
        });
        builder
    }
}
