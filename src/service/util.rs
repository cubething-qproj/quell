//! Helpers for building services.

use crate::prelude::*;
use bevy::ecs::{
    schedule::{IntoScheduleConfigs, ScheduleConfigs},
    system::ScheduleSystem,
};

/// Configured systems that a screen installs into one of its schedules.
///
/// Wrap the service's systems and ordering with [`Self::new`], then pass
/// [`Self::take`]'s result to [`ScreenScopeBuilder::add_systems`]. This wrapper
/// does not install systems or add run conditions itself.
#[derive(Deref, DerefMut)]
pub struct ServiceSystems(pub(crate) ScheduleConfigs<ScheduleSystem>);
impl ServiceSystems {
    /// Wraps systems, with any ordering already applied.
    pub fn new<M>(systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> Self {
        Self(systems.into_configs())
    }
    /// Unwraps the systems for `add_systems`.
    pub fn take(self) -> ScheduleConfigs<ScheduleSystem> {
        self.0
    }
}
