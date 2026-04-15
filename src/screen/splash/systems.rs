use crate::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct SplashScreen;
impl Screen for SplashScreen {
    fn builder(builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder
    }
}
