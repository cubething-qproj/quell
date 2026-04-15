use std::any::TypeId;

use crate::prelude::*;

#[derive(States, Copy, Clone, Reflect, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ScreenLoadingState {
    Loading(TypeId),
    Ready(TypeId),
    Unloaded(TypeId),
}
impl ScreenLoadingState {
    fn finish_loading<S: Screen>(mut data: ScreenInfoMut<S>) {
        data.finish_loading();
    }
    fn register<S: Screen>(app: &mut App) {
        app.add_systems(OnEnter(Self::ready::<S>()), Self::finish_loading::<S>);
        app.add_systems(
            on_screen_unloaded::<S>(),
            |mut next: ResMut<NextState<Self>>| next.set(Self::unloaded::<S>()),
        );
        app.add_systems(
            on_screen_load::<S>(),
            |mut next: ResMut<NextState<Self>>| next.set(Self::unloaded::<S>()),
        );
    }
    pub fn ready<S: Screen>() -> Self {
        Self::Ready(TypeId::of::<S>())
    }
    pub fn loading<S: Screen>() -> Self {
        Self::Loading(TypeId::of::<S>())
    }
    pub fn unloaded<S: Screen>() -> Self {
        Self::Unloaded(TypeId::of::<S>())
    }
}

pub trait ScreenLoadingExt {
    fn register_screen_loading_state<S: Screen>(&mut self);
}

impl ScreenLoadingExt for App {
    fn register_screen_loading_state<S: Screen>(&mut self) {
        ScreenLoadingState::register::<S>(self);
    }
}
