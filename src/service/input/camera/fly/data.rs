use crate::prelude::*;

/// Predicts capture before upstream flight runs, including the release frame.
#[derive(Component, Default)]
pub(crate) struct FreeCameraInput {
    pub(super) armed: bool,
    pub(super) toggled: bool,
}

impl FreeCameraInput {
    pub(crate) fn reset(&mut self, state: &mut FreeCameraState) {
        self.armed = false;
        self.toggled = false;
        Self::stop(state);
    }

    pub(super) fn stop(state: &mut FreeCameraState) {
        state.enabled = false;
        state.velocity = Vec3::ZERO;
        state.rotation_curve = None;
    }
}
